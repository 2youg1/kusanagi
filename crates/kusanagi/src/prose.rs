// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The same outcome, said to a person.
//!
//! This is the second of the two renderings `report.rs` promises, and it is a
//! separate file because it answers a different question: not *what happened*,
//! which is the value, but *what somebody reading a terminal needs to see first*.
//! Column widths, the words `cut off`, and the warning under `forgotten` all
//! belong here and nowhere near the value a machine parses.

use crate::report::{Entry, Measured, Outcome, Summary};

/// Renders one outcome as prose.
pub fn render(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Identity { handle, site } => {
            format!("this endpoint is {handle}\n  site  {site}")
        }
        Outcome::Channels { channels } if channels.is_empty() => {
            "no channels yet; `kusanagi invite` starts one".to_owned()
        }
        Outcome::Channels { channels } => listing(channels),
        Outcome::Invited {
            name,
            invite,
            expires_at,
        } => format!(
            "channel `{name}` is open, and expires at {expires_at}\n\n{invite}\n\n\
             hand that line over once. Anybody who holds it can join, so treat it \
             the way you would treat a key."
        ),
        Outcome::Joined {
            name,
            handle,
            peer,
            waypoint,
        } => format!(
            "joined `{name}`\n  you       {handle}\n  peer      {peer}\n  waypoint  {waypoint}"
        ),
        Outcome::Sent {
            name,
            index,
            id,
            address,
        } => format!("sent on `{name}` #{index}\n  id      {id}\n  address {address}"),
        Outcome::Read {
            name,
            author,
            height,
            segments,
        } => stream(name, author, *height, segments),
        Outcome::Revoked { name, step } => format!(
            "the peer of `{name}` is cut off\n  step  {step}\n\
             nothing they write from now on will be accepted here."
        ),
        Outcome::Forgotten { name, waypoint } => format!(
            "`{name}` is gone from this endpoint\n  waypoint  {waypoint}\n\
             the drops stay where they are, and the secret that opened them does not. \
             This channel cannot be re-entered, by this invitation or any copy of it."
        ),
        Outcome::Examined {
            waypoint,
            kind,
            tier,
            capabilities,
        } => certificate(waypoint, kind, tier, capabilities),
        Outcome::Hosted { address, directory } => {
            format!("stopped hosting {directory} on {address}")
        }
    }
}

/// The channel table, one row each.
///
/// The authority column is what a person opens this listing to see: whether the
/// channel still works, and until when.
fn listing(channels: &[Summary]) -> String {
    let mut lines = vec![format!("{} channel(s)", channels.len())];
    lines.extend(channels.iter().map(|channel| {
        let authority = match (channel.refused, channel.expires_at) {
            (Some(code), _) => format!("nothing: {code}"),
            (None, None) => channel.can.join(","),
            (None, Some(until)) => format!("{} until {until}", channel.can.join(",")),
        };
        let peer = match (&channel.peer, channel.peer_refused) {
            (None, _) => "(nobody has joined yet)".to_owned(),
            (Some(peer), None) => peer.clone(),
            (Some(peer), Some(code)) => format!("{peer} — cut off ({code})"),
        };
        // The waypoint goes last because it is the one column with no bound on
        // its width: a long locator then runs off the end instead of pushing
        // everything after it out of line.
        format!(
            "  {:<16} {:<8} {:<30} {:<40} {}",
            channel.name, channel.standing, authority, peer, channel.waypoint,
        )
    }));
    lines.join("\n")
}

/// One verified stream, header then payloads as text.
fn stream(name: &str, author: &str, height: Option<u64>, segments: &[Entry]) -> String {
    let header = match height {
        None => format!("`{name}`: {author} has written nothing yet"),
        Some(height) => format!(
            "`{name}`: {author} verifies to height {height} ({} segment(s))",
            segments.len()
        ),
    };
    let mut lines = vec![header];
    lines.extend(
        segments
            .iter()
            .map(|entry| format!("  #{:<3} {}", entry.index, entry.text)),
    );
    lines.join("\n")
}

/// What a host was measured to do, capability by capability.
fn certificate(waypoint: &str, kind: &str, tier: &str, capabilities: &[Measured]) -> String {
    let mut lines = vec![
        format!("{waypoint}\n  kind  {kind}\n  tier  {tier}"),
        String::new(),
    ];
    lines.extend(capabilities.iter().map(|measured| {
        let detail = measured
            .detail
            .as_ref()
            .map_or_else(String::new, |detail| format!(" — {detail}"));
        format!("  {:<18} {}{detail}", measured.capability, measured.verdict)
    }));
    lines.join("\n")
}
