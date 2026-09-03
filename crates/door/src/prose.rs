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

use crate::fence::Fence;
use crate::report::Outcome;
use crate::rows::{Entry, Measured, Summary};

/// Renders one outcome as prose.
pub fn render(outcome: &Outcome, fence: Fence) -> String {
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
            check,
            expires_at,
            expires_in,
        } => format!(
            "channel `{name}` is open. This invitation lasts {}, until {expires_at}\n\n{invite}\n\n\
             hand that line over once. Anybody who holds it can join, so treat it \
             the way you would treat a key.\n\n\
             check code {check} \u{2014} read it out to whoever you gave the line to. If their \
             `join` shows anything else, the line was altered on the way.",
            lasting(*expires_in)
        ),
        Outcome::Joined {
            name,
            handle,
            peer,
            check,
            waypoint,
        } => format!(
            "joined `{name}`\n  you       {handle}\n  peer      {peer}\n  waypoint  {waypoint}\n\
             \n  check code {check} \u{2014} it should match what the person who invited you says"
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
        } => stream(name, author, *height, segments, fence),
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
        Outcome::Exported { recovery, archive } => format!(
            "{} bytes of archive are on stdout. The key that opens them is\n\n  {recovery}\n\n\
             write it down now: it is shown once, it is stored nowhere, and without \
             it the archive is noise.",
            archive.len()
        ),
        Outcome::Imported { site, channels } => {
            format!("restored {channels} channel(s) into {site}")
        }
        Outcome::Hosted { address, directory } => {
            format!("stopped hosting {directory} on {address}")
        }
    }
}

/// A span of seconds, in the largest unit that still says something.
///
/// A person reading a listing wants to know whether to act today; the exact
/// instant is in `expires_at` for whatever needs to compute with it.
fn lasting(seconds: u64) -> String {
    match seconds {
        0 => "no longer".to_owned(),
        1..=90 => format!("{seconds}s"),
        91..=5_400 => format!("{}m", seconds / 60),
        5_401..=172_800 => format!("{}h", seconds / 3_600),
        _ => format!("{}d", seconds / 86_400),
    }
}

/// The channel table, one row each.
///
/// The authority column is what a person opens this listing to see: whether the
/// channel still works, and until when.
fn listing(channels: &[Summary]) -> String {
    let mut lines = vec![format!("{} channel(s)", channels.len())];
    lines.extend(channels.iter().map(|channel| {
        let authority = match (channel.refused, channel.expires_in) {
            (Some(code), _) => format!("nothing: {code}"),
            (None, None) => channel.can.join(","),
            (None, Some(left)) => format!("{} for {}", channel.can.join(","), lasting(left)),
        };
        let peer = match (&channel.peer, channel.peer_refused) {
            (None, _) => "(nobody met yet)".to_owned(),
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

/// One verified stream, header then payloads inside a fence.
///
/// **No byte a peer wrote ever shares a line with a byte kusanagi wrote.** The
/// index and the size are this program speaking; everything between the tags is
/// the other end, and the tags are what says so to a reader with no parser. See
/// `fence.rs` for why that reader is the one to design for.
fn stream(
    name: &str,
    author: &str,
    height: Option<u64>,
    segments: &[Entry],
    fence: Fence,
) -> String {
    // The listing abbreviates a handle and so does this: the full one is in the
    // `author` field for whatever needs to match on it.
    let who: String = author.chars().take(12).collect();
    let header = match height {
        None => format!("`{name}`: {who} has written nothing yet"),
        Some(height) => format!(
            "`{name}`: {who} verifies to height {height} ({} segment(s))",
            segments.len()
        ),
    };
    let mut lines = vec![header];
    for entry in segments {
        lines.push(format!("  #{:<3} {}", entry.index, entry.carried.said()));
        lines.push(fence.opens());
        lines.push(entry.carried.shown());
        lines.push(fence.closes());
    }
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
