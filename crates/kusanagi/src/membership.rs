// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who is on a channel, and how they stop being on it.
//!
//! Five verbs that all change the same record: minting an invitation, accepting
//! one, cutting the peer off, grouping channels, and dropping one here. They are
//! together because they share one question — *what does this endpoint know
//! about the other end* — and apart from `traffic.rs` because none of them
//! moves a payload. Learning who accepted is `greeting.rs`, which owns the one
//! message both `join` and `greet` have to agree on.

use kusanagi_grant::{Grant, Scope};
use kusanagi_kernel::{Freight, Instant, Object, PutOutcome, Segment, Signer, Waypoint as _};
use kusanagi_seal::{Fit, Secret, derive, offer, open as open_sealed, period, rendezvous, seal};
use kusanagi_site::{Channel, Invite, Offer, Peer, Roster, Site, Standing};
use kusanagi_waypoint::{Conditional as _, Locator, TtlOutcome};

use crate::assembly::{declared, open, signer, ward};
use crate::greeting::{INTRODUCTION, believed, greeting};
use crate::request::Habit;
use crate::world::fresh_seed;
use kusanagi_door::Complaint;
use kusanagi_door::Grouping;
use kusanagi_door::Outcome;
use zeroize::Zeroize as _;

pub(crate) fn invite(
    site: &Site,
    name: &str,
    waypoint: &str,
    lifetime: u64,
    abilities: kusanagi_grant::Abilities,
    habit: Habit,
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
    // Which bin this endpoint reads. It goes into the offer because a writer
    // cannot deliver to a reader whose corner of the host it does not know.
    let my_ward = ward(site)?;
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

    // The offer goes to the host **before** the channel goes on the disk, so
    // the two failures are the two harmless ones. A host that will not take it
    // leaves nothing here to clean up; a disk that will not take the record
    // leaves an offer nobody holds the key to, which the lifetime sweeps away.
    let place = open(site, waypoint, now)?;
    let (address, key) = offer(&secret);
    let at = Object::new(rendezvous(&secret), address);
    let announcement = Offer {
        inviter: me.verifying_key(),
        ward: my_ward,
        retention: habit.retention,
        declaration: declared(site, &me)?,
        grant,
    };
    let sealed = seal(&key, Fit::Veil, &announcement.to_bytes())?;
    if place.put_with_ttl(&at, &sealed, lifetime)? == TtlOutcome::NotOffered {
        // A bucket expires objects by lifecycle rule rather than per object.
        // The offer still goes there; what it loses is the automatic sweep, and
        // `doctor` reports that about a host before anybody trusts it with a
        // channel.
    }

    let invitation = Invite {
        secret: secret.clone(),
        bearer_seed,
        locator: waypoint.to_owned(),
    };
    let check = invitation.check();

    site.keep(&Channel {
        name: name.to_owned(),
        secret,
        root: me.handle(),
        introduction: bearer.verifying_key(),
        locator: waypoint.to_owned(),
        standing: Standing::Root,
        cadence: habit.cadence,
        retention: habit.retention,
        opened: period(now.as_unix_seconds()),
        peer: None,
    })?;

    Ok(Outcome::Invited {
        name: name.to_owned(),
        invite: invitation.to_string(),
        check,
        expires_at: expires_at.as_unix_seconds(),
        expires_in: lifetime,
    })
}

pub(crate) fn join(
    site: &Site,
    text: &str,
    name: &str,
    habit: Habit,
    now: Instant,
) -> Result<Outcome, Complaint> {
    if site.holds(name)? {
        return Err(Complaint::ChannelExists {
            name: name.to_owned(),
        });
    }
    let invitation = Invite::parse(text)?;
    let me = signer(site)?;
    let place = open(site, &invitation.locator, now)?;

    // The line says where to look and holds the key to look with; who is
    // inviting, and by what authority, is in the drop it points at.
    let (offered_at, offer_key) = offer(&invitation.secret);
    let rendezvous_bin = rendezvous(&invitation.secret);
    let Some(sealed) = place.get(&Object::new(rendezvous_bin, offered_at))? else {
        return Err(Complaint::NoInvitation);
    };
    let announcement = Offer::from_bytes(&open_sealed(&offer_key, Fit::Veil, &sealed)?)?;

    // An invitation is for somebody else. A stream is derived from the channel
    // secret and the author's handle, so an endpoint that accepted its own would
    // hold two local names for one stream, discover itself as the peer, and read
    // its own segments back as though a peer had written them. Refusing here is
    // what keeps "one channel, two parties" true.
    let root = announcement.inviter.handle();
    if root == me.handle() {
        return Err(Complaint::OwnInvitation);
    }
    let revoked = site.revocations()?;

    // What the invitation conveys is checked before it is used, so an expired or
    // malformed one fails here rather than after a stranger's bytes are written.
    // A grant rooted in anybody but the inviter named beside it is refused by
    // `verify` as `grant.wrong_root`.
    let scope = announcement.grant.verify(&root, now, &revoked)?;
    // The inviter's name is believed only under the inviter's own key.
    let alias = believed(announcement.declaration.as_ref(), &announcement.inviter)?;
    let bearer = invitation.bearer();
    let mine = announcement.grant.attenuate(&bearer, &me.handle(), scope)?;

    let introduction = invitation.secret.stream(&bearer.handle());
    let hello = Segment::genesis(
        &bearer,
        &introduction.trail(&bearer),
        Freight::message(greeting(
            &me.verifying_key(),
            ward(site)?,
            declared(site, &me)?.as_ref(),
            &mine,
        ))?,
    )?;
    let (greeting_at, greeting_key) = derive(&introduction, INTRODUCTION);
    let sealed = seal(&greeting_key, Fit::Veil, &hello.to_canonical_bytes())?;

    // The invitation is one-time because this address is write-once. Nothing
    // tracks whether it has been used; the host refuses the second greeting.
    if place.put_if_absent(&Object::new(rendezvous_bin, greeting_at), &sealed)?
        == PutOutcome::AlreadyPresent
    {
        return Err(Complaint::InviteSpent);
    }

    site.keep(&Channel {
        name: name.to_owned(),
        secret: invitation.secret.clone(),
        root,
        introduction: bearer.verifying_key(),
        locator: invitation.locator.clone(),
        standing: Standing::Granted(mine),
        cadence: habit.cadence,
        // The inviter's choice, not this end's: retention decides the key
        // schedule, and a channel is one schedule.
        retention: announcement.retention,
        opened: period(now.as_unix_seconds()),
        peer: Some(Peer {
            key: announcement.inviter,
            ward: announcement.ward,
            standing: Standing::Root,
            alias,
        }),
    })?;

    Ok(Outcome::Joined {
        name: name.to_owned(),
        handle: me.handle().to_string(),
        peer: root.to_string(),
        check: invitation.check(),
        waypoint: invitation.locator,
        retention: announcement.retention.word(),
    })
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

/// Replaces one group's roster, after checking every member is a channel here.
///
/// **Checked now rather than at fan-out time**, because a roster naming a
/// channel that does not exist is a roster that will fail for that member on
/// every send, and the person writing it is the one who can fix it. A member
/// that is forgotten later still fails at fan-out, and that is a row in the
/// report rather than a refusal of the whole send.
pub(crate) fn group(site: &Site, name: &str, members: &[String]) -> Result<Outcome, Complaint> {
    for member in members {
        if !site.holds(member)? {
            return Err(Complaint::UnknownChannel {
                name: member.clone(),
            });
        }
    }
    let roster = Roster {
        name: name.to_owned(),
        members: members.to_vec(),
    };
    site.enrol(&roster)?;
    Ok(Outcome::Grouped {
        group: Grouping {
            name: roster.name,
            members: roster.members,
        },
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
