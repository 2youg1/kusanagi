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

use kusanagi_door::{Delivery, Landed};
use kusanagi_grant::Ability;
use kusanagi_kernel::{Freight, Instant, Purpose, PutOutcome, Segment, Signer, Waypoint};
use kusanagi_seal::{Fit, seal};
use kusanagi_site::{Channel, Site};

use crate::assembly::{open, peer_ward, signer, ward};
use crate::lane::{Lane, verified};
use crate::membership::greet;
use crate::request::Whose;
use crate::walk::{Reach, Walked, track};
use kusanagi_door::Complaint;
use kusanagi_door::Outcome;

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
        site.queue(name, payload)?;
        return Ok(Outcome::Queued {
            name: name.to_owned(),
            waiting: site.pending(name)?.len(),
            period: channel.cadence.period(),
        });
    }
    let written = appended(site, &signer(site)?, name, Purpose::Message, payload, now)?;
    Ok(Outcome::Sent {
        name: name.to_owned(),
        index: written.index,
        id: written.id,
        address: written.address,
    })
}

/// Appends one segment to every member of a group, and reports each separately.
///
/// **One member's failure is not the send's failure.** A host that is down, a
/// channel that was forgotten, or a grant that was revoked stops that member
/// from hearing this and stops nothing else; collapsing the five results into
/// one would either hide a person who did not receive it or claim four people
/// did not when they did. The caller reads the rows.
///
/// # Errors
///
/// [`Complaint::UnknownGroup`] when there is no such group. That is the one
/// failure of the fan-out itself rather than of a member.
pub(crate) fn fanout(
    site: &Site,
    group: &str,
    payload: &[u8],
    now: Instant,
) -> Result<Outcome, Complaint> {
    // One signer for every member: N members used to cost N identity reads.
    let me = signer(site)?;
    let roster = site.roster(group).map_err(|error| match error {
        kusanagi_site::SiteError::UnknownChannel { name } => Complaint::UnknownGroup { name },
        other => other.into(),
    })?;
    let delivered = roster
        .members
        .iter()
        .map(|member| Delivery {
            member: member.clone(),
            landed: match appended(site, &me, member, Purpose::Message, payload, now) {
                Ok(written) => Landed::Sent {
                    index: written.index,
                    address: written.address,
                },
                Err(refusal) => Landed::Refused {
                    code: refusal.code(),
                    error: refusal.to_string(),
                },
            },
        })
        .collect();
    Ok(Outcome::FannedOut {
        group: group.to_owned(),
        delivered,
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
    purpose: Purpose,
    payload: &[u8],
    now: Instant,
) -> Result<Appended, Complaint> {
    let channel = site.channel(name)?;
    let revoked = site.revocations()?;
    channel
        .standing
        .permits(&channel.root, &me.handle(), Ability::Send, now, &revoked)?;
    // A segment the peer is no longer allowed to read is a segment that should
    // not be written: revocation cuts both directions, or a fan-out keeps
    // delivering to the one member it was meant to exclude. The question is
    // the mirror of the one `read` asks about the peer, and fails the same way.
    if let Some(peer) = &channel.peer {
        peer.standing
            .permits(&channel.root, &peer.handle(), Ability::Read, now, &revoked)?;
    }

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
    let mine = Lane::open(
        site,
        name,
        &channel,
        &me.verifying_key(),
        peer_ward(&channel, name)?,
    )?;
    // Only the head is needed, so this walk owes the caller no segment and may
    // resume from the cairn: sending the thousandth segment asks the host for one
    // address rather than announcing the previous nine hundred and ninety-nine.
    let walked = track(site, name, &place, &mine, Reach::Head)?;

    // The height still comes from the waypoint rather than from a local count:
    // the cairn moves the walk's starting point and proves the join to it, so a
    // lost or absent cairn changes what this costs and never what it decides.
    // The trail is derived here and dropped at the end of this command. It is
    // never written down: an author recomputes it from a deterministic signature
    // over their own lane, so a killed process loses nothing and a seized disk
    // holds no proof of anything.
    let acknowledged = match &channel.peer {
        None => 0,
        Some(peer) => verified(site, name, &peer.handle())?,
    };
    let freight = match purpose {
        Purpose::Message => Freight::message(payload.to_vec()),
        Purpose::Filler => Freight::filler(),
    }?
    .acknowledging(acknowledged);

    let trail = mine.keys.trail(me);
    let segment = match walked.head() {
        None => Segment::genesis(me, &trail, freight),
        Some(head) => Segment::extend(&trail, me.handle(), freight, head),
    }?;

    let address = mine.keys.address(segment.index());
    let sealed = seal(
        &mine.keys.key(segment.index())?,
        Fit::Veil,
        &segment.to_canonical_bytes(),
    )?;
    match Waypoint::put_if_absent(&place, &mine.at(address), &sealed)? {
        // The host took it at an address that was empty, so this endpoint knows
        // the segment is there without reading it back. Recording that now is
        // what keeps the next send at one request: a position left one behind
        // the stream would make every send rediscover what it had just written.
        PutOutcome::Stored => {
            if let Some(cairn) = walked.extended(&segment)? {
                site.mark(name, &cairn)?;
            }
        }
        PutOutcome::AlreadyPresent => {
            return Err(Complaint::DropTaken {
                address: address.to_string(),
                name: name.to_owned(),
            });
        }
    }

    Ok(Appended {
        index: segment.index(),
        id: segment.id().to_string(),
        address: address.to_string(),
    })
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

    let theirs = Lane::open(site, name, &channel, &peer.key, ward(site)?)?;
    let walked = track(site, name, &place, &theirs, reach(after))?;
    let answer = reported(name, &peer.handle().to_string(), &walked, after);

    // Only now, once the caller holds what was read: settling is the step that
    // destroys it.
    if channel.retention.releases() {
        settle(site, name, &channel, &place, &walked, &theirs, me)?;
    }
    Ok(answer)
}

/// Acts on what a read just learned, on a channel that releases.
///
/// Two halves of one promise, and they are here together because doing either
/// alone would be a claim this endpoint could not keep. **Deletion is the honest
/// host's half**: the peer said how much of this endpoint's stream they had
/// verified, so those drops are removed and an honest host now holds no history.
/// **The ratchet is the dishonest host's half**: the keys that opened them are
/// destroyed, so a host that quietly kept a copy holds bytes nobody can open.
///
/// A failure to delete is reported, because an endpoint that believes its
/// history is gone and is wrong is an endpoint making a false promise.
fn settle(
    site: &Site,
    name: &str,
    channel: &Channel,
    place: &impl Waypoint,
    walked: &Walked,
    theirs: &Lane,
    me: &Signer,
) -> Result<(), Complaint> {
    // The peer repeats their acknowledgement in every segment, so the highest
    // one in this walk is the current answer and an older segment cannot undo a
    // newer one.
    let acknowledged = walked
        .held()
        .iter()
        .map(|held| held.segment.acknowledged())
        .max()
        .unwrap_or(0);

    if acknowledged > 0 {
        let ours = Lane::open(
            site,
            name,
            channel,
            &me.verifying_key(),
            peer_ward(channel, name)?,
        )?;
        for index in ours.keys.floor()..acknowledged {
            place.delete(&ours.holding(index))?;
        }
        ours.burn_below(site, name, acknowledged.saturating_sub(1))?;
    }

    // The peer's own lane burns behind the reader in the same way. What was
    // verified has been handed over; nothing will ask for it again.
    if let Some(head) = walked.head() {
        theirs.burn_below(site, name, head.index())?;
    }
    Ok(())
}

/// Turns a walk into the answer for it, dropping what the caller already holds
/// and what nobody meant to say.
///
/// Two filters, and they are different kinds of thing. `--after` is a property
/// of the request, so it lives with the verb rather than in `door`, which
/// renders what it is handed and cannot perform a walk. **A filler is filtered
/// because it is not a message at all**: it exists so that an observer cannot
/// tell a silent endpoint from a busy one, and reporting it to the caller would
/// hand them padding to read as though somebody had written it.
///
/// The height is unaffected by either filter. It is the verified head of the
/// stream, fillers included — a height that skipped them would tell a reader
/// exactly how many slots went by empty, which is the fact the fillers were
/// spent to hide.
fn reported(name: &str, author: &str, walked: &Walked, after: Option<u64>) -> Outcome {
    Outcome::read(
        name,
        author,
        walked.head().map(|head| head.index()),
        walked
            .held()
            .iter()
            .filter(|held| held.segment.purpose() == Purpose::Message)
            .filter(|held| after.is_none_or(|floor| held.segment.index() > floor))
            .map(|held| {
                (
                    held.segment.index(),
                    held.segment.acknowledged(),
                    held.segment.payload(),
                )
            }),
    )
}

/// Reports this endpoint's own stream, verified the same way a peer's is.
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
    )?;
    let walked = track(site, name, &place, &ours, reach(after))?;
    Ok(reported(name, &me.handle().to_string(), &walked, after))
}
