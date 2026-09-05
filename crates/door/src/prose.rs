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

use crate::chamber::{founded, invited, joined, room, sent};
use crate::fence::Fence;
use crate::passage::members;
use crate::passage::{enrolled, forgotten, greeted, posted, served, severed, welcomed};
use crate::report::Outcome;
use crate::rows::{Delivery, Entry, Grouping, Landed, Measured, Summary, called};

/// The channels, then every group and what it stands for.
fn grouped_listing(channels: &[Summary], groups: &[Grouping]) -> String {
    let mut said = listing(channels);
    for group in groups {
        said.push_str("\n\ngroup `");
        said.push_str(&group.name);
        said.push('`');
        said.push_str(&members(group));
    }
    said
}

/// Renders one outcome as prose.
pub fn render(outcome: &Outcome, fence: Fence) -> String {
    match outcome {
        Outcome::Identity {
            handle,
            site,
            alias,
        } => format!(
            "this endpoint is {handle}\n  name  {}\n  site  {site}",
            alias
                .as_deref()
                .unwrap_or("(none; `kusanagi name --as NAME` sets one)")
        ),
        Outcome::Channels { channels, groups } if channels.is_empty() && groups.is_empty() => {
            "no channels yet; `kusanagi invite` starts one".to_owned()
        }
        Outcome::Channels { channels, groups } => grouped_listing(channels, groups),
        Outcome::Grouped { group } => enrolled(group),
        Outcome::FannedOut { group, delivered } => fanned(group, delivered),
        Outcome::RoomFounded {
            name,
            ward,
            founder,
        } => founded(name, ward, founder),
        Outcome::RoomInvited {
            name,
            invite,
            check,
            expires_at,
        } => invited(name, invite, check, *expires_at),
        Outcome::RoomJoined {
            name,
            handle,
            founder,
            check,
        } => joined(name, handle, founder, check),
        Outcome::RoomSent {
            name,
            index,
            address,
        } => sent(name, *index, address),
        Outcome::Room { name, threads } => room(name, threads, fence),
        Outcome::Invited {
            name,
            invite,
            check,
            expires_at,
            expires_in,
        } => welcomed(name, invite, check, *expires_at, *expires_in),
        Outcome::Joined {
            name,
            handle,
            peer,
            check,
            waypoint,
            retention,
        } => greeted(name, handle, peer, check, waypoint, retention),
        Outcome::Sent {
            name,
            index,
            id,
            address,
        } => posted(name, *index, id, address),
        Outcome::Queued { .. } | Outcome::Ticked { .. } => scheduled(outcome),
        Outcome::Served { calls } => served(*calls),
        Outcome::Read {
            name,
            author,
            alias,
            height,
            segments,
        } => stream(name, author, alias.as_deref(), *height, segments, fence),
        Outcome::Revoked { name, step } => severed(name, step),
        Outcome::Sweeping { .. } | Outcome::Egress { .. } => setting(outcome),
        Outcome::Forgotten { name, waypoint } => forgotten(name, waypoint),
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
        Outcome::Here {
            site,
            under_profile,
            at_rest,
            proxy,
            binary,
        } => machine(site, *under_profile, at_rest, *proxy, binary),
        Outcome::Imported { site, channels } => {
            format!("restored {channels} channel(s) into {site}")
        }
        Outcome::Hosted { address, directory } => {
            format!("stopped hosting {directory} on {address}")
        }
    }
}

/// What this machine is doing, with each answer said rather than abbreviated.
///
/// A report somebody is meant to act on has to say which answer is the wrong
/// one. `false` under a heading is a value; "another account may be able to read
/// it" is an instruction.
fn machine(
    site: &str,
    under_profile: Option<bool>,
    at_rest: &str,
    proxy: bool,
    binary: &str,
) -> String {
    let profile = match under_profile {
        Some(true) => "yes, so no other account inherits access to it",
        Some(false) => {
            "NO \u{2014} another account may be able to read it; put --root under \
             your profile directory"
        }
        None => "not a question on this platform",
    };
    let sealed = if at_rest == "plain" {
        "whoever takes this disk reads these records"
    } else {
        "sealed to this account's logon credentials"
    };
    let through = if proxy {
        "set, and every request goes through it"
    } else {
        "not set, so this machine's address reaches the host directly"
    };
    format!(
        "this machine\n  site           {site}\n  under profile  {profile}\n  \
         at rest        {at_rest} \u{2014} {sealed}\n  proxy          {through}\n  \
         binary         {binary}"
    )
}

/// What each member of a group got, with the failures where they cannot be missed.
///
/// The count comes first because it is the one thing a person has to check. A
/// fan-out that reached four of five people looks like a success at a glance,
/// and the fifth person is the one who will not know why they were left out.
fn fanned(group: &str, delivered: &[Delivery]) -> String {
    let arrived = delivered
        .iter()
        .filter(|row| matches!(row.landed, Landed::Sent { .. }))
        .count();
    let rows: String = delivered
        .iter()
        .map(|row| match &row.landed {
            Landed::Sent { index, address } => {
                format!("\n  {:<20} #{index}  {address}", row.member)
            }
            Landed::Refused { code, error } => {
                format!("\n  {:<20} not sent \u{2014} {code}: {error}", row.member)
            }
        })
        .collect();
    format!(
        "sent to {arrived} of {} on `{group}`{rows}",
        delivered.len()
    )
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
        // How it writes and what it keeps, in one narrow column. `release` is
        // shown even though it is not the default precisely because it is not:
        // on such a channel this disk is the only copy of the conversation.
        let habit = match channel.period {
            None => channel.retention.to_owned(),
            Some(seconds) => format!("{}/{seconds}s", channel.retention),
        };
        // The waypoint goes last because it is the one column with no bound on
        // its width: a long locator then runs off the end instead of pushing
        // everything after it out of line.
        format!(
            "  {:<16} {:<8} {:<12} {:<30} {:<40} {}",
            channel.name, channel.standing, habit, authority, peer, channel.waypoint,
        )
    }));
    lines.join("\n")
}

/// The two outcomes a channel with a rhythm produces.
///
/// Apart from the match above because both need several lines to say one thing:
/// **the caller's message did not go out when they asked, and that is the
/// point.** A person reading either of these has to be told where their words
/// are and what will move them.
/// What one of the two site settings says about itself, with its consequence.
fn setting(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Sweeping { digits, wards } => format!(
            "a read names {digits} of the ward's four digits, so it is one of the readers of \
             {wards} ward(s) and downloads what all of them received"
        ),
        Outcome::Egress {
            proxy_required: true,
        } => "a proxy is required: without KUSANAGI_PROXY, every verb that would reach a host \
             refuses instead of going direct"
            .to_owned(),
        _ => "a proxy is optional: with KUSANAGI_PROXY unset, requests go straight to the host, \
              and the host learns this machine's address"
            .to_owned(),
    }
}

fn scheduled(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Queued {
            name,
            waiting,
            period,
        } => format!(
            "queued on `{name}`; {waiting} waiting
             this channel writes one drop every {} seconds whether or not there is              anything to say, so nothing goes out until `kusanagi tick --from {name}`              reaches its next slot.",
            period.unwrap_or(0)
        ),
        Outcome::Ticked {
            name,
            slot,
            period,
            wrote,
            carried,
            waiting,
            heard,
        } => format!(
            "slot {slot} on `{name}`, one every {period}s
  wrote     {}
               carried   {carried}
  waiting   {waiting}
  heard     {}",
            wrote.map_or_else(
                || "nothing; this slot was already filled".to_owned(),
                |at| format!("#{at}")
            ),
            heard.map_or_else(|| "nothing yet".to_owned(), |at| format!("up to #{at}"))
        ),
        // Unreachable by construction: the caller matched these two variants.
        // Modelled rather than asserted away, because this crate does not panic.
        _ => String::new(),
    }
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
    alias: Option<&str>,
    height: Option<u64>,
    segments: &[Entry],
    fence: Fence,
) -> String {
    // The one naming rule, here as in the listing: the full handle is in the
    // `author` field for whatever needs to match on it. The alias appears on
    // this header line only — this program's line — and never inside a fence.
    let who = called(alias, author);
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
