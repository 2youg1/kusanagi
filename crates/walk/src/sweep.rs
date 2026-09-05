// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Taking a ward whole, so that a read never names an address.
//!
//! A reader that asked for `address` handed the host the one relation this
//! network exists to hide — the writer of an address and its reader, paired on
//! the host's own access log — and Tor only moved that pair from an IP to an
//! exit (`ARCHITECTURE.md` §8 D-20). A sweep asks for something else: **every
//! object in one period of one ward**, which is a function of public data and
//! of nothing the reader knows. Every reader of a ward makes the same requests,
//! so the host can say a ward was read and not by whom for what.
//!
//! A sweep is a pass over the bins of one ward from one period through another,
//! handed out one bin at a time. Whoever drives it matches the objects of the
//! bin in hand against however many lanes it is walking — one for a channel,
//! every member's for a room — and lets the bin go before taking the next.
//! **One period is held at a time** — law 2 — so catching up after a week costs
//! a week of listings and never a week of memory, whatever the number of lanes.
//!
//! **Only what the bin lists beyond the last sweep is fetched.** The keys the
//! bin listed last time are kept beside the period (`site::sweeps`), and a poll
//! fetches the ones that are new; a bin listed exactly as before costs one
//! request and no bytes. The decision is a function of two listings the host
//! served, so every reader of a ward that saw them makes it identically, and
//! the host learns nothing from the fetches it does not see.
//!
//! Heights are matched in order, and that order is what lets one period be
//! enough: a writer files each segment in the period it was written, so height
//! `h+1` is never in an earlier bin than `h` unless the writer's clock ran
//! backwards across a period boundary. That is the honest edge of this design,
//! written in `kusanagi-SPEC.md` §11 rather than papered over.

use std::collections::HashMap;

use kusanagi_kernel::{Bin, DropAddr, Listing, Object, Period, Sweep, Ward, Waypoint};
use kusanagi_site::Swept;

use kusanagi_door::Complaint;

/// How many hex digits of its ward a reader names when it has not said.
///
/// Four is one ward: the smallest crowd and the least bandwidth. `kusanagi
/// sweep --digits` records another width on the site, and the reader alone
/// decides; nobody else is told.
pub const DIGITS: u8 = Ward::DIGITS;

/// The most objects one bin is allowed to hold before a reader gives up on it.
///
/// A hostile host, or a crowded ward, can fill a bin with objects a reader must
/// download to find its own. Two hundred and fifty-six drops is thirty-two
/// mebibytes, and it is the cost of one *catch-up* rather than of one poll: a
/// poll fetches only what the bin lists beyond the last sweep. **Filling a bin
/// can cause a denial and never a leak**: the reader still asked for the whole
/// bin and named nothing in it.
pub const CAP: usize = 256;

/// One bin as a sweep took it: what the host listed, and the sealed bytes of
/// every object listed beyond the last sweep, by address.
pub struct Taken {
    /// What the bin listed, for whoever writes down how far the sweep got.
    pub seen: Swept,
    /// The sealed bytes fetched, keyed by address so a lane can claim its own.
    pub held: HashMap<DropAddr, Vec<u8>>,
}

/// One reader's pass over the bins of its ward, from one period through another.
pub struct Sweeping<'a, P: Waypoint + Listing + Sync> {
    place: &'a P,
    ward: Ward,
    digits: u8,
    through: Period,
    /// What the last sweep saw, so that a bin listed the same is not taken twice.
    known: Option<Swept>,
    /// The next period to take; none once the last one has been.
    next: Option<Period>,
}

impl<'a, P: Waypoint + Listing + Sync> Sweeping<'a, P> {
    /// A sweep naming `digits` of `ward` on `place`, over every period from
    /// `since` through `through` inclusive, knowing what the last sweep saw.
    /// Nothing is asked of the host until a bin is.
    pub fn over(
        place: &'a P,
        ward: Ward,
        digits: u8,
        since: Period,
        through: Period,
        known: Option<Swept>,
    ) -> Self {
        Self {
            place,
            ward,
            digits,
            through,
            known,
            next: (since <= through).then_some(since),
        }
    }

    /// The next bin of the sweep, or none once every period through the last
    /// has been taken.
    ///
    /// # Errors
    ///
    /// [`Complaint::WardOverfull`] when the bin holds more than [`CAP`] objects,
    /// and whatever the host reports.
    pub fn take(&mut self) -> Result<Option<Taken>, Complaint> {
        let Some(period) = self.next else {
            return Ok(None);
        };
        // Past the last period of the sweep, or past the last period a `u64`
        // can count, there is nothing left to take.
        self.next = period
            .count()
            .checked_add(1)
            .map(Period::from_count)
            .filter(|after| *after <= self.through);
        self.bin(period).map(Some)
    }

    /// Lists one bin and takes what it lists beyond the last sweep of it — all
    /// of it, when this ward has never been swept in this period.
    fn bin(&self, period: Period) -> Result<Taken, Complaint> {
        let sweep = Sweep::of(Bin::new(period, self.ward), self.digits);
        // Narrowed here rather than trusted there: an adapter that lists too
        // much is corrected by the one authority on what a sweep covers.
        let listed: Vec<Object> = self
            .place
            .list(&sweep)?
            .into_iter()
            .filter(|object| sweep.holds(object))
            .collect();
        if listed.len() > CAP {
            return Err(Complaint::WardOverfull {
                ward: self.ward.to_string(),
                period: period.to_string(),
                objects: listed.len(),
            });
        }
        let seen = Swept::of(period, &listed);
        let wanted: Vec<&Object> = seen
            .objects
            .iter()
            .filter(|object| {
                !self
                    .known
                    .as_ref()
                    .is_some_and(|known| known.through == period && known.lists(object))
            })
            .collect();
        // In flight together: the host sees a bin being taken, not a sequence.
        let fetched: Vec<Result<Option<Vec<u8>>, Complaint>> = std::thread::scope(|scope| {
            let running: Vec<_> = wanted
                .iter()
                .map(|object| scope.spawn(move || Ok(self.place.get(object)?)))
                .collect();
            running
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(Complaint::Local {
                            action: "take a bin",
                            source: std::io::Error::other("a reader did not finish"),
                        })
                    })
                })
                .collect()
        });
        let mut held = HashMap::with_capacity(wanted.len());
        for (object, bytes) in wanted.iter().zip(fetched) {
            // Listed and then gone is an object the host expired between the two
            // requests, and not one of ours to miss: a drop that is not there
            // is a drop nobody can read.
            if let Some(bytes) = bytes? {
                held.insert(object.addr(), bytes);
            }
        }
        Ok(Taken { seen, held })
    }
}
