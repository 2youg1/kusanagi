// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a command reports, in one structure rendered two ways.
//!
//! Prose and JSON come from the same value, so the two can never disagree about
//! what happened. That is not a convenience: the caller on the other side of this
//! door is usually an agent, and a program whose human output and machine output
//! drift apart is a program that lies to one of its two readers.

use kusanagi_grant::Revocations;
use kusanagi_kernel::{Handle, Instant};
use kusanagi_site::Channel;
use kusanagi_waypoint::{Certificate, Verdict};
use serde::Serialize;

use crate::fence::Fence;
use crate::prose;
use crate::rows::{Delivery, Entry, Grouping, Measured, Summary, Thread};

/// The version of the shape a machine reads.
///
/// Every top-level JSON object carries it, success and failure alike. A caller
/// that pins it fails loudly on a build that changed the shape, instead of
/// quietly reading a field that no longer means what it did. Adding a field does
/// not move it; removing or renaming one does.
pub const CONTRACT: u8 = 1;

/// One outcome as a machine reads it: the contract version, then the outcome.
#[derive(Serialize)]
struct Answer<'a> {
    contract: u8,
    #[serde(flatten)]
    outcome: &'a Outcome,
}

/// What a command produced.
#[derive(Serialize, Debug)]
#[serde(tag = "command", rename_all = "snake_case")]
#[non_exhaustive]
pub enum Outcome {
    /// This endpoint's identity.
    Identity {
        /// The handle, in full.
        handle: String,
        /// Where the site lives.
        site: String,
        /// What this endpoint asks to be called, if it has said. Set with
        /// `kusanagi name --as`; it travels, signed, in every invitation and
        /// every greeting made afterwards.
        alias: Option<String>,
    },
    /// Every channel here, and every group of them.
    Channels {
        /// One row per channel.
        channels: Vec<Summary>,
        /// One row per group. A group is a list of the channels above it.
        groups: Vec<Grouping>,
    },
    /// A group's roster was replaced by this one.
    Grouped {
        /// The group as it now stands.
        group: Grouping,
    },
    /// One segment was appended for every member of a group.
    FannedOut {
        /// Which group.
        group: String,
        /// One row per member, in roster order. **Read every row**: a member
        /// that failed is a member who has not heard this.
        delivered: Vec<Delivery>,
    },
    /// An invitation was minted.
    Invited {
        /// What the channel is called here.
        name: String,
        /// The line to hand over. **This is a bearer credential.**
        invite: String,
        /// Four hexadecimal digits both ends compute, to read out in person.
        check: String,
        /// When it stops being accepted, in seconds since the Unix epoch.
        expires_at: u64,
        /// How many seconds that is from now.
        expires_in: u64,
    },
    /// An invitation was accepted.
    Joined {
        /// What the channel is called here.
        name: String,
        /// This endpoint's own handle.
        handle: String,
        /// The handle that issued the invitation.
        peer: String,
        /// Four hexadecimal digits both ends compute, to read out in person.
        check: String,
        /// Where the drops live.
        waypoint: String,
        /// `keep` or `release`: the inviter's choice, now this end's too.
        retention: &'static str,
    },
    /// A segment was appended.
    Sent {
        /// Which channel.
        name: String,
        /// Its height.
        index: u64,
        /// Its content address.
        id: String,
        /// Where it was left.
        address: String,
    },
    /// A segment was queued for a slot rather than written now.
    ///
    /// Apart from [`Outcome::Sent`] because the promise is different: a sent
    /// segment is on a host, and a queued one is on this disk until the slot
    /// comes round. A caller that treated them alike would report delivery that
    /// has not happened yet.
    Queued {
        /// Which channel.
        name: String,
        /// How many payloads are now waiting, this one included.
        waiting: usize,
        /// How many seconds one slot lasts.
        period: Option<u32>,
    },
    /// One slot was filled, or found already filled.
    Ticked {
        /// Which channel.
        name: String,
        /// Which slot the clock is in for this endpoint on this channel.
        slot: u64,
        /// How many seconds one slot lasts.
        period: u32,
        /// The height written, absent when the slot was already filled.
        wrote: Option<u64>,
        /// What the drop carried: `message`, `filler`, or `nothing`.
        ///
        /// **A host cannot tell these apart** — that is the point of a slot —
        /// so this field exists only on this side of the door.
        carried: &'static str,
        /// How many payloads are still waiting.
        waiting: usize,
        /// The peer's verified height after the look this slot includes.
        heard: Option<u64>,
    },
    /// A stream was read and verified.
    Read {
        /// Which channel.
        name: String,
        /// The handle that signed every segment reported here.
        ///
        /// The peer's, or this endpoint's own when the read was `--mine`. It is
        /// not called `peer` because with that flag it would not be one.
        author: String,
        /// The name that author signed for themselves, verified when it
        /// arrived. **Never inside a segment**: it is this program's word
        /// about the author, and the fence holds only the author's own.
        alias: Option<String>,
        /// The verified height, absent when nothing has been written.
        height: Option<u64>,
        /// Every segment, in order.
        segments: Vec<Entry>,
    },
    /// A room was founded.
    RoomFounded {
        /// What the room is called here.
        name: String,
        /// Which bin of the host every member sweeps.
        ward: String,
        /// The founder's handle, the roster's only member.
        founder: String,
    },
    /// An invitation into a room was minted.
    RoomInvited {
        /// Which room.
        name: String,
        /// The line to hand over. **This is a bearer credential.**
        invite: String,
        /// Four hexadecimal digits both ends compute, to read out in person.
        check: String,
        /// When it stops being accepted, in seconds since the Unix epoch.
        expires_at: u64,
    },
    /// A room invitation was accepted.
    RoomJoined {
        /// What the room is called here.
        name: String,
        /// This endpoint's own handle.
        handle: String,
        /// The handle that founded the room.
        founder: String,
        /// Four hexadecimal digits both ends compute, to read out in person.
        check: String,
    },
    /// A segment was appended to this endpoint's stream in a room.
    RoomSent {
        /// Which room.
        name: String,
        /// Its height on this endpoint's stream.
        index: u64,
        /// Where it was left.
        address: String,
    },
    /// A room was read and verified, one author's stream per row.
    Room {
        /// Which room.
        name: String,
        /// One row per author, in roster order. **Read every row**: an author
        /// that failed is an author who has not been heard.
        threads: Vec<Thread>,
    },
    /// A peer was cut off.
    Revoked {
        /// Which channel.
        name: String,
        /// The delegation step that no longer counts.
        step: String,
    },
    /// How this endpoint may reach a host, after `proxy` read or changed it.
    Egress {
        /// Whether every host-reaching verb refuses without `KUSANAGI_PROXY`.
        proxy_required: bool,
    },
    /// How wide this endpoint sweeps, after `sweep` read or changed it.
    Sweeping {
        /// How many of the ward's four hex digits a read names.
        digits: u8,
        /// How many wards a read is therefore indistinguishable among.
        wards: u32,
    },
    /// A channel was deleted from this endpoint.
    Forgotten {
        /// What it was called here.
        name: String,
        /// Where its drops remain, untouched.
        waypoint: String,
    },
    /// A host was measured.
    Examined {
        /// What was measured.
        waypoint: String,
        /// What kind of place it is.
        kind: &'static str,
        /// The tier it qualifies for.
        tier: &'static str,
        /// One row per capability.
        capabilities: Vec<Measured>,
    },
    /// What this machine is doing with what this endpoint holds.
    ///
    /// Every line of the documentation that says "you should" has a field here,
    /// so that checking is running a command rather than reading and believing.
    /// **Nothing in it is a secret**: a path, three yes-or-no answers, and a
    /// hash of a file anybody who has the binary can hash themselves.
    Here {
        /// Where this endpoint keeps its identity and channels.
        site: String,
        /// Whether the site sits under this user's profile directory, where the
        /// inherited access control list already excludes other accounts.
        /// Absent where the question has no meaning.
        under_profile: Option<bool>,
        /// Which store seals every record: `dpapi` or `plain`.
        at_rest: &'static str,
        /// Whether an outgoing proxy is configured. **The value is not shown**,
        /// because a proxy address is a fact about how somebody is trying to
        /// stay unobserved.
        proxy: bool,
        /// The BLAKE3 of the running binary, as the binary itself computes it.
        ///
        /// The one number in a verification procedure that needs no second
        /// tool: whoever hands over a build hands over this, and whoever
        /// receives it runs `doctor --here` and compares.
        binary: String,
    },
    /// Everything this endpoint holds, sealed.
    Exported {
        /// The key that opens it, in hexadecimal. **Shown once, here.**
        recovery: String,
        /// The archive itself, which goes to stdout rather than into JSON.
        #[serde(skip)]
        archive: Vec<u8>,
    },
    /// An archive was put back.
    Imported {
        /// Where it landed.
        site: String,
        /// How many channels came back with it.
        channels: usize,
    },
    /// This endpoint answered an agent over MCP until the pipe closed.
    Served {
        /// How many calls it answered.
        calls: u64,
    },
    /// This endpoint served as a host until the listener stopped.
    Hosted {
        /// What it was listening on.
        address: String,
        /// The directory it kept drops in.
        directory: String,
    },
}

impl Outcome {
    /// Reports one channel listing, with its authority checked at `now`.
    #[must_use]
    pub fn summarise(
        name: &str,
        channel: &Channel,
        who: &Handle,
        now: Instant,
        revoked: &Revocations,
    ) -> Summary {
        Summary::of(name, channel, who, now, revoked)
    }

    /// Reports a verified stream: its head, and the segments to show.
    ///
    /// `height` is always the verified head, whatever the caller filtered out of
    /// `segments`: one call then answers both of a caller's questions — how far
    /// the stream goes, and what of it is new.
    ///
    /// The segments arrive as `(index, acknowledged, payload)` rather than as
    /// the walk they came from, because a walk is a thing this crate must not
    /// be able to perform. Which of them to show is the verb's decision and
    /// stays with the verb; how to render them is this crate's and stays here.
    #[must_use]
    pub fn read<'a>(
        name: &str,
        author: &str,
        alias: Option<&str>,
        height: Option<u64>,
        segments: impl IntoIterator<Item = (u64, u64, &'a [u8])>,
    ) -> Self {
        Self::Read {
            name: name.to_owned(),
            author: author.to_owned(),
            alias: alias.map(str::to_owned),
            height,
            segments: segments
                .into_iter()
                .map(|(index, acknowledged, payload)| Entry::of(index, acknowledged, payload))
                .collect(),
        }
    }

    /// Reports what a host was measured to do.
    #[must_use]
    pub fn examined(waypoint: &str, kind: &'static str, certificate: &Certificate) -> Self {
        Self::Examined {
            waypoint: waypoint.to_owned(),
            kind,
            tier: certificate.tier().name(),
            capabilities: certificate
                .findings()
                .iter()
                .map(|finding| Measured {
                    capability: finding.capability.name(),
                    verdict: finding.verdict.word(),
                    detail: match &finding.verdict {
                        Verdict::Held => None,
                        Verdict::NotOffered { because } => Some(because.clone()),
                        Verdict::Broken { detail } => Some(detail.clone()),
                    },
                })
                .collect(),
        }
    }

    /// Renders this outcome for a person or for a machine.
    ///
    /// `fence` is the tag the prose puts around anything a peer wrote. It is a
    /// parameter because randomness has one source in this program and it is not
    /// here; JSON ignores it, because a parser draws its own boundaries.
    #[must_use]
    pub fn render(&self, json: bool, fence: Fence) -> String {
        if json {
            let answer = Answer {
                contract: CONTRACT,
                outcome: self,
            };
            return serde_json::to_string_pretty(&answer)
                .unwrap_or_else(|error| format!(r#"{{"error":"{error}"}}"#));
        }
        prose::render(self, fence)
    }
}
