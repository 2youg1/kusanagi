// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Reading one author's stream out of a waypoint, checking it as it goes.
//!
//! Four checks happen on the way, in this order, and none of them is optional:
//! the bytes must open under the key that address derives, they must decode to a
//! segment, that segment must be signed by the handle we expected, and it must
//! follow the one before it. A failure at any of them stops the walk — a chain
//! that has been interfered with is not a chain with a gap in it.
//!
//! **Where a walk starts is a privacy decision, not a performance one.** A walk
//! names every address it visits, out loud, to a host that is keeping an access
//! log; addresses derived to be unrelated stop being unrelated the moment one
//! connection asks for them in ascending order, back to back. So a reader that
//! starts at height zero every time hands the host the grouping that `seal`
//! exists to deny it, once per poll, for the whole history. A walk that only
//! owes the caller the head, or the segments above a height the caller already
//! holds, starts from the cairn this endpoint wrote last time instead — which
//! makes a poll name one address rather than all of them.
//!
//! What a resumed walk does not close is the first catch-up, which still visits
//! what it has never seen. **The order it visits them in is closed here.** A
//! catch-up fetches a bounded window at once instead of one address after the
//! next, so a host sees several requests in flight together rather than a chain
//! of request-then-next-request; the window is bounded, so law 2 holds and
//! memory does not grow with the stream, and **verification stays strictly in
//! order** — only the fetching is not.
//!
//! The window starts at one and doubles to [`WINDOW`], which is what keeps a poll
//! costing one request: an endpoint that is up to date asks for one address,
//! finds nothing, and stops. An endpoint a hundred segments behind asks in
//! eights. The ramp also blurs the live edge by up to a window, because a batch
//! that runs past the end of the stream has already asked for what is not there
//! — `ARCHITECTURE.md` §3 records that edge as followable, and this makes
//! following it approximate rather than exact.

use kusanagi_chain::{Cairn, Verifier};
use kusanagi_kernel::{ChainHead, DropAddr, Segment, SegmentError, Waypoint};
use kusanagi_seal::{Fit, open};
use kusanagi_site::Site;

use crate::lane::Lane;
use kusanagi_door::Complaint;

/// One segment, and the address it was found at.
pub struct Held {
    /// Where it was.
    pub address: DropAddr,
    /// What it was.
    pub segment: Segment,
}

/// A stream as it was found on a waypoint.
pub struct Walked {
    verifier: Verifier,
    held: Vec<Held>,
}

impl Walked {
    /// The verified head, absent when the stream has not started.
    ///
    /// After a resumed walk this is the head of the whole stream, not of what
    /// this walk fetched: the cairn it resumed from carries the rest.
    #[must_use]
    pub fn head(&self) -> Option<ChainHead> {
        self.verifier.head()
    }

    /// The segments this walk fetched, in order.
    ///
    /// A resumed walk holds only what it had not verified before. [`Reach`] is
    /// what guarantees that whatever the caller intends to show was fetched.
    #[must_use]
    pub fn held(&self) -> &[Held] {
        &self.held
    }

    /// The position reached, to be written down for the next walk.
    #[must_use]
    pub fn cairn(&self) -> Option<Cairn> {
        self.verifier.cairn()
    }

    /// Where this walk stands once `segment` is appended to it.
    ///
    /// A sender needs this. It walked to the head, built the next segment from
    /// that head, and watched the host accept it at an address that was empty —
    /// so the segment is verified by construction more strongly than reading it
    /// back would verify it. Without this the record would lag one behind the
    /// stream forever, and every send would pay an extra request to rediscover
    /// what it had just written.
    ///
    /// # Errors
    ///
    /// [`Complaint::Chain`] when `segment` does not follow this walk, which means
    /// the caller is appending to a chain other than the one it read.
    pub fn extended(&self, segment: &Segment) -> Result<Option<Cairn>, Complaint> {
        let mut verifier = self.verifier;
        verifier.accept(segment)?;
        Ok(verifier.cairn())
    }
}

/// What the caller of a walk actually needs out of it.
///
/// This states the *need*, not the mechanism, because how far back to fetch
/// follows from the need together with the cairn on disk — and that derivation
/// belongs in one place rather than at each verb. A caller that says what it
/// will show cannot ask for a cheap walk and then display segments the cheap
/// walk never fetched.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reach {
    /// Every segment, because the caller is going to show them all.
    Whole,
    /// Only the segments above `floor`; the caller already holds the rest.
    Above(u64),
    /// No segments at all — only how high the stream stands.
    Head,
}

/// Walks a stream and records where it got to.
///
/// This is the one place that decides when a cairn is read and when it is
/// written, so that no verb can accidentally hold a different policy. Marking
/// happens after a successful walk and never after a failed one: a walk that
/// stopped at a broken link has verified nothing new to remember.
///
/// # Errors
///
/// Everything [`walk`] reports, plus [`Complaint::Local`] when the cairn cannot
/// be written.
pub fn track(
    site: &Site,
    name: &str,
    waypoint: &(impl Waypoint + Sync),
    lane: &Lane,
    reach: Reach,
) -> Result<Walked, Complaint> {
    let named = lane.author.handle();
    let recorded = site.cairn(name, &named)?;
    // **A lane whose keys burn behind it cannot be walked from below its floor.**
    // There the cairn is not an optimisation, it is the only place a walk can
    // start: the drops beneath it have been deleted and the keys that opened
    // them destroyed, so asking for them would find empty addresses and report a
    // stream that never began.
    let burned = lane.keys.floor() > 0;
    let from = match reach {
        Reach::Whole if burned => recorded,
        Reach::Whole => None,
        Reach::Head => recorded,
        // A cairn above the caller's floor cannot be resumed from: the segments
        // between the floor and the cairn are ones the caller asked to see, and a
        // resumed walk would never fetch them. Falling back to the whole stream
        // costs requests; getting this wrong would silently drop segments.
        Reach::Above(floor) => recorded.filter(|cairn| burned || cairn.head().index() <= floor),
    };
    let walked = walk(waypoint, lane, name, from)?;

    // A resumed walk cannot contradict the record it resumed from: it started
    // there. A walk from genesis can, and that is the only shape in which a host
    // gets to lie by omission — hand back a shorter chain that verifies
    // perfectly, and a reader with no memory believes it.
    if from.is_none()
        && let Some(known) = recorded
    {
        confirm(&walked, &known, name)?;
    }

    // Only when the position moved. A poll that finds nothing is the common
    // invocation, and rewriting an identical record costs a flush to disk —
    // measured at 4 ms, more than everything else the poll does put together.
    if let Some(cairn) = walked.cairn()
        && recorded != Some(cairn)
    {
        site.mark(name, &cairn)?;
    }
    Ok(walked)
}

/// Refuses a reading that contradicts one this endpoint already verified.
///
/// Two ways it can contradict, and both are the host withdrawing a promise it
/// made when it accepted the write: the stream is shorter than it was, or the
/// segment at a height already read is a different segment. Neither can be
/// distinguished from honest emptiness without the record, which is why this
/// check exists here and not in `chain`.
fn confirm(walked: &Walked, recorded: &Cairn, name: &str) -> Result<(), Complaint> {
    let changed = |what: String| Complaint::HistoryChanged {
        name: name.to_owned(),
        what,
    };
    let floor = recorded.head().index();

    let Some(head) = walked.head() else {
        return Err(changed(format!(
            "this endpoint verified up to height {floor}, and the host is now \
             serving nothing at all"
        )));
    };
    if head.index() < floor {
        return Err(changed(format!(
            "this endpoint verified up to height {floor}, and the host now stops \
             at {}",
            head.index()
        )));
    }
    let at_floor = walked
        .held()
        .iter()
        .find(|candidate| candidate.segment.index() == floor);
    match at_floor {
        Some(found) if found.segment.id() != recorded.head().id() => Err(changed(format!(
            "the segment at height {floor} is not the one read here before"
        ))),
        _ => Ok(()),
    }
}

/// Reads one drop, if anything is there.
///
/// `author` is whose signature the segment must carry. It is a parameter rather
/// than something read out of the bytes because a segment names its author
/// without carrying the key that checks the name: the key comes from the channel
/// record, which is to say from having been introduced.
///
/// # Errors
///
/// [`Complaint::Waypoint`] when the host fails, [`Complaint::Sealed`] when the
/// bytes do not open under this address's key, and [`Complaint::Segment`] when
/// what comes out is not a segment by `author`.
pub fn peek(
    waypoint: &impl Waypoint,
    lane: &Lane,
    index: u64,
) -> Result<Option<Segment>, Complaint> {
    let Some(sealed) = waypoint.get(&lane.keys.address(index))? else {
        return Ok(None);
    };
    let plain = open(&lane.keys.key(index)?, Fit::Veil, &sealed)?;
    Ok(Some(Segment::from_canonical_bytes(&plain, &lane.author)?))
}

/// Walks a stream from `from` until the first empty address.
///
/// `from` is where verification carries on from, not merely where fetching
/// starts: the first segment read must link to that cairn's head, so resuming
/// checks the join rather than assuming it.
///
/// # Errors
///
/// Everything [`peek`] reports, plus [`Complaint::NotThePeer`] when a segment is
/// signed by somebody other than `author`, and [`Complaint::Chain`] when the
/// segments do not form a chain — which, on a resumed walk, is also what a host
/// that revised a drop this endpoint already read comes out as.
pub fn walk(
    waypoint: &(impl Waypoint + Sync),
    lane: &Lane,
    name: &str,
    from: Option<Cairn>,
) -> Result<Walked, Complaint> {
    let mut verifier = match from {
        Some(cairn) => Verifier::resume(cairn),
        None => Verifier::new(),
    };
    let mut held = Vec::new();

    // A cairn at the last height a `u64` can express has nothing above it, so the
    // walk is already over. That is an answer, not a failure.
    let Some(start) = from.map_or(Some(0), |cairn| cairn.next_index()) else {
        return Ok(Walked { verifier, held });
    };

    let mut index = start;
    let mut width = 1;
    loop {
        for found in fetch(waypoint, lane, name, index, width)? {
            let Some(segment) = found else {
                return Ok(Walked { verifier, held });
            };
            verifier.accept(&segment)?;
            held.push(Held {
                address: lane.keys.address(segment.index()),
                segment,
            });
        }
        let Some(next) = u64::try_from(width)
            .ok()
            .and_then(|step| index.checked_add(step))
        else {
            return Ok(Walked { verifier, held });
        };
        index = next;
        width = width.saturating_mul(2).min(WINDOW);
    }
}

/// The most addresses one walk asks for at once.
///
/// Bounded because law 2 says memory does not grow with the work: a window is a
/// constant number of drops held at once whatever the height of the stream. Eight
/// because it is enough for the requests to overlap on any real link and small
/// enough that a walk which runs past the end of a stream wastes eight requests
/// rather than a hundred.
pub const WINDOW: usize = 8;

/// Fetches `width` consecutive addresses at once, and returns them in order.
///
/// **In flight together, delivered in order.** The concurrency is what a host
/// sees; the ordering is what the verifier needs. Doing it the other way round —
/// verifying whatever arrived first — would be a chain check that depends on
/// network timing, which is not a chain check.
///
/// # Errors
///
/// The first failure among the batch, by address order rather than by arrival,
/// so that two runs against the same host report the same thing.
fn fetch(
    waypoint: &(impl Waypoint + Sync),
    lane: &Lane,
    name: &str,
    from: u64,
    width: usize,
) -> Result<Vec<Option<Segment>>, Complaint> {
    let indices: Vec<u64> = (0..width)
        .filter_map(|step| from.checked_add(u64::try_from(step).ok()?))
        .collect();
    let collected: Vec<Result<Option<Segment>, Complaint>> = std::thread::scope(|scope| {
        let running: Vec<_> = indices
            .iter()
            .map(|index| scope.spawn(move || peek(waypoint, lane, *index)))
            .collect();
        running
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err(Complaint::Local {
                        action: "read a drop",
                        source: std::io::Error::other("a reader did not finish"),
                    })
                })
            })
            .collect()
    });

    let mut found = Vec::with_capacity(collected.len());
    for outcome in collected {
        // A genuine segment by somebody else is the host answering with a drop
        // from a stream nobody asked for. The decoder catches it, and it is
        // reported as what it is rather than as a malformed segment.
        match outcome {
            Err(Complaint::Segment(SegmentError::NotTheAuthor { .. })) => {
                return Err(Complaint::NotThePeer {
                    name: name.to_owned(),
                });
            }
            other => found.push(other?),
        }
    }
    Ok(found)
}
