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

use kusanagi_grant::Ability;
use kusanagi_kernel::{Instant, PutOutcome, Segment, Signer, Waypoint as _};
use kusanagi_seal::{derive, seal};
use kusanagi_site::{Channel, Site};

use crate::assembly::{open, signer};
use crate::complaint::Complaint;
use crate::membership::greet;
use crate::report::Outcome;
use crate::request::Whose;
use crate::walk::{Reach, track};

/// What a `read` owes its caller: everything, or only what sits above the height
/// the caller says it already holds.
const fn reach(after: Option<u64>) -> Reach {
    match after {
        None => Reach::Whole,
        Some(floor) => Reach::Above(floor),
    }
}

pub(crate) fn send(
    site: &Site,
    name: &str,
    payload: &[u8],
    now: Instant,
) -> Result<Outcome, Complaint> {
    let me = signer(site)?;
    let channel = site.channel(name)?;
    channel.standing.permits(
        &channel.root,
        &me.handle(),
        Ability::Send,
        now,
        &site.revocations()?,
    )?;

    let place = open(&channel.locator, now)?;
    let stream = channel.secret.stream(&me.handle());
    // Only the head is needed, so this walk owes the caller no segment and may
    // resume from the cairn: sending the thousandth segment asks the host for one
    // address rather than announcing the previous nine hundred and ninety-nine.
    let mine = track(site, name, &place, &stream, &me.handle(), Reach::Head)?;

    // The height still comes from the waypoint rather than from a local count:
    // the cairn moves the walk's starting point and proves the join to it, so a
    // lost or absent cairn changes what this costs and never what it decides.
    let segment = match mine.head() {
        None => Segment::genesis(&me, payload.to_vec()),
        Some(head) => Segment::extend(&me, payload.to_vec(), head),
    }?;

    let (address, key) = derive(&stream, segment.index());
    let sealed = seal(&key, &segment.to_canonical_bytes())?;
    match place.put_if_absent(&address, &sealed)? {
        // The host took it at an address that was empty, so this endpoint knows
        // the segment is there without reading it back. Recording that now is
        // what keeps the next send at one request: a position left one behind
        // the stream would make every send rediscover what it had just written.
        PutOutcome::Stored => {
            if let Some(cairn) = mine.extended(&segment)? {
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

    Ok(Outcome::Sent {
        name: name.to_owned(),
        index: segment.index(),
        id: segment.id().to_string(),
        address: address.to_string(),
    })
}

pub(crate) fn read(
    site: &Site,
    name: &str,
    after: Option<u64>,
    whose: Whose,
    now: Instant,
) -> Result<Outcome, Complaint> {
    let me = signer(site)?;
    let channel = site.channel(name)?;
    let revoked = site.revocations()?;
    if whose == Whose::Mine {
        return mine(site, &channel, &me, name, after, now);
    }
    channel
        .standing
        .permits(&channel.root, &me.handle(), Ability::Read, now, &revoked)?;

    let place = open(&channel.locator, now)?;
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
        .permits(&channel.root, &peer.handle, Ability::Send, now, &revoked)?;

    let stream = channel.secret.stream(&peer.handle);
    let theirs = track(site, name, &place, &stream, &peer.handle, reach(after))?;
    Ok(Outcome::read(
        name,
        &peer.handle.to_string(),
        &theirs,
        after,
    ))
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
    let place = open(&channel.locator, now)?;
    let stream = channel.secret.stream(&me.handle());
    let ours = track(site, name, &place, &stream, &me.handle(), reach(after))?;
    Ok(Outcome::read(name, &me.handle().to_string(), &ours, after))
}
