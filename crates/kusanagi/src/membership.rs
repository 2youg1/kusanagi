// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who is on a channel, and how they stop being on it.
//!
//! Five verbs that all change the same record: minting an invitation, accepting
//! one, learning who accepted, cutting them off, and dropping the channel here.
//! They are together because they share one question — *what does this endpoint
//! know about the other end* — and apart from `traffic.rs` because none of them
//! moves a payload.

use kusanagi_grant::{Ability, Grant, Scope};
use kusanagi_kernel::{Instant, PutOutcome, Segment, Signer, Waypoint as _};
use kusanagi_seal::{Secret, derive, seal};
use kusanagi_site::{Channel, Invite, Peer, Site, Standing};
use kusanagi_waypoint::{Locator, Place};

use crate::assembly::{open, signer};
use crate::complaint::Complaint;
use crate::report::Outcome;
use crate::walk::peek;
use crate::world::fresh_seed;

/// The height of the introduction stream that carries a newcomer's greeting.
const INTRODUCTION: u64 = 0;

pub(crate) fn invite(
    site: &Site,
    name: &str,
    waypoint: &str,
    lifetime: u64,
    abilities: kusanagi_grant::Abilities,
    now: Instant,
) -> Result<Outcome, Complaint> {
    if site.holds(name)? {
        return Err(Complaint::ChannelExists {
            name: name.to_owned(),
        });
    }
    // Parsed before anything is written, so a mistyped locator costs nothing.
    let _: Locator = waypoint.parse()?;

    let me = signer(site)?;
    let secret = Secret::from_bytes(fresh_seed()?);
    let bearer_seed = fresh_seed()?;
    let bearer = Signer::from_seed(&bearer_seed);
    let expires_at = now.plus_seconds(lifetime);
    let grant = Grant::issue(&me, &bearer.handle(), Scope::new(abilities, expires_at));

    site.keep(
        name,
        &Channel {
            secret: secret.clone(),
            root: me.handle(),
            introduction: bearer.handle(),
            locator: waypoint.to_owned(),
            standing: Standing::Root,
            peer: None,
        },
    )?;

    let invitation = Invite {
        inviter: me.handle(),
        secret,
        bearer_seed,
        locator: waypoint.to_owned(),
        grant,
    };
    Ok(Outcome::Invited {
        name: name.to_owned(),
        invite: invitation.to_string(),
        expires_at: expires_at.as_unix_seconds(),
        expires_in: lifetime,
    })
}

pub(crate) fn join(
    site: &Site,
    text: &str,
    name: &str,
    now: Instant,
) -> Result<Outcome, Complaint> {
    if site.holds(name)? {
        return Err(Complaint::ChannelExists {
            name: name.to_owned(),
        });
    }
    let invitation = Invite::parse(text)?;
    let me = signer(site)?;
    // An invitation is for somebody else. A stream is derived from the channel
    // secret and the author's handle, so an endpoint that accepted its own would
    // hold two local names for one stream, discover itself as the peer, and read
    // its own segments back as though a peer had written them. Refusing here is
    // what keeps "one channel, two parties" true.
    if invitation.inviter == me.handle() {
        return Err(Complaint::OwnInvitation);
    }
    let revoked = site.revocations()?;

    // What the invitation conveys is checked before it is used, so an expired or
    // malformed one fails here rather than after a stranger's bytes are written.
    let scope = invitation
        .grant
        .verify(&invitation.inviter, now, &revoked)?;
    let bearer = invitation.bearer();
    let mine = invitation.grant.attenuate(&bearer, &me.handle(), scope)?;

    let place = open(&invitation.locator, now)?;
    let introduction = invitation.secret.stream(&bearer.handle());
    let greeting = Segment::genesis(&me, mine.to_canonical_bytes())?;
    let (address, key) = derive(&introduction, INTRODUCTION);
    let sealed = seal(&key, &greeting.to_canonical_bytes())?;

    // The invitation is one-time because this address is write-once. Nothing
    // tracks whether it has been used; the host refuses the second greeting.
    if place.put_if_absent(&address, &sealed)? == PutOutcome::AlreadyPresent {
        return Err(Complaint::InviteSpent);
    }

    site.keep(
        name,
        &Channel {
            secret: invitation.secret.clone(),
            root: invitation.inviter,
            introduction: bearer.handle(),
            locator: invitation.locator.clone(),
            standing: Standing::Granted(mine),
            peer: Some(Peer {
                handle: invitation.inviter,
                standing: Standing::Root,
            }),
        },
    )?;

    Ok(Outcome::Joined {
        name: name.to_owned(),
        handle: me.handle().to_string(),
        peer: invitation.inviter.to_string(),
        waypoint: invitation.locator,
    })
}

/// Learns who accepted an invitation, from the introduction stream.
///
/// This is the one place a read writes: the peer it discovers is a fact that has
/// been verified against the channel's own root, and re-deriving it on every
/// command would mean paying for a request that can only ever give the same
/// answer.
pub(crate) fn greet(
    site: &Site,
    name: &str,
    channel: Channel,
    place: &Place,
    now: Instant,
) -> Result<Channel, Complaint> {
    let introduction = channel.secret.stream(&channel.introduction);
    let Some(greeting) = peek(place, &introduction, INTRODUCTION)? else {
        return Err(Complaint::NoPeerYet {
            name: name.to_owned(),
        });
    };

    let grant = Grant::from_canonical_bytes(greeting.payload())?;
    // Three things have to agree before a stranger becomes the peer: the grant
    // descends from this channel's root, it was issued to the handle that signed
    // the greeting, and it permits that handle to write here.
    grant.permits(
        &channel.root,
        &greeting.author(),
        Ability::Send,
        now,
        &site.revocations()?,
    )?;

    let channel = Channel {
        peer: Some(Peer {
            handle: greeting.author(),
            standing: Standing::Granted(grant),
        }),
        ..channel
    };
    site.keep(name, &channel)?;
    Ok(channel)
}

pub(crate) fn revoke(site: &Site, name: &str) -> Result<Outcome, Complaint> {
    let channel = site.channel(name)?;
    let peer = channel.peer.ok_or_else(|| Complaint::NoPeerYet {
        name: name.to_owned(),
    })?;
    let step = peer
        .standing
        .grant()
        .and_then(|grant| grant.steps().last())
        .ok_or_else(|| Complaint::CannotRevokeRoot {
            name: name.to_owned(),
        })?
        .id();

    site.revoke(step)?;
    Ok(Outcome::Revoked {
        name: name.to_owned(),
        step: step.to_string(),
    })
}

/// Removes one channel from this endpoint and tells nobody.
///
/// Revoking and forgetting are not two spellings of one act. Revoking is a
/// statement about the world that survives here and is enforced on every later
/// read; forgetting is this machine dropping a key, which the peer cannot
/// observe and the host cannot be asked to help with. Doing both from one verb
/// would mean a caller who wanted one always got the other.
pub(crate) fn forget(site: &Site, name: &str) -> Result<Outcome, Complaint> {
    let channel = site.channel(name)?;
    site.forget(name)?;
    Ok(Outcome::Forgotten {
        name: name.to_owned(),
        waypoint: channel.locator,
    })
}
