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
use kusanagi_kernel::{Instant, PutOutcome, Reader, Segment, Signer, VerifyingKey, Waypoint as _};
use kusanagi_seal::{Fit, Secret, derive, seal};
use kusanagi_site::{Channel, Invite, Peer, Site, Standing};
use kusanagi_waypoint::{Locator, Place};

use crate::assembly::{open, signer};
use crate::walk::peek;
use crate::world::fresh_seed;
use kusanagi_door::Complaint;
use kusanagi_door::Outcome;
use zeroize::Zeroize as _;

/// The height of the introduction stream that carries a newcomer's greeting.
const INTRODUCTION: u64 = 0;

/// What a newcomer says on the introduction stream: a key, then a grant.
///
/// ```text
/// key      VerifyingKey::WIDTH bytes   the newcomer's own verifying key
/// grant    the rest                    bearer -> that key's handle
/// ```
///
/// **The greeting is signed by the one-time bearer key, not by the newcomer.**
/// It has to be: the inviter is about to learn the newcomer's key *from* this
/// message, so a message only that key could authenticate would be one the
/// inviter could not read. The bearer key is the one thing both ends already
/// hold — the invitation carried its seed — and it is also what the
/// introduction stream's address is derived through, so the author of the
/// greeting and the owner of the lane it sits in are now the same identity.
///
/// The key inside is bound to the grant rather than trusted: `greet` refuses it
/// unless the grant it arrives with was issued to that key's handle.
fn greeting(key: &VerifyingKey, grant: &Grant) -> Vec<u8> {
    let mut out = key.as_bytes().to_vec();
    out.extend_from_slice(&grant.to_canonical_bytes());
    out
}

/// Reads a greeting, without deciding whether to believe it.
fn read_greeting(payload: &[u8], name: &str) -> Result<(VerifyingKey, Grant), Complaint> {
    let mut reader = Reader::new(payload);
    let unreadable = |reason: String| Complaint::BadGreeting {
        name: name.to_owned(),
        reason,
    };
    let key = reader
        .take_array::<{ VerifyingKey::WIDTH }>()
        .map(VerifyingKey::from_bytes)
        .map_err(|error| unreadable(error.to_string()))?;
    let rest = reader
        .take(reader.remaining())
        .map_err(|error| unreadable(error.to_string()))?;
    let grant = Grant::from_canonical_bytes(rest).map_err(|error| unreadable(error.to_string()))?;
    Ok((key, grant))
}

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
    // The seed is bound so that it can be erased. Handing `fresh_seed()?`
    // straight to a constructor leaves the bytes in a temporary that lives until
    // the end of the statement and is never overwritten — which is how a channel
    // secret ends up in a core dump of a process that had already finished with
    // it.
    let mut seed = fresh_seed()?;
    let secret = Secret::from_bytes(seed);
    seed.zeroize();
    let bearer_seed = fresh_seed()?;
    let bearer = Signer::from_seed(&bearer_seed);
    let expires_at = now.plus_seconds(lifetime);
    let grant = Grant::issue(&me, &bearer.handle(), Scope::new(abilities, expires_at));

    site.keep(&Channel {
        name: name.to_owned(),
        secret: secret.clone(),
        root: me.handle(),
        introduction: bearer.verifying_key(),
        locator: waypoint.to_owned(),
        standing: Standing::Root,
        peer: None,
    })?;

    let invitation = Invite {
        inviter: me.verifying_key(),
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
    let root = invitation.inviter.handle();
    if root == me.handle() {
        return Err(Complaint::OwnInvitation);
    }
    let revoked = site.revocations()?;

    // What the invitation conveys is checked before it is used, so an expired or
    // malformed one fails here rather than after a stranger's bytes are written.
    let scope = invitation.grant.verify(&root, now, &revoked)?;
    let bearer = invitation.bearer();
    let mine = invitation.grant.attenuate(&bearer, &me.handle(), scope)?;

    let place = open(&invitation.locator, now)?;
    let introduction = invitation.secret.stream(&bearer.handle());
    let announcement = Segment::genesis(
        &bearer,
        &introduction.trail(&bearer),
        greeting(&me.verifying_key(), &mine),
    )?;
    let (address, key) = derive(&introduction, INTRODUCTION);
    let sealed = seal(&key, Fit::Veil, &announcement.to_canonical_bytes())?;

    // The invitation is one-time because this address is write-once. Nothing
    // tracks whether it has been used; the host refuses the second greeting.
    if place.put_if_absent(&address, &sealed)? == PutOutcome::AlreadyPresent {
        return Err(Complaint::InviteSpent);
    }

    site.keep(&Channel {
        name: name.to_owned(),
        secret: invitation.secret.clone(),
        root,
        introduction: bearer.verifying_key(),
        locator: invitation.locator.clone(),
        standing: Standing::Granted(mine),
        peer: Some(Peer {
            key: invitation.inviter,
            standing: Standing::Root,
        }),
    })?;

    Ok(Outcome::Joined {
        name: name.to_owned(),
        handle: me.handle().to_string(),
        peer: root.to_string(),
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
    let introduction = channel.secret.stream(&channel.introduction.handle());
    let Some(said) = peek(place, &introduction, INTRODUCTION, &channel.introduction)? else {
        return Err(Complaint::NoPeerYet {
            name: name.to_owned(),
        });
    };

    let (key, grant) = read_greeting(said.payload(), name)?;
    // Three things have to agree before a stranger becomes the peer: the grant
    // descends from this channel's root, it was issued to the handle of the key
    // the greeting announces, and it permits that handle to write here. The
    // greeting itself was already checked against the one-time key when it was
    // decoded, which is what stops anybody but the invitee putting a key here.
    grant.permits(
        &channel.root,
        &key.handle(),
        Ability::Send,
        now,
        &site.revocations()?,
    )?;

    let channel = Channel {
        peer: Some(Peer {
            key,
            standing: Standing::Granted(grant),
        }),
        ..channel
    };
    site.keep(&channel)?;
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
