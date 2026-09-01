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

use kusanagi_kernel::Hex;
use kusanagi_waypoint::{Certificate, Verdict};
use serde::Serialize;

use crate::channel::{Channel, Standing};
use crate::site::abbreviate;
use crate::walk::Walked;

/// One segment as it is reported.
#[derive(Serialize, Debug)]
pub struct Entry {
    index: u64,
    id: String,
    address: String,
    /// The exact bytes, in lowercase hexadecimal.
    ///
    /// This is the field a program reads. It exists because the one beside it
    /// cannot be parsed back, and a caller that cannot recover what was sent is
    /// not on a channel.
    payload: String,
    /// The same bytes as text, lossily.
    ///
    /// For eyes only: a payload that is not UTF-8 arrives here with replacement
    /// characters, and nothing downstream can tell that from the real thing.
    text: String,
}

/// One channel as it is listed.
#[derive(Serialize, Debug)]
pub struct Summary {
    name: String,
    waypoint: String,
    standing: &'static str,
    peer: Option<String>,
}

/// One measured capability as it is reported.
#[derive(Serialize, Debug)]
pub struct Measured {
    capability: &'static str,
    verdict: &'static str,
    detail: Option<String>,
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
    },
    /// Every channel here.
    Channels {
        /// One row per channel.
        channels: Vec<Summary>,
    },
    /// An invitation was minted.
    Invited {
        /// What the channel is called here.
        name: String,
        /// The line to hand over. **This is a bearer credential.**
        invite: String,
        /// When it stops being accepted, in seconds since the Unix epoch.
        expires_at: u64,
    },
    /// An invitation was accepted.
    Joined {
        /// What the channel is called here.
        name: String,
        /// This endpoint's own handle.
        handle: String,
        /// The handle that issued the invitation.
        peer: String,
        /// Where the drops live.
        waypoint: String,
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
    /// A stream was read and verified.
    Read {
        /// Which channel.
        name: String,
        /// Whose stream was read.
        peer: String,
        /// The verified height, absent when nothing has been written.
        height: Option<u64>,
        /// Every segment, in order.
        segments: Vec<Entry>,
    },
    /// A peer was cut off.
    Revoked {
        /// Which channel.
        name: String,
        /// The delegation step that no longer counts.
        step: String,
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
    /// This endpoint served as a host until the listener stopped.
    Hosted {
        /// What it was listening on.
        address: String,
        /// The directory it kept drops in.
        directory: String,
    },
}

impl Outcome {
    /// Reports one channel listing.
    #[must_use]
    pub fn summarise(name: &str, channel: &Channel) -> Summary {
        Summary {
            name: name.to_owned(),
            waypoint: channel.locator.clone(),
            standing: match channel.standing {
                Standing::Root => "root",
                Standing::Granted(_) => "granted",
            },
            peer: channel.peer.as_ref().map(|peer| abbreviate(&peer.handle)),
        }
    }

    /// Reports a verified stream, from `after` upwards.
    ///
    /// The height reported is always the verified head, whatever `after` hides:
    /// one call then answers both of a caller's questions — how far the stream
    /// goes, and what of it is new.
    #[must_use]
    pub fn read(name: &str, peer: &str, walked: &Walked, after: Option<u64>) -> Self {
        Self::Read {
            name: name.to_owned(),
            peer: peer.to_owned(),
            height: walked.head().map(|head| head.index()),
            segments: walked
                .held()
                .iter()
                .filter(|held| after.is_none_or(|floor| held.segment.index() > floor))
                .map(|held| Entry {
                    index: held.segment.index(),
                    id: held.segment.id().to_string(),
                    address: held.address.to_string(),
                    payload: Hex(held.segment.payload()).to_string(),
                    text: String::from_utf8_lossy(held.segment.payload()).into_owned(),
                })
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
    #[must_use]
    pub fn render(&self, json: bool) -> String {
        if json {
            return serde_json::to_string_pretty(self)
                .unwrap_or_else(|error| format!(r#"{{"error":"{error}"}}"#));
        }
        self.prose()
    }

    fn prose(&self) -> String {
        match self {
            Self::Identity { handle, site } => {
                format!("this endpoint is {handle}\n  site  {site}")
            }
            Self::Channels { channels } if channels.is_empty() => {
                "no channels yet; `kusanagi invite` starts one".to_owned()
            }
            Self::Channels { channels } => {
                let mut lines = vec![format!("{} channel(s)", channels.len())];
                lines.extend(channels.iter().map(|channel| {
                    format!(
                        "  {:<16} {:<8} {:<40} {}",
                        channel.name,
                        channel.standing,
                        channel.waypoint,
                        channel.peer.as_deref().unwrap_or("(nobody has joined yet)")
                    )
                }));
                lines.join("\n")
            }
            Self::Invited {
                name,
                invite,
                expires_at,
            } => format!(
                "channel `{name}` is open, and expires at {expires_at}\n\n{invite}\n\n\
                 hand that line over once. Anybody who holds it can join, so treat it \
                 the way you would treat a key."
            ),
            Self::Joined {
                name,
                handle,
                peer,
                waypoint,
            } => format!(
                "joined `{name}`\n  you       {handle}\n  peer      {peer}\n  waypoint  {waypoint}"
            ),
            Self::Sent {
                name,
                index,
                id,
                address,
            } => format!("sent on `{name}` #{index}\n  id      {id}\n  address {address}"),
            Self::Read {
                name,
                peer,
                height,
                segments,
            } => {
                let header = match height {
                    None => format!("`{name}`: {peer} has written nothing yet"),
                    Some(height) => format!(
                        "`{name}`: {peer} verifies to height {height} ({} segment(s))",
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
            Self::Revoked { name, step } => format!(
                "the peer of `{name}` is cut off\n  step  {step}\n\
                 nothing they write from now on will be accepted here."
            ),
            Self::Examined {
                waypoint,
                kind,
                tier,
                capabilities,
            } => {
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
            Self::Hosted { address, directory } => {
                format!("stopped hosting {directory} on {address}")
            }
        }
    }
}
