// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who is in a room, and how they get in: founding, inviting, joining.
//!
//! Three verbs that make or change the room record and write nothing on a
//! member's stream. Apart from `chamber_talk.rs` because membership and
//! traffic change for different reasons: a rule about who may invite lands
//! here, a rule about what a read shows lands there, and neither needs the
//! other.
//!
//! **The founder is the room's one authority.** Only the founder's key signs
//! the roster, so only the founder can admit, and so only the founder mints
//! invitations: a line minted by anybody else would be a promise nobody can
//! keep. A founder that goes away leaves a room nobody can join — that is a
//! boundary this build states rather than hides; a successor rule is a later
//! change to the roster's signing, not to this file.

use kusanagi_kernel::{
    Freight, Instant, Object, PutOutcome, Roster, Segment, Signer, Waypoint as _,
};
use kusanagi_seal::{Fit, Secret, derive, offer, open as open_sealed, period, rendezvous, seal};
use kusanagi_site::{Invite, Room, RoomOffer, Site};
use kusanagi_waypoint::Conditional as _;
use kusanagi_waypoint::{Locator, TtlOutcome};

use crate::assembly::{open, signer};
use crate::world::{fresh_seed, fresh_ward};
use kusanagi_door::{Complaint, Outcome};
use zeroize::Zeroize as _;

/// Founds a room: a shared secret, a ward every member sweeps, and a roster
/// naming only the founder, signed by them.
pub(crate) fn room(
    site: &Site,
    name: &str,
    waypoint: &str,
    now: Instant,
) -> Result<Outcome, Complaint> {
    if site.holds(name)? {
        return Err(Complaint::ChannelExists {
            name: name.to_owned(),
        });
    }
    let _: Locator = waypoint.parse()?;
    let me = signer(site)?;
    let mut seed = fresh_seed()?;
    let secret = Secret::from_bytes(seed);
    seed.zeroize();
    let ward = fresh_ward()?;
    let roster = Roster::sign(&me, vec![me.verifying_key()])?;
    site.keep_room(&Room {
        name: name.to_owned(),
        secret,
        ward,
        roster,
        roster_at: None,
        ushers: Vec::new(),
        locator: waypoint.to_owned(),
        opened: period(now.as_unix_seconds()),
    })?;
    Ok(Outcome::RoomFounded {
        name: name.to_owned(),
        ward: ward.to_string(),
        founder: me.handle().to_string(),
    })
}

/// Mints the one line that invites somebody into a room.
///
/// The line carries the room secret, so whoever holds it can read the offer
/// and join; the offer carries the founder's key, the shared ward, and the
/// signed roster. The offer goes to the host before anything is written here,
/// so the two failures are the two harmless ones. The one-time key the line
/// carries is remembered beside the room as an usher: the newcomer greets on
/// that key's stream, and the founder's next read admits them from it.
pub(crate) fn room_invite(
    site: &Site,
    name: &str,
    lifetime: u64,
    now: Instant,
) -> Result<Outcome, Complaint> {
    let mut chamber = site.room(name)?;
    let me = signer(site)?;
    if chamber.founder() != Some(me.handle()) {
        return Err(Complaint::NotTheFounder {
            name: name.to_owned(),
        });
    }
    let place = open(site, &chamber.locator, now)?;
    let (address, key) = offer(&chamber.secret);
    let announcement = RoomOffer {
        founder: me.verifying_key(),
        ward: chamber.ward,
        roster: chamber.roster.clone(),
    };
    let sealed = seal(&key, Fit::Veil, &announcement.to_bytes()?)?;
    let at = Object::new(rendezvous(&chamber.secret), address);
    // A bucket expires objects by lifecycle rule rather than per object; the
    // offer still goes there, and what it loses is the automatic sweep.
    let _: TtlOutcome = place.put_with_ttl(&at, &sealed, lifetime)?;
    let bearer_seed = fresh_seed()?;
    let invitation = Invite {
        secret: chamber.secret.clone(),
        bearer_seed,
        locator: chamber.locator.clone(),
    };
    chamber
        .ushers
        .push(Signer::from_seed(&bearer_seed).verifying_key());
    site.keep_room(&chamber)?;
    Ok(Outcome::RoomInvited {
        name: name.to_owned(),
        invite: invitation.to_string(),
        check: invitation.check(),
        expires_at: now.plus_seconds(lifetime).as_unix_seconds(),
    })
}

/// Accepts a room invitation: reads the founder's offer, checks the roster
/// signature against the founder's key, greets, and records the room.
///
/// The roster is believed only under the founder's own key, which the same
/// drop carries: a roster moved under another founder's key fails here rather
/// than being shown. A newcomer is not on that roster yet, and that is
/// expected: they announce themselves on the introduction stream the
/// invitation's one-time key derives, and the founder admits them from it.
/// Joining one's own room is refused, for the same reason joining one's own
/// channel is — two local names for one stream.
pub(crate) fn room_join(
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
    let place = open(site, &invitation.locator, now)?;
    let (offered_at, offer_key) = offer(&invitation.secret);
    let rendezvous_bin = rendezvous(&invitation.secret);
    let Some(sealed) = place.get(&Object::new(rendezvous_bin, offered_at))? else {
        return Err(Complaint::NoInvitation);
    };
    let announcement = RoomOffer::from_bytes(&open_sealed(&offer_key, Fit::Veil, &sealed)?)?;
    let founder = announcement.founder.handle();
    if founder == me.handle() {
        return Err(Complaint::OwnInvitation);
    }
    announcement
        .roster
        .verify(&announcement.founder)
        .map_err(|_| Complaint::BadInvitation {
            reason: "this room's roster was not signed by its founder".to_owned(),
        })?;
    // The greeting carries the newcomer's key and nothing else: the founder
    // learns who arrived beside nothing else to trust, and reads it once.
    let bearer = invitation.bearer();
    let introduction = invitation.secret.stream(&bearer.handle());
    let hello = Segment::genesis(
        &bearer,
        &introduction.trail(&bearer),
        Freight::message(me.verifying_key().as_bytes().to_vec())?,
    )?;
    let (greeting_at, greeting_key) = derive(&introduction, 0);
    let sealed = seal(&greeting_key, Fit::Veil, &hello.to_canonical_bytes())?;
    if place.put_if_absent(&Object::new(rendezvous_bin, greeting_at), &sealed)?
        == PutOutcome::AlreadyPresent
    {
        return Err(Complaint::InviteSpent);
    }
    site.keep_room(&Room {
        name: name.to_owned(),
        secret: invitation.secret.clone(),
        ward: announcement.ward,
        roster: announcement.roster,
        roster_at: None,
        ushers: Vec::new(),
        locator: invitation.locator.clone(),
        opened: period(now.as_unix_seconds()),
    })?;
    Ok(Outcome::RoomJoined {
        name: name.to_owned(),
        handle: me.handle().to_string(),
        founder: founder.to_string(),
        check: invitation.check(),
    })
}
