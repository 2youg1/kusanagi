// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Reading authors' streams out of a waypoint, and writing down where they got to.
//!
//! **What a walk asks the host for is a privacy decision, not a performance
//! one.** A walk that named every address it visited handed a host with an
//! access log the grouping `seal` exists to deny it. So a walk does not talk to
//! the host about heights: it takes the reader's ward one bin at a time
//! ([`Sweeping`]) and matches the objects of each bin against the lanes it is
//! walking, here on this machine. A host sees a bin being read and never which
//! object in it was wanted (D-20).
//!
//! **One sweep serves every lane of a ward.** A channel walks one lane through
//! it; a room walks every member's. The bin in hand is offered to each lane in
//! turn, and let go before the next is taken, so the memory held is one bin
//! whatever the number of lanes, and the requests made are one listing per
//! period whatever the number of lanes — a room read costs what a channel read
//! costs.
//!
//! **Where a walk starts still matters, because it decides how many bins are
//! swept.** A walk that only owes the caller the head, or the segments above a
//! height the caller already holds, resumes from the cairn this endpoint wrote
//! last time and sweeps from the period it last swept through, so a poll costs
//! one or two listings; a walk that owes every segment sweeps from the period
//! the channel was opened in. When lanes share a sweep, the one that needs the
//! most decides for all.

use kusanagi_chain::{Cairn, Verifier};
use kusanagi_kernel::{ChainHead, Instant, Listing, Segment, Waypoint};
use kusanagi_seal::period;
use kusanagi_site::{Site, Swept};

use crate::lane::Lane;
use crate::stepping::{Held, Stepping, decode};
use crate::sweep::{DIGITS, Sweeping};
use kusanagi_door::Complaint;

/// A stream as it was found on a waypoint.
pub struct Walked {
    verifier: Verifier,
    held: Vec<Held>,
    /// What the last bin swept for it listed, when it was found by sweeping.
    ///
    /// Carried out so that a writer, having added to that bin, can record the
    /// listing as it left it and spare the next sweep a fetch of everything.
    /// Every lane of one sweep carries the same listing.
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

/// Where a walk carries on from, given what it owes and what is written down.
///
/// **A lane whose keys burn behind it cannot be walked from below its floor.**
/// There the cairn is not an optimisation, it is the only place a walk can
/// start: the drops beneath it have been deleted and the keys that opened them
/// destroyed, so asking for them would find empty addresses and report a stream
/// that never began.
fn starting(reach: Reach, recorded: Option<Cairn>, burned: bool) -> Option<Cairn> {
    match reach {
        Reach::Whole if burned => recorded,
        Reach::Whole => None,
        Reach::Head => recorded,
        // A cairn above the caller's floor cannot be resumed from: the segments
        // between the floor and the cairn are ones the caller asked to see, and a
        // resumed walk would never fetch them. Falling back to the whole stream
        // costs requests; getting this wrong would silently drop segments.
        Reach::Above(floor) => recorded.filter(|cairn| burned || cairn.head().index() <= floor),
    }
}

/// Walks one lane and records where it got to.
///
/// # Errors
///
/// Everything [`track_all`] reports.
pub fn track(
    site: &Site,
    name: &str,
    place: &(impl Waypoint + Listing + Sync),
    lane: &Lane,
    reach: Reach,
    now: Instant,
) -> Result<Walked, Complaint> {
    track_all(site, name, place, &[(lane, reach)], now)?
        .pop()
        .ok_or_else(|| Complaint::Local {
            action: "walk a lane",
            source: std::io::Error::other("one lane walked came back as none"),
        })
}

/// Walks every lane of one ward through one sweep, and records where each got to.
///
/// This is the one place that decides when a cairn is read and when it is
/// written, so that no verb can accidentally hold a different policy. Marking
/// happens after every lane walked and never after a failed one: a walk that
/// stopped at a broken link has verified nothing new to remember.
///
/// The lanes are one ward's — a channel's one lane, or a room's members, whose
/// lanes derive from one record. A lane filed in another ward finds nothing
/// here, because its addresses are in a bin this sweep never lists.
///
/// # Errors
///
/// Everything a sweep and a step report, [`Complaint::HistoryChanged`] when a
/// walk from genesis contradicts what this endpoint verified before, and
/// [`Complaint::Local`] when a record cannot be written.
pub fn track_all(
    site: &Site,
    name: &str,
    place: &(impl Waypoint + Listing + Sync),
    lanes: &[(&Lane, Reach)],
    now: Instant,
) -> Result<Vec<Walked>, Complaint> {
    let Some((first, _)) = lanes.first() else {
        return Ok(Vec::new());
    };
    let ward = first.bin.ward();
    let opened = lanes
        .iter()
        .map(|(lane, _)| lane.opened)
        .fold(first.opened, Ord::min);

    let mut steps = Vec::with_capacity(lanes.len());
    let mut resumed = true;
    for (lane, reach) in lanes {
        let recorded = site.cairn(name, &lane.author.handle())?;
        let from = starting(*reach, recorded, lane.keys.floor() > 0);
        resumed &= from.is_some();
        steps.push((Stepping::from(lane, from), recorded, from));
    }

    // A walk that resumes sweeps from the last period it swept through — that
    // one included, since a bin keeps filling until its period ends — and a
    // walk from genesis sweeps from the period the channel was opened in. One
    // lane walking from genesis makes the sweep start there for all of them.
    let recorded_sweep = site.swept(name, ward)?;
    let (since, known) = if resumed {
        (
            recorded_sweep
                .as_ref()
                .map_or(opened, |swept| swept.through),
            recorded_sweep.clone(),
        )
    } else {
        (opened, None)
    };
    let through = period(now.as_unix_seconds());
    let digits = site.sweep_digits()?.unwrap_or(DIGITS);
    let mut sweeping = Sweeping::over(place, ward, digits, since, through, known);
    let mut listed = None;
    while let Some(mut taken) = sweeping.take()? {
        for (step, _, _) in &mut steps {
            step.advance(&mut taken.held, name)?;
        }
        listed = Some(taken.seen);
    }

    let mut walked = Vec::with_capacity(steps.len());
    for (step, recorded, from) in steps {
        let (verifier, held) = step.finish();
        let done = Walked {
            verifier,
            held,
            listed: listed.clone(),
        };
        // A resumed walk cannot contradict the record it resumed from: it
        // started there. A walk from genesis can, and that is the only shape in
        // which a host gets to lie by omission — hand back a shorter chain that
        // verifies perfectly, and a reader with no memory believes it.
        if from.is_none()
            && let Some(known) = recorded
        {
            confirm(&done, &known, name)?;
        }
        walked.push((done, recorded));
    }

    // Only when the position moved. A poll that finds nothing is the common
    // invocation, and rewriting an identical record costs a flush to disk —
    // measured at 4 ms, more than everything else the poll does put together.
    for (done, recorded) in &walked {
        if let Some(cairn) = done.cairn()
            && *recorded != Some(cairn)
        {
            site.mark(name, &cairn)?;
        }
    }
    // Only when the bin changed: a record rewritten with its own contents is a
    // flush paid for nothing, and an idle poll is the invocation a scheduler
    // makes most.
    if let Some(seen) = &listed
        && recorded_sweep.as_ref() != Some(seen)
    {
        site.sweep_to(name, ward, seen)?;
    }
    Ok(walked.into_iter().map(|(done, _)| done).collect())
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
/// The one read that names an address: an introduction stream sits in the
/// rendezvous bin at a height both ends know, and is read exactly once.
/// `lane.author` is whose signature the segment must carry — a parameter
/// rather than something read out of the bytes, because a segment names its
/// author without carrying the key that checks the name.
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
