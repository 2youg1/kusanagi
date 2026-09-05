// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What moves in a room: one segment out, every member's stream in, and the
//! roster segment that admits a newcomer.
//!
//! Apart from `chamber.rs` because these three write on streams and those
//! three write only the record. Every member's lane derives from the room
//! secret through their own handle and is filed in the room's one ward, so a
//! member writes a stream only they can write and a read takes one ward and
//! matches every lane on this machine — one sweep, however many members.

use std::collections::BTreeMap;

use kusanagi_kernel::{Bin, Freight, Handle, Instant, Purpose, Roster, VerifyingKey};
use kusanagi_seal::{Keyring, period, rendezvous};
use kusanagi_site::{Room, Site};
use kusanagi_walk::{Lane, Reach, Walked, peek, track, track_all, verified};

use crate::assembly::{open, signer};
use crate::traffic::append;
use kusanagi_door::{Complaint, Outcome};

/// One member's lane in a room: their stream under the room secret, filed in
/// the room's ward.
///
/// A room never releases, so the keyring is always standing: there is no
/// ratchet to burn behind, and history is what a room is for.
fn lane_of(room: &Room, author: &VerifyingKey, now: Instant) -> Lane {
    Lane {
        keys: Keyring::Standing(room.secret.stream(&author.handle())),
        author: *author,
        bin: Bin::new(period(now.as_unix_seconds()), room.ward),
        opened: room.opened,
    }
}

/// Appends one segment to this endpoint's stream in a room.
///
/// The same shape as a channel send with the two-party parts removed: no
/// standing to check, no greeting, no release. **No roster check on the way
/// out either**: the room secret is the capability, and a member the roster
/// does not name yet — joined, not yet admitted — writes a stream the
/// founder's next read picks up from its genesis. What the roster decides is
/// who a read reports, and it decides that on every reader's machine.
pub(crate) fn room_send(
    site: &Site,
    name: &str,
    payload: &[u8],
    now: Instant,
) -> Result<Outcome, Complaint> {
    let chamber = site.room(name)?;
    let me = signer(site)?;
    let place = open(site, &chamber.locator, now)?;
    let mine = lane_of(&chamber, &me.verifying_key(), now);
    let walked = track(site, name, &place, &mine, Reach::Head, now)?;
    let acknowledged = verified(site, name, &me.handle())?;
    let freight = Freight::message(payload.to_vec())?.acknowledging(acknowledged);
    let written = append(site, name, &place, &mine, &me, freight, walked)?;
    Ok(Outcome::RoomSent {
        name: name.to_owned(),
        index: written.index,
        address: written.address,
    })
}

/// Reads a room: one sweep of its ward, every member's stream verified.
///
/// `after` holds, per author, the height the caller already has; an author not
/// in it is shown whole. Roster segments on the founder's stream replace the
/// roster as they are met and are never reported; a member they admit is
/// walked on this same read, at the cost of one more sweep on the read that
/// first sees them.
///
/// **The founder is walked from no higher than the roster was read at**, so a
/// process killed between the walk marking its cairn and this record being
/// written meets the same roster segment again on the next read instead of
/// resuming past it. That is what keeps a kill from changing a result.
pub(crate) fn room_read(
    site: &Site,
    name: &str,
    after: &BTreeMap<String, u64>,
    now: Instant,
) -> Result<Outcome, Complaint> {
    let chamber = site.room(name)?;
    let place = open(site, &chamber.locator, now)?;
    let me = signer(site)?;
    let mut chamber = if chamber.founder() == Some(me.handle()) {
        admit(site, name, &place, chamber, &me, now)?
    } else {
        chamber
    };
    let founder = chamber.roster.members().first().copied();
    let roster_at = chamber.roster_at;
    let reach = |member: &VerifyingKey| {
        let asked = after.get(&member.handle().to_string()).copied();
        let floor = if Some(*member) == founder {
            asked
                .zip(roster_at)
                .map(|(asked, read_at)| asked.min(read_at))
        } else {
            asked
        };
        floor.map_or(Reach::Whole, Reach::Above)
    };
    let mut walked: BTreeMap<Handle, Walked> = BTreeMap::new();
    loop {
        let lanes: Vec<Lane> = chamber
            .roster
            .members()
            .iter()
            .filter(|member| !walked.contains_key(&member.handle()))
            .map(|member| lane_of(&chamber, member, now))
            .collect();
        if lanes.is_empty() {
            break;
        }
        let asked: Vec<(&Lane, Reach)> = lanes
            .iter()
            .map(|lane| (lane, reach(&lane.author)))
            .collect();
        for (lane, done) in lanes
            .iter()
            .zip(track_all(site, name, &place, &asked, now)?)
        {
            walked.insert(lane.author.handle(), done);
        }
        if let Some(founder) = founder
            && let Some(done) = walked.get(&founder.handle())
        {
            chamber.roster = latest_roster(&founder, chamber.roster, done)?;
            chamber.roster_at = done.head().map(|head| head.index());
        }
    }
    site.keep_room(&chamber)?;
    Ok(kusanagi_door::chamber::reported(
        name,
        chamber.roster.members().iter().map(|member| {
            let author = member.handle();
            let floor = after.get(&author.to_string()).copied();
            let done = walked.get(&author);
            (
                author.to_string(),
                done.and_then(Walked::head).map(|head| head.index()),
                done.map_or_else(Vec::new, |done| {
                    done.held()
                        .iter()
                        .filter(|held| held.segment.purpose() == Purpose::Message)
                        .filter(|held| floor.is_none_or(|floor| held.segment.index() > floor))
                        .map(|held| {
                            (
                                held.segment.index(),
                                held.segment.acknowledged(),
                                held.segment.payload(),
                            )
                        })
                        .collect()
                }),
            )
        }),
    ))
}

/// The roster as the founder last signed it on their stream, or `current`
/// when no later one was met.
///
/// A roster segment on the founder's own verified stream is already the
/// founder's word; the signature check is what makes a roster carried out of
/// a record believed, and it is repeated here so one rule holds everywhere. A
/// roster segment that is not a roster is refused rather than skipped: the
/// founder's build wrote it, and a room whose founder speaks nonsense is a
/// room to report, not one to read around.
fn latest_roster(
    founder: &VerifyingKey,
    current: Roster,
    walked: &Walked,
) -> Result<Roster, Complaint> {
    let mut latest = current;
    for held in walked.held() {
        if held.segment.purpose() != Purpose::Roster {
            continue;
        }
        let roster = Roster::from_bytes(held.segment.payload())?;
        roster.verify(founder)?;
        latest = roster;
    }
    Ok(latest)
}

/// Admits whoever greeted since the founder last looked.
///
/// Each invitation minted here named a one-time usher key, and each newcomer
/// wrote their own verifying key on that key's stream at height zero. Every
/// greeting found names a member; the roster is re-signed once with all of
/// them and travels once, as a roster segment on the founder's stream, so
/// every member's next read replaces theirs. An usher whose greeting was read
/// is spent and forgotten, so a read never asks about it again.
fn admit(
    site: &Site,
    name: &str,
    place: &(impl kusanagi_kernel::Waypoint + kusanagi_kernel::Listing + Sync),
    mut chamber: Room,
    me: &kusanagi_kernel::Signer,
    now: Instant,
) -> Result<Room, Complaint> {
    let rendezvous_bin = rendezvous(&chamber.secret);
    let mut members = chamber.roster.members().to_vec();
    let mut waiting = Vec::with_capacity(chamber.ushers.len());
    for usher in chamber.ushers.drain(..) {
        let introduction = Lane {
            keys: Keyring::Standing(chamber.secret.stream(&usher.handle())),
            author: usher,
            bin: rendezvous_bin,
            opened: chamber.opened,
        };
        let Some(hello) = peek(place, &introduction, name, 0)? else {
            waiting.push(usher);
            continue;
        };
        let raw: [u8; VerifyingKey::WIDTH] =
            hello
                .payload()
                .try_into()
                .map_err(|_| Complaint::BadGreeting {
                    name: name.to_owned(),
                    reason: "a room greeting does not carry a key".to_owned(),
                })?;
        let key = VerifyingKey::from_bytes(raw);
        if !members.contains(&key) {
            members.push(key);
        }
    }
    chamber.ushers = waiting;
    if members.len() == chamber.roster.members().len() {
        return Ok(chamber);
    }
    chamber.roster = Roster::sign(me, members)?;
    let mine = lane_of(&chamber, &me.verifying_key(), now);
    let walked = track(site, name, place, &mine, Reach::Head, now)?;
    let freight = Freight::roster(chamber.roster.to_bytes()?)?;
    let written = append(site, name, place, &mine, me, freight, walked)?;
    chamber.roster_at = Some(written.index);
    site.keep_room(&chamber)?;
    Ok(chamber)
}
