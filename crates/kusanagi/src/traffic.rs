// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What moves on a channel: one segment out, one stream in.
//!
//! Both verbs check authority before they touch a waypoint, and they check
//! different sides of it. `send` asks whether this endpoint may write; `read`
//! asks whether the author it is about to read was permitted to. The second is
//! the one that enforces a revocation, because it needs no cooperation from the
//! peer or from the host.

use kusanagi_door::Outcome;
use kusanagi_grant::Ability;
use kusanagi_kernel::{
    Freight, Instant, MAX_PAYLOAD, PutOutcome, Segment, Signer, Waypoint, divide,
};
use kusanagi_seal::{Fit, seal};
use kusanagi_site::{Channel, Site};

use crate::settle::settle;

use crate::assembly::{open, peer_ward, signer, ward};
use crate::greeting::greet;
use crate::request::Whose;
use crate::traffic_read::reported;
use kusanagi_door::Complaint;
use kusanagi_walk::{Lane, verified};
use kusanagi_walk::{Reach, Walked, track};

/// How many segments one message on a channel may take.
///
/// **The limit is about whose ward pays, not about what a person wants to
/// send.** A channel's drops are filed in the *peer's* ward, which they share
/// with strangers who chose the same four digits; every one of those strangers
/// downloads whatever lands there, and none of them agreed to it. Thirty-two
/// drops is an eighth of a bin ([`kusanagi_walk::CAP`]), so eight of these can
/// happen in one period before the ward is refused to everybody in it.
///
/// A room's is twice this and lives beside `room_send`: a room's ward is the
/// room's own, so the members who pay for it are the members who joined it.
pub(crate) const CHANNEL_PARTS: u16 = 32;

/// What a `read` owes its caller: everything, or only what sits above the height
/// the caller says it already holds.
const fn reach(after: Option<u64>) -> Reach {
    match after {
        None => Reach::Whole,
        Some(floor) => Reach::Above(floor),
    }
}

/// What appending one segment produced, before anybody decides how to say it.
///
/// The verb and the fan-out want the same three facts and report them in two
/// different shapes, so the facts are a value and neither shape is the other's
/// special case.
pub(crate) struct Appended {
    pub(crate) index: u64,
    pub(crate) id: String,
    pub(crate) address: String,
}

/// Appends one segment, or queues it when the channel writes on a schedule.
///
/// **A slotted channel cannot write when it is asked to**, because writing when
/// asked is exactly the rhythm a slot exists to hide. So the payload is left in
/// the outbox and `tick` takes it out when the slot comes round; the caller is
/// told which of the two happened, because the difference is a delay of up to
/// one period and a promise kept by this disk rather than by a host.
pub(crate) fn send(
    site: &Site,
    name: &str,
    payload: &[u8],
    now: Instant,
) -> Result<Outcome, Complaint> {
    let channel = site.channel(name)?;
    if channel.cadence.period().is_some() {
        // A slot is one drop per period, whatever there is to say. A message in
        // several drops would either burst — which is the shape a slot exists to
        // hide — or straddle periods, and a run straddling periods is one a
        // reader must re-walk from genesis on every poll until it completes.
        let whole = usize::try_from(MAX_PAYLOAD).unwrap_or(usize::MAX);
        if payload.len() > whole {
            return Err(Complaint::SlottedOneDrop {
                name: name.to_owned(),
                limit: whole,
            });
        }
        site.queue(name, payload)?;
        return Ok(Outcome::Queued {
            name: name.to_owned(),
            waiting: site.pending(name)?.len(),
            period: channel.cadence.period(),
        });
    }
    // Divided before anything is opened, so a message past the limit is refused
    // here, on this machine, without a host being told that anybody tried.
    let freights = divide(payload, CHANNEL_PARTS)?;
    let written = appended(site, &signer(site)?, name, freights, now)?;
    Ok(Outcome::Sent {
        name: name.to_owned(),
        index: written.index,
        id: written.id,
        address: written.address,
    })
}

/// Writes one segment on this endpoint's own lane and reports where it landed.
///
/// Every segment carries how far its author has verified the other side. It
/// rides inside the sealed part, so the host learns nothing from it, and it is
/// outside the signature, so it proves nothing to anybody afterwards — a signed
/// receipt for *how much of you I read* would be transferable evidence that the
/// conversation happened at all.
pub(crate) fn appended(
    site: &Site,
    me: &Signer,
    name: &str,
    freights: Vec<Freight>,
    now: Instant,
) -> Result<Appended, Complaint> {
    let channel = site.channel(name)?;
    let revoked = site.revocations()?;
    channel
        .standing
        .permits(&channel.root, &me.handle(), Ability::Send, now, &revoked)?;
    let place = open(site, &channel.locator, now)?;
    // Where this segment goes is the peer's ward, so an endpoint that has not
    // met its peer yet meets them now. This is the same lazy introduction `read`
    // performs and the same one request; before a bin had to be chosen, a send
    // could be written for somebody who had not arrived, and now it cannot —
    // there is nowhere to deliver to until they say where they look.
    let channel = match channel.peer {
        Some(_) => channel,
        None => greet(site, name, channel, &place, now)?,
    };
    // A segment the peer is no longer allowed to read is a segment that should
    // not be written: revocation cuts both directions, or a fan-out keeps
    // delivering to the one member it was meant to exclude. The question is
    // the mirror of the one `read` asks about the peer, and fails the same way.
    // **After the greeting**: the peer met a moment ago is the one this checks.
    if let Some(peer) = &channel.peer {
        peer.standing
            .permits(&channel.root, &peer.handle(), Ability::Read, now, &revoked)?;
    }
    let mine = Lane::open(
        site,
        name,
        &channel,
        &me.verifying_key(),
        peer_ward(&channel, name)?,
        now,
    )?;
    // Only the head is needed, so this walk owes the caller no segment and may
    // resume from the cairn: sending the thousandth segment sweeps the bins
    // since the last send rather than the whole history. The ward it sweeps is
    // the peer's, which the host already saw this endpoint write into.
    let walked = track(site, name, &place, &mine, Reach::Head, now)?;

    // The height still comes from the waypoint rather than from a local count:
    // the cairn moves the walk's starting point and proves the join to it, so a
    // lost or absent cairn changes what this costs and never what it decides.
    let acknowledged = match &channel.peer {
        None => 0,
        Some(peer) => verified(site, name, &peer.handle())?,
    };
    append(
        site,
        name,
        &place,
        &mine,
        me,
        freights
            .into_iter()
            .map(|freight| freight.acknowledging(acknowledged))
            .collect(),
        walked,
    )
}

/// Seals `freights` as the next segments of `mine` and leaves them on the host.
///
/// The one place a segment is written, whichever stream it is on: a channel's,
/// a room member's, or a founder's roster. `walked` is the walk to the head
/// the caller just made, and this extends it.
///
/// **The whole run is built before any of it is written.** Every address in it
/// follows from the head this endpoint already holds, so nothing in the
/// building waits on a host and the writes go out together — the same shape a
/// sweep uses to fetch a bin. A run that fails halfway leaves segments no
/// reader will ever report, because a run only becomes a message once all of it
/// is there; the caller sends again.
///
/// The trail is derived here and dropped at the end of this command. It is
/// never written down: an author recomputes it from a deterministic signature
/// over their own lane, so a killed process loses nothing and a seized disk
/// holds no proof of anything.
///
/// # Errors
///
/// [`Complaint::DropTaken`] when an address the run derives is already held,
/// which is another writer on this lane; and whatever sealing, the host, or the
/// two records report.
pub(crate) fn append(
    site: &Site,
    name: &str,
    place: &(impl Waypoint + Sync),
    mine: &Lane,
    me: &Signer,
    freights: Vec<Freight>,
    walked: Walked,
) -> Result<Appended, Complaint> {
    let trail = mine.keys.trail(me);
    let mut standing = walked.standing();
    let mut run = Vec::with_capacity(freights.len());
    for freight in freights {
        let segment = match standing.head() {
            None => Segment::genesis(me, &trail, freight),
            Some(head) => Segment::extend(&trail, me.handle(), freight, head),
        }?;
        standing.accept(&segment)?;
        let sealed = seal(
            &mine.keys.key(segment.index())?,
            Fit::Veil,
            &segment.to_canonical_bytes(),
        )?;
        run.push(Written {
            index: segment.index(),
            id: segment.id().to_string(),
            object: mine.at(mine.keys.address(segment.index())),
            sealed,
        });
    }
    // In flight together: the host sees a bin being added to, not a sequence.
    let outcomes: Vec<Result<PutOutcome, Complaint>> = std::thread::scope(|scope| {
        let running: Vec<_> = run
            .iter()
            .map(|written| {
                scope.spawn(move || {
                    Ok(Waypoint::put_if_absent(
                        place,
                        &written.object,
                        &written.sealed,
                    )?)
                })
            })
            .collect();
        running
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err(Complaint::Local {
                        action: "write a segment",
                        source: std::io::Error::other("a writer did not finish"),
                    })
                })
            })
            .collect()
    });
    for (outcome, written) in outcomes.into_iter().zip(&run) {
        if outcome? == PutOutcome::AlreadyPresent {
            return Err(Complaint::DropTaken {
                address: written.object.to_string(),
                name: name.to_owned(),
            });
        }
    }
    // The host took them at addresses that were empty, so this endpoint knows
    // they are there without reading them back. Recording that now is what
    // keeps the next send at one request: a position left behind the stream
    // would make every send rediscover what it had just written, and a bin
    // recorded without the objects just added would make the next sweep take
    // the whole bin to find drops this endpoint wrote itself.
    if let Some(cairn) = standing.cairn() {
        site.mark(name, &cairn)?;
    }
    if let Some(listed) = walked.listed {
        let listed = run
            .iter()
            .fold(listed, |listed, written| listed.including(written.object));
        site.sweep_to(name, mine.bin.ward(), &listed)?;
    }
    // The last of the run is where the message now stands, which is the height
    // a reader will report it at and the one this endpoint carries on from.
    let last = run.pop().ok_or_else(|| Complaint::Local {
        action: "append to a stream",
        source: std::io::Error::other("a send with no segments in it"),
    })?;
    // The key rather than the address alone: since a drop is filed in a bin,
    // the address by itself no longer says where on the host anything is.
    Ok(Appended {
        index: last.index,
        id: last.id,
        address: last.object.to_string(),
    })
}

/// One segment of a run, built and sealed, waiting for the host.
struct Written {
    index: u64,
    id: String,
    object: kusanagi_kernel::Object,
    sealed: Vec<u8>,
}

pub(crate) fn read(
    site: &Site,
    me: &Signer,
    name: &str,
    after: Option<u64>,
    whose: Whose,
    now: Instant,
) -> Result<Outcome, Complaint> {
    let channel = site.channel(name)?;
    let revoked = site.revocations()?;
    if whose == Whose::Mine {
        return mine(site, &channel, me, name, after, now);
    }
    channel
        .standing
        .permits(&channel.root, &me.handle(), Ability::Read, now, &revoked)?;

    let place = open(site, &channel.locator, now)?;
    let channel = match channel.peer {
        Some(_) => channel,
        None => greet(site, name, channel, &place, now)?,
    };
    let peer = channel.peer.as_ref().ok_or_else(|| Complaint::NoPeerYet {
        name: name.to_owned(),
    })?;

    // The peer's own authority is checked before their bytes are read, so a
    // revoked peer's stream is refused rather than displayed with a warning.
    peer.standing
        .permits(&channel.root, &peer.handle(), Ability::Send, now, &revoked)?;

    let theirs = Lane::open(site, name, &channel, &peer.key, ward(site)?, now)?;
    let walked = track(site, name, &place, &theirs, reach(after), now)?;
    let answer = reported(
        name,
        &peer.handle().to_string(),
        peer.alias.as_ref(),
        &walked,
        after,
    );

    // Only now, once the caller holds what was read: settling is the step that
    // destroys it.
    if channel.retention.releases() {
        settle(site, name, &channel, &walked, &theirs, me, now)?;
    }
    Ok(answer)
}

/// Reports this endpoint's own stream, verified the same way a peer's is.
///
/// It sweeps the peer's ward, because that is where this endpoint's segments are
/// filed; the host already saw this endpoint write there, so it learns nothing
/// new from seeing it read there.
///
/// No standing is checked, and that is a statement about where enforcement can
/// live rather than a relaxation. These segments sit at addresses derived from a
/// secret this endpoint holds and carry signatures this endpoint made; refusing
/// to show them would refuse nothing, because the bytes are reachable with or
/// without this program's permission. The checks that can stop something are in
/// `send` and in the peer's `read`, and they stay there.
///
/// An agent that was killed mid-loop needs exactly this: its own height, without
/// writing a segment to find out.
fn mine(
    site: &Site,
    channel: &Channel,
    me: &Signer,
    name: &str,
    after: Option<u64>,
    now: Instant,
) -> Result<Outcome, Complaint> {
    let place = open(site, &channel.locator, now)?;
    let ours = Lane::open(
        site,
        name,
        channel,
        &me.verifying_key(),
        peer_ward(channel, name)?,
        now,
    )?;
    let walked = track(site, name, &place, &ours, reach(after), now)?;
    Ok(reported(
        name,
        &me.handle().to_string(),
        site.alias()?.as_ref(),
        &walked,
        after,
    ))
}
