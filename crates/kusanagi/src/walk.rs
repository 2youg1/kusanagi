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
//! **What a walk asks the host for is a privacy decision, not a performance
//! one.** A walk that named every address it visited handed a host with an
//! access log the grouping `seal` exists to deny it. So a walk does not talk to
//! the host: it asks a [`Source`] for the sealed bytes at a height, and the
//! production source is a [`Sweeping`](crate::sweep::Sweeping) — the reader's whole
//! ward for a period, taken with one listing and as many fetches as it has
//! objects, matched against the lane's addresses here on this machine. A host
//! sees a bin being read and never which object in it was wanted (D-20).
//!
//! **Where a walk starts still matters, because it decides how many bins are
//! swept.** A walk that only owes the caller the head, or the segments above a
//! height the caller already holds, resumes from the cairn this endpoint wrote
//! last time and sweeps from the period it last swept through, so a poll costs
//! one or two listings; a walk that owes every segment sweeps from the period
//! the channel was opened in.
//!
//! The window from one to [`WINDOW`] heights per request is kept for the one
//! source that still names addresses — a directory, in tests — and for the
//! verifier, which accepts strictly in order whatever the fetching did.

use kusanagi_chain::{Cairn, Verifier};
use kusanagi_kernel::{ChainHead, DropAddr, Instant, Listing, Segment, SegmentError, Waypoint};

use crate::source::Source;
use kusanagi_seal::{Fit, open, period};
use kusanagi_site::Site;

use crate::lane::Lane;
use crate::sweep::{DIGITS, Sweeping};
use kusanagi_door::Complaint;
use kusanagi_site::Swept;

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
    /// What the last bin swept for it listed, when it was found by sweeping.
    ///
    /// Carried out so that a writer, having added to that bin, can record the
    /// listing as it left it and spare the next sweep a fetch of everything.
    pub listed: Option<Swept>,
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
    place: &(impl Waypoint + Listing + Sync),
    lane: &Lane,
    reach: Reach,
    now: Instant,
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
    // A walk that resumes sweeps from the last period it swept through — that
    // one included, since a bin keeps filling until its period ends — and a
    // walk from genesis sweeps from the period the channel was opened in. The
    // one record moves the other's starting point, never its conclusion.
    let recorded_sweep = site.swept(name, &named)?;
    let (since, known) = match from {
        Some(_) => (
            recorded_sweep
                .as_ref()
                .map_or(lane.opened, |swept| swept.through),
            recorded_sweep.clone(),
        ),
        None => (lane.opened, None),
    };
    let through = period(now.as_unix_seconds());
    let digits = site.sweep_digits()?.unwrap_or(DIGITS);
    let sweeping = Sweeping::over(place, lane.bin.ward(), digits, since, through, known);
    let mut walked = walk(&sweeping, lane, name, from)?;
    walked.listed = sweeping.listed()?;

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
    // Only when the bin changed: a record rewritten with its own contents is a
    // flush paid for nothing, and an idle poll is the invocation a scheduler
    // makes most.
    if let Some(seen) = &walked.listed
        && recorded_sweep.as_ref() != Some(seen)
    {
        site.sweep_to(name, &named, seen)?;
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
    name: &str,
    index: u64,
) -> Result<Option<Segment>, Complaint> {
    let Some(sealed) = waypoint.get(&lane.holding(index))? else {
        return Ok(None);
    };
    Ok(Some(decode(lane, name, index, &sealed)?))
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
    source: &impl Source,
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
        return Ok(Walked {
            verifier,
            held,
            listed: None,
        });
    };

    let mut index = start;
    let mut width = 1;
    loop {
        for (step, found) in source.sealed(lane, index, width)?.into_iter().enumerate() {
            let Some(sealed) = found else {
                return Ok(Walked {
                    verifier,
                    held,
                    listed: None,
                });
            };
            let Some(at) = u64::try_from(step)
                .ok()
                .and_then(|step| index.checked_add(step))
            else {
                return Ok(Walked {
                    verifier,
                    held,
                    listed: None,
                });
            };
            let segment = decode(lane, name, at, &sealed)?;
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
            return Ok(Walked {
                verifier,
                held,
                listed: None,
            });
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

/// Opens and decodes what was found at `index`, as a segment by this lane's
/// author.
///
/// # Errors
///
/// [`Complaint::Sealed`] when the bytes do not open under this height's key,
/// [`Complaint::NotThePeer`] when what comes out is a genuine segment by
/// somebody else — the host answering with a drop from a stream nobody asked
/// for, reported as what it is rather than as a malformed segment — and
/// [`Complaint::Segment`] for anything else that is not a segment.
fn decode(lane: &Lane, name: &str, index: u64, sealed: &[u8]) -> Result<Segment, Complaint> {
    let plain = open(&lane.keys.key(index)?, Fit::Veil, sealed)?;
    match Segment::from_canonical_bytes(&plain, &lane.author) {
        Err(SegmentError::NotTheAuthor { .. }) => Err(Complaint::NotThePeer {
            name: name.to_owned(),
        }),
        other => Ok(other?),
    }
}
