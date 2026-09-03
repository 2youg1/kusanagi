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

use kusanagi_grant::{Ability, Revocations};
use kusanagi_kernel::{Handle, Hex, Instant};
use kusanagi_waypoint::{Certificate, Verdict};
use serde::Serialize;

use kusanagi_site::{Channel, Standing};

use crate::prose;

use crate::walk::Walked;

/// A handle rendered short enough to read, for listings.
///
/// Shortening is a rendering decision, so it lives with the renderings and not
/// with the record: what is stored is always the whole handle.
fn abbreviate(handle: &Handle) -> String {
    handle.to_string().chars().take(12).collect()
}

/// One segment as it is reported.
#[derive(Serialize, Debug)]
pub struct Entry {
    pub(crate) index: u64,
    pub(crate) id: String,
    pub(crate) address: String,
    /// The exact bytes, in lowercase hexadecimal.
    ///
    /// This is the field a program reads. It exists because the one beside it
    /// cannot be parsed back, and a caller that cannot recover what was sent is
    /// not on a channel.
    pub(crate) payload: String,
    /// The same bytes as text, lossily.
    ///
    /// For eyes only: a payload that is not UTF-8 arrives here with replacement
    /// characters, and nothing downstream can tell that from the real thing.
    pub(crate) text: String,
}

/// What this endpoint may do on a channel at one moment.
///
/// The two cases are kept apart in the type so that the listing cannot report
/// abilities and a refusal at the same time. Flattening happens once, at the
/// edge, in [`Summary`].
enum Authority {
    /// Verified now, with what survived and when it lapses.
    Held {
        /// The abilities that passed verification.
        can: Vec<&'static str>,
        /// When they stop being accepted, absent for a root authority.
        until: Option<u64>,
    },
    /// Nothing, and the stable code that says why.
    Void(&'static str),
}

/// One channel as it is listed.
#[derive(Serialize, Debug)]
pub struct Summary {
    pub(crate) name: String,
    pub(crate) waypoint: String,
    pub(crate) standing: &'static str,
    pub(crate) peer: Option<String>,
    /// What this endpoint may do here right now, verified rather than claimed.
    ///
    /// Empty exactly when `refused` is present: a caller reads one field or the
    /// other, never both.
    pub(crate) can: Vec<&'static str>,
    /// When the authority lapses, in seconds since the Unix epoch.
    ///
    /// Absent for a root authority, which nobody issued and nothing expires.
    pub(crate) expires_at: Option<u64>,
    /// How long that is from the instant this command sampled.
    ///
    /// The same fact as `expires_at` in the frame a reader is in. Both are
    /// reported because they answer different questions: one survives being
    /// written down, the other can be acted on without a clock.
    pub(crate) expires_in: Option<u64>,
    /// The stable code that says why `can` is empty, absent when it is not.
    pub(crate) refused: Option<&'static str>,
    /// The code a read of the peer's stream would fail with, absent when it
    /// would not.
    ///
    /// This is where a revocation becomes visible to the endpoint that made it.
    /// Cutting somebody off is one-sided by construction — there is no channel
    /// on which to tell them — so their own listing goes on reporting a live
    /// grant, and the refusal lives on this side.
    pub(crate) peer_refused: Option<&'static str>,
}

/// One measured capability as it is reported.
#[derive(Serialize, Debug)]
pub struct Measured {
    pub(crate) capability: &'static str,
    pub(crate) verdict: &'static str,
    pub(crate) detail: Option<String>,
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
        /// The handle that signed every segment reported here.
        ///
        /// The peer's, or this endpoint's own when the read was `--mine`. It is
        /// not called `peer` because with that flag it would not be one.
        author: String,
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
    /// This endpoint served as a host until the listener stopped.
    Hosted {
        /// What it was listening on.
        address: String,
        /// The directory it kept drops in.
        directory: String,
    },
}

/// What `who` may do under `standing` at `now`, asked the way a verb asks it.
///
/// `send` asks whether this endpoint may write; `read` asks whether the author
/// of what it is about to read may. Putting those questions through the same
/// function is what keeps a listing from disagreeing with the command it
/// describes — and everything needed to answer them is on this machine, so a
/// listing costs no request.
fn authority(
    standing: &Standing,
    root: &Handle,
    who: &Handle,
    now: Instant,
    revoked: &Revocations,
) -> Authority {
    let until = expiry(standing, root, now, revoked);
    let held = |can: Vec<&'static str>| Authority::Held { can, until };
    match (
        standing.permits(root, who, Ability::Send, now, revoked),
        standing.permits(root, who, Ability::Read, now, revoked),
    ) {
        (Ok(()), Ok(())) => held(vec!["send", "read"]),
        (Ok(()), Err(_)) => held(vec!["send"]),
        (Err(_), Ok(())) => held(vec!["read"]),
        // Both refusals have one cause — an expired, revoked or detached chain
        // refuses every ability alike — so either error names it.
        (Err(error), Err(_)) => Authority::Void(error.code()),
    }
}

/// When a standing lapses, for the standings that lapse at all.
///
/// A root authority has no expiry because nobody issued it, and a chain that no
/// longer verifies has no expiry worth reporting — what it has is a refusal.
fn expiry(standing: &Standing, root: &Handle, now: Instant, revoked: &Revocations) -> Option<u64> {
    let scope = standing.grant()?.verify(root, now, revoked).ok()?;
    Some(scope.expires_at().as_unix_seconds())
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
        let (can, expires_at, refused) =
            match authority(&channel.standing, &channel.root, who, now, revoked) {
                Authority::Held { can, until } => (can, until, None),
                Authority::Void(code) => (Vec::new(), None, Some(code)),
            };
        // The peer is asked the one question a read of their stream asks.
        let peer_refused = channel.peer.as_ref().and_then(|peer| {
            peer.standing
                .permits(&channel.root, &peer.handle(), Ability::Send, now, revoked)
                .err()
                .map(|error| error.code())
        });
        let expires_in = expires_at.map(|at| at.saturating_sub(now.as_unix_seconds()));
        Summary {
            name: name.to_owned(),
            waypoint: channel.locator.clone(),
            standing: match channel.standing {
                Standing::Root => "root",
                Standing::Granted(_) => "granted",
            },
            peer: channel.peer.as_ref().map(|peer| abbreviate(&peer.handle())),
            can,
            expires_at,
            expires_in,
            refused,
            peer_refused,
        }
    }

    /// Reports a verified stream, from `after` upwards.
    ///
    /// The height reported is always the verified head, whatever `after` hides:
    /// one call then answers both of a caller's questions — how far the stream
    /// goes, and what of it is new.
    #[must_use]
    pub fn read(name: &str, author: &str, walked: &Walked, after: Option<u64>) -> Self {
        Self::Read {
            name: name.to_owned(),
            author: author.to_owned(),
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
        prose::render(self)
    }
}
