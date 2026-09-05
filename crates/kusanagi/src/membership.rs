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

use kusanagi_grant::{Grant, Scope};
use kusanagi_kernel::{
    Freight, Instant, Object, PutOutcome, Reader, Segment, Signer, VerifyingKey, Ward,
    Waypoint as _,
};
use kusanagi_seal::{Fit, Secret, derive, offer, open as open_sealed, period, rendezvous, seal};
use kusanagi_site::{Channel, Invite, Offer, Peer, Roster, Site, Standing};
use kusanagi_waypoint::{Conditional as _, Locator, Place, TtlOutcome};

use crate::assembly::{open, signer, ward};
use crate::lane::Lane;
use crate::request::Habit;
use crate::walk::peek;
use crate::world::fresh_seed;
use kusanagi_door::Complaint;
use kusanagi_door::Grouping;
use kusanagi_door::Outcome;
use zeroize::Zeroize as _;

/// The height of the introduction stream that carries a newcomer's greeting.
const INTRODUCTION: u64 = 0;

/// What a newcomer says on the introduction stream: a key, then a grant.
///
/// ```text
/// key      VerifyingKey::WIDTH bytes   the newcomer's own verifying key
/// ward     2 bytes                     the bin of the host the newcomer reads
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
fn greeting(key: &VerifyingKey, ward: Ward, grant: &Grant) -> Vec<u8> {
    let mut out = key.as_bytes().to_vec();
    out.extend_from_slice(&ward.bits().to_be_bytes());
    out.extend_from_slice(&grant.to_canonical_bytes());
    out
}

/// Reads a greeting, without deciding whether to believe it.
fn read_greeting(payload: &[u8], name: &str) -> Result<(VerifyingKey, Ward, Grant), Complaint> {
    let mut reader = Reader::new(payload);
    let unreadable = |reason: String| Complaint::BadGreeting {
        name: name.to_owned(),
        reason,
    };
    let key = reader
        .take_array::<{ VerifyingKey::WIDTH }>()
        .map(VerifyingKey::from_bytes)
        .map_err(|error| unreadable(error.to_string()))?;
    let ward = reader
        .take_array::<2>()
        .map(|bytes| Ward::from_bits(u16::from_be_bytes(bytes)))
        .map_err(|error| unreadable(error.to_string()))?;
    let rest = reader
        .take(reader.remaining())
        .map_err(|error| unreadable(error.to_string()))?;
    let grant = Grant::from_canonical_bytes(rest).map_err(|error| unreadable(error.to_string()))?;
    Ok((key, ward, grant))
}

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
    let bearer = invitation.bearer();
    let mine = announcement.grant.attenuate(&bearer, &me.handle(), scope)?;

    let introduction = invitation.secret.stream(&bearer.handle());
    let hello = Segment::genesis(
        &bearer,
        &introduction.trail(&bearer),
        Freight::message(greeting(&me.verifying_key(), ward(site)?, &mine))?,
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
    // The introduction stream never releases and never ratchets: it carries one
    // segment, written by a key that exists for that one purpose, and both ends
    // need it openable until somebody reads it.
    let introduction = Lane {
        keys: kusanagi_seal::Keyring::Standing(
            channel.secret.stream(&channel.introduction.handle()),
        ),
        author: channel.introduction,
        // The greeting sits in the rendezvous bin, not in anybody's ward: it is
        // written by somebody this endpoint has not met, so there is no ward for
        // either end to agree on except the one the channel secret produces.
        bin: rendezvous(&channel.secret),
        opened: channel.opened,
    };
    let Some(said) = peek(place, &introduction, name, INTRODUCTION)? else {
        return Err(Complaint::NoPeerYet {
            name: name.to_owned(),
        });
    };

    let (key, peer_ward, grant) = read_greeting(said.payload(), name)?;
    // Two things have to agree before a stranger is recorded as the peer: the
    // grant descends from this channel's root, and it was issued to the handle
    // of the key the greeting announces. What the peer may *do* is not decided
    // here — `read` checks they may send before showing their segments, `send`
    // checks they may read before writing — because discovering who arrived and
    // admitting what they may do are different decisions. The greeting itself
    // was already checked against the one-time key when it was decoded, which
    // is what stops anybody but the invitee putting a key here.
    grant.verify(&channel.root, now, &site.revocations()?)?;
    // The key inside is bound to the grant rather than trusted: a greeting that
    // announced one key and carried a grant issued to another would let anybody
    // redirect this channel's peer at a stranger.
    if grant.holder()? != key.handle() {
        return Err(Complaint::BadGreeting {
            name: name.to_owned(),
            reason: "the greeting's grant was not issued to the key it announces".to_owned(),
        });
    }

    let channel = Channel {
        peer: Some(Peer {
            key,
            ward: peer_ward,
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
