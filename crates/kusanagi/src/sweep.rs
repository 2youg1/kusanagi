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
//! What the walk sees is unchanged. It asks for the sealed bytes at a height,
//! as it always did; a [`Swept`] answers out of the bin it has taken rather
//! than by asking the host, and moves on to the next period when the height is
//! not in this one. **One period is held at a time** — law 2 — so catching up
//! after a week costs a week of listings and never a week of memory.
//!
//! **Only what the bin lists beyond the last sweep is fetched.** The keys the
//! bin listed last time are kept beside the period (`site::sweeps`), and a poll
//! fetches the ones that are new; a bin listed exactly as before costs one
//! request and no bytes. The decision is a function of two listings the host
//! served, so every reader of a ward that saw them makes it identically, and
//! the host learns nothing from the fetches it does not see.
//!
//! Heights are asked for in order, and that order is what lets one period be
//! enough: a writer files each segment in the period it was written, so height
//! `h+1` is never in an earlier bin than `h` unless the writer's clock ran
//! backwards across a period boundary. That is the honest edge of this design,
//! written in `kusanagi-SPEC.md` §11 rather than papered over.

use std::collections::HashMap;
use std::sync::Mutex;

use kusanagi_kernel::{Bin, DropAddr, Listing, Object, Period, Sweep, Ward, Waypoint};
use kusanagi_site::Swept;

use crate::lane::Lane;
use crate::source::Source;
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

/// One reader's pass over the bins of its ward, from one period through another.
///
/// Answers [`Source::sealed`] out of whatever period it currently holds and
/// loads the next when a height is not there. Interior mutability because a
/// source is shared with the walk by reference; the lock is held for the length
/// of one call and never across a request to the host.
pub struct Sweeping<'a, P: Waypoint + Listing + Sync> {
    place: &'a P,
    ward: Ward,
    digits: u8,
    through: Period,
    /// What the last sweep saw, so that a bin listed the same is not taken twice.
    known: Option<Swept>,
    cursor: Mutex<Cursor>,
}

/// Where a sweep stands: the objects of the period in hand, the next period to
/// load — none once the last one has been taken — and what the last listing
/// held, for the record.
struct Cursor {
    held: HashMap<DropAddr, Vec<u8>>,
    next: Option<Period>,
    listed: Option<Swept>,
}

impl<'a, P: Waypoint + Listing + Sync> Sweeping<'a, P> {
    /// A sweep naming `digits` of `ward` on `place`, over every period from
    /// `since` through `through` inclusive, knowing what the last sweep saw.
    /// Nothing is asked of the host until a height is.
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
            cursor: Mutex::new(Cursor {
                held: HashMap::new(),
                next: (since <= through).then_some(since),
                listed: None,
            }),
        }
    }

    /// What the last bin this sweep listed, for whoever writes down how far it
    /// got. `None` until a height has been asked for.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the sweep was abandoned mid-bin by a reader that
    /// panicked, which is a state nothing should trust.
    pub fn listed(&self) -> Result<Option<Swept>, Complaint> {
        Ok(self.cursor.lock().map_err(|_| abandoned())?.listed.clone())
    }

    /// Lists one bin and takes what it lists beyond the last sweep of it — all
    /// of it, when this lane has never swept this period.
    ///
    /// # Errors
    ///
    /// [`Complaint::WardOverfull`] when the bin holds more than [`CAP`] objects,
    /// and whatever the host reports.
    fn take(&self, period: Period) -> Result<(Swept, HashMap<DropAddr, Vec<u8>>), Complaint> {
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
        Ok((seen, held))
    }
}

impl<P: Waypoint + Listing + Sync> Source for Sweeping<'_, P> {
    /// The sealed bytes at `width` heights from `from`, matched by address
    /// against the bins of this sweep, loading the next period whenever the
    /// height is not in the one in hand.
    ///
    /// Stops filling at the first height found nowhere, because the walk stops
    /// there too and a height above it could only be reached by skipping one.
    fn sealed(
        &self,
        lane: &Lane,
        from: u64,
        width: usize,
    ) -> Result<Vec<Option<Vec<u8>>>, Complaint> {
        let mut cursor = self.cursor.lock().map_err(|_| abandoned())?;
        let mut found = Vec::with_capacity(width);
        for step in 0..width {
            let Some(index) = u64::try_from(step)
                .ok()
                .and_then(|step| from.checked_add(step))
            else {
                found.push(None);
                continue;
            };
            let address = lane.keys.address(index);
            let bytes = loop {
                if let Some(bytes) = cursor.held.remove(&address) {
                    break Some(bytes);
                }
                let Some(period) = cursor.next else {
                    break None;
                };
                let (seen, held) = self.take(period)?;
                cursor.held = held;
                cursor.listed = Some(seen);
                // Past the last period of the sweep, or past the last period a
                // `u64` can count, there is nothing left to load.
                cursor.next = period
                    .count()
                    .checked_add(1)
                    .map(Period::from_count)
                    .filter(|after| *after <= self.through);
            };
            let exhausted = bytes.is_none();
            found.push(bytes);
            if exhausted {
                found.resize(width, None);
                break;
            }
        }
        Ok(found)
    }
}

/// The failure of finding a sweep's lock poisoned.
fn abandoned() -> Complaint {
    Complaint::Local {
        action: "resume a sweep",
        source: std::io::Error::other("a sweep was abandoned mid-bin"),
    }
}
