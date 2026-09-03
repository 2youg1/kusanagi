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
//! What that does not close is the first catch-up, which still walks what it has
//! never seen, and the live edge, which a host can follow as it advances. Both
//! are recorded in `ARCHITECTURE.md` §3 rather than fixed here.

use kusanagi_chain::{Cairn, Verifier};
use kusanagi_kernel::{ChainHead, DropAddr, Segment, SegmentError, VerifyingKey, Waypoint};
use kusanagi_seal::{Fit, Stream, derive, open};
use kusanagi_site::Site;

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
    waypoint: &impl Waypoint,
    stream: &Stream,
    author: &VerifyingKey,
    reach: Reach,
) -> Result<Walked, Complaint> {
    let named = author.handle();
    let from = match reach {
        Reach::Whole => None,
        Reach::Head => site.cairn(name, &named)?,
        // A cairn above the caller's floor cannot be resumed from: the segments
        // between the floor and the cairn are ones the caller asked to see, and a
        // resumed walk would never fetch them. Falling back to the whole stream
        // costs requests; getting this wrong would silently drop segments.
        Reach::Above(floor) => site
            .cairn(name, &named)?
            .filter(|cairn| cairn.head().index() <= floor),
    };
    let walked = walk(waypoint, stream, author, name, from)?;

    // A resumed walk cannot contradict the record it resumed from: it started
    // there. A walk from genesis can, and that is the only shape in which a host
    // gets to lie by omission — hand back a shorter chain that verifies
    // perfectly, and a reader with no memory believes it.
    if from.is_none()
        && let Some(recorded) = site.cairn(name, &named)?
    {
        confirm(&walked, &recorded, name)?;
    }

    if let Some(cairn) = walked.cairn() {
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
    stream: &Stream,
    index: u64,
    author: &VerifyingKey,
) -> Result<Option<Segment>, Complaint> {
    let (address, key) = derive(stream, index);
    let Some(sealed) = waypoint.get(&address)? else {
        return Ok(None);
    };
    let plain = open(&key, Fit::Veil, &sealed)?;
    Ok(Some(Segment::from_canonical_bytes(&plain, author)?))
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
    waypoint: &impl Waypoint,
    stream: &Stream,
    author: &VerifyingKey,
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

    for index in start..u64::MAX {
        // A genuine segment by somebody else is the host answering with a drop
        // from a stream nobody asked for. The decoder catches it, and it is
        // reported as what it is rather than as a malformed segment.
        let found = match peek(waypoint, stream, index, author) {
            Err(Complaint::Segment(SegmentError::NotTheAuthor { .. })) => {
                return Err(Complaint::NotThePeer {
                    name: name.to_owned(),
                });
            }
            other => other?,
        };
        let Some(segment) = found else {
            break;
        };
        verifier.accept(&segment)?;
        held.push(Held {
            address: derive(stream, index).0,
            segment,
        });
    }

    Ok(Walked { verifier, held })
}
