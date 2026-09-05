// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One sentence per outcome that fits in one.
//!
//! Apart from `prose.rs` because that dispatch is at both its limits: the
//! function past one hundred lines, the file past four hundred. Each sentence
//! here is a function of its fields, said once, so the dispatch stays one line
//! per outcome.

use crate::rows::Grouping;

/// What cutting a peer off says: the step that no longer counts.
pub(crate) fn severed(name: &str, step: &str) -> String {
    format!(
        "the peer of `{name}` is cut off\n  step  {step}\n\
         nothing they write from now on will be accepted here."
    )
}

/// What deleting a channel says: the drops stay, the secret does not.
pub(crate) fn forgotten(name: &str, waypoint: &str) -> String {
    format!(
        "`{name}` is gone from this endpoint\n  waypoint  {waypoint}\n\
         the drops stay where they are, and the secret that opened them does not. \
         This channel cannot be re-entered, by this invitation or any copy of it."
    )
}

/// What a finished service says: how many calls it answered.
pub(crate) fn served(calls: u64) -> String {
    format!("answered {calls} call(s); the agent closed the pipe")
}

/// What a minted invitation says: the line, and the check code beside it.
pub(crate) fn welcomed(
    name: &str,
    invite: &str,
    check: &str,
    expires_at: u64,
    expires_in: u64,
) -> String {
    format!(
        "channel `{name}` is open. This invitation lasts {}, until {expires_at}\n\n{invite}\n\n\
         hand that line over once. Anybody who holds it can join, so treat it \
         the way you would treat a key.\n\n\
         check code {check} \u{2014} read it out to whoever you gave the line to. If their \
         `join` shows anything else, the line was altered on the way.",
        lasting(expires_in)
    )
}

/// A span of seconds, in the largest unit that still says something.
///
/// Beside `welcomed` because that is its only caller: a person reading an
/// invitation wants to know whether to act today, and the exact instant is in
/// `expires_at` for whatever needs to compute with it.
fn lasting(seconds: u64) -> String {
    match seconds {
        0 => "no longer".to_owned(),
        1..=90 => format!("{seconds}s"),
        91..=5_400 => format!("{}m", seconds / 60),
        5_401..=172_800 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// What an accepted invitation says: who arrived, and who invited them.
pub(crate) fn greeted(
    name: &str,
    handle: &str,
    peer: &str,
    check: &str,
    waypoint: &str,
    retention: &str,
) -> String {
    format!(
        "joined `{name}`\n  you       {handle}\n  peer      {peer}\n  waypoint  {waypoint}\n  \
         retention {retention}\n\
         \n  check code {check} \u{2014} it should match what the person who invited you says"
    )
}

/// What an appended segment says: where it was left.
pub(crate) fn posted(name: &str, index: u64, id: &str, address: &str) -> String {
    format!("sent on `{name}` #{index}\n  id      {id}\n  address {address}")
}

/// What one group stands for, said once so the dispatch stays short.
pub(crate) fn enrolled(group: &Grouping) -> String {
    format!(
        "group `{}` now stands for {} channel(s){}\n\
         sending to it writes one drop per member, and nothing is shared between them.",
        group.name,
        group.members.len(),
        members(group)
    )
}

/// The members of one group, one per line, or a sentence saying there are none.
pub(crate) fn members(group: &Grouping) -> String {
    if group.members.is_empty() {
        return "\n  (nobody \u{2014} a message to it goes nowhere)".to_owned();
    }
    let mut listed = String::new();
    for member in &group.members {
        listed.push_str("\n  ");
        listed.push_str(member);
    }
    listed
}
