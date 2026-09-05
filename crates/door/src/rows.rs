// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The rows an answer is made of, and the one question they all ask.
//!
//! `report.rs` owns what a verb answers; this owns the shapes inside that
//! answer. They are apart because they change for different reasons: a new verb
//! adds an `Outcome`, while a new column adds a field here.
//!
//! The question every row asks is the same one the verb it describes asks —
//! *may this handle do this, right now?* — and it is asked through
//! [`authority`] so that a listing cannot disagree with the command it
//! describes. Everything needed to answer it is on this machine, so a listing
//! costs no request.

use kusanagi_grant::{Ability, Revocations};
use kusanagi_kernel::{Handle, Hex, Instant};
use kusanagi_site::{Channel, Retention, Standing};
use serde::Serialize;

/// What to call somebody: the name they signed for themselves, or twelve
/// characters of their handle when they signed none.
///
/// **The one rule for naming a peer** (D-10): a listing, a stream header and a
/// merged thread all ask this function, so they cannot disagree. The alias
/// arrives already verified against the key beside it and already held to one
/// printable line, so nothing here re-checks it; the full handle stays in its
/// own field for whatever needs to match on it.
#[must_use]
pub fn called(alias: Option<&str>, handle: &str) -> String {
    alias.map_or_else(|| handle.chars().take(12).collect(), str::to_owned)
}

/// One group as it is reported: what it is called, and who is in it.
#[derive(Serialize, Debug)]
pub struct Grouping {
    /// What this endpoint calls the group.
    pub name: String,
    /// The channels a message to it fans out to.
    pub members: Vec<String>,
}

/// What one member of a group got.
///
/// **A fan-out has no single result.** Five members are five channels, five
/// hosts and five chances to fail, and one unreachable host must not decide
/// whether the other four heard anything. So the answer is a row per member,
/// and partial delivery is reported as what it is rather than collapsed into a
/// failure or hidden behind a success.
#[derive(Serialize, Debug)]
pub struct Delivery {
    /// The channel it went on.
    pub member: String,
    /// What happened there.
    #[serde(flatten)]
    pub landed: Landed,
}

/// How one member's copy ended up.
#[derive(Serialize, Debug)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Landed {
    /// It was appended to that member's stream.
    Sent {
        /// Its height on that stream.
        index: u64,
        /// Where it was left.
        address: String,
    },
    /// It was not, and this is the same code the verb alone would have given.
    Refused {
        /// The stable code of the failure.
        code: &'static str,
        /// What went wrong, in the words the single-channel verb would use.
        error: String,
    },
}

/// One segment as it is reported.
///
/// Two fields, and one of them used to be four. What went: `id` and `address`
/// are derived values a caller can recompute and almost never wants, and the
/// pair of payload renderings said the same sentence twice — once unreadably.
#[derive(Serialize, Debug)]
pub struct Entry {
    pub(crate) index: u64,
    /// How many of the reader's own segments the author had verified when
    /// they wrote this one. Two streams carry no clock, and this is the one
    /// fact that orders them: a segment stands after everything it counts.
    pub(crate) acknowledged: u64,
    #[serde(flatten)]
    pub(crate) carried: Carried,
}

/// What a segment carried, in the one encoding that does not lose it.
///
/// **An enum because the two are exclusive, and were not before.** A payload
/// that is valid UTF-8 survives a JSON string byte for byte, so hexadecimal
/// beside it doubled the size of every ordinary message to say the same thing;
/// a payload that is not text cannot go in a string at all, so hexadecimal is
/// the only honest rendering of it. Which one appears therefore says something
/// true about the bytes, and a reader that handles both handles everything.
#[derive(Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Carried {
    /// Every byte of it is text, and this is exactly those bytes.
    Text(String),
    /// It is not text. The exact bytes, in lowercase hexadecimal.
    Payload(String),
}

impl Carried {
    /// Renders `bytes` in whichever form keeps all of them.
    ///
    /// Text is narrower than valid UTF-8. A terminal is an interpreter and a
    /// language model reads a tool result as one stream, so bytes a terminal
    /// would *execute* — an escape sequence that writes the clipboard, a bare
    /// carriage return that overwrites the line this program just printed, a C1
    /// control — and the bidirectional overrides that reorder what a reader
    /// sees are not text, whatever encoding they arrive in. They are shown as
    /// hexadecimal, where nothing can act on them.
    pub(super) fn of(bytes: &[u8]) -> Self {
        match core::str::from_utf8(bytes) {
            Ok(text) if is_inert(text) => Self::Text(text.to_owned()),
            _ => Self::Payload(Hex(bytes).to_string()),
        }
    }

    /// The peer's own bytes, and nothing of this program's.
    ///
    /// Whatever this returns goes inside the fence, so it must not contain a
    /// word kusanagi is responsible for. Bytes that are not text appear as
    /// hexadecimal rather than as a sentence about them — a sentence would be
    /// this program speaking from inside the peer's half of the answer.
    pub(crate) fn shown(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Payload(hex) => hex.clone(),
        }
    }

    /// What this program says about the bytes, which goes outside the fence.
    pub(crate) fn said(&self) -> String {
        match self {
            Self::Text(text) => format!("text, {} bytes", text.len()),
            Self::Payload(hex) => format!("not text, {} bytes as hex", hex.len() / 2),
        }
    }
}

/// Whether every character can be printed without a terminal or a reader
/// doing anything but showing it: no control character but tab, newline and
/// the carriage return of a `\r\n` pair, and no bidirectional override.
fn is_inert(text: &str) -> bool {
    let mut characters = text.chars().peekable();
    while let Some(character) = characters.next() {
        let allowed = match character {
            '\t' | '\n' => true,
            '\r' => characters.peek() == Some(&'\n'),
            '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}' => false,
            other => !other.is_control(),
        };
        if !allowed {
            return false;
        }
    }
    true
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
    /// The peer, as [`called`] names them: their signed alias, or an
    /// abbreviated handle. Absent until somebody joins.
    pub(crate) peer: Option<String>,
    /// The alias the peer signed for themselves, on its own so a caller can
    /// tell a name from an abbreviation. Absent when they declared none.
    pub(crate) alias: Option<String>,
    /// How many seconds one slot lasts, absent on a channel that writes on
    /// demand.
    pub(crate) period: Option<u32>,
    /// What becomes of a drop the peer has read: `keep` or `release`.
    ///
    /// **Reported because the combination that must not exist is release
    /// without a backup**, and a listing is where somebody notices. On a
    /// releasing channel this site is the only copy of the conversation.
    pub(crate) retention: &'static str,
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

impl Summary {
    /// Reports one channel listing, with its authority checked at `now`.
    pub(super) fn of(
        name: &str,
        channel: &Channel,
        who: &Handle,
        now: Instant,
        revoked: &Revocations,
    ) -> Self {
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
        Self {
            name: name.to_owned(),
            waypoint: channel.locator.clone(),
            standing: match channel.standing {
                Standing::Root => "root",
                Standing::Granted(_) => "granted",
            },
            period: channel.cadence.period(),
            retention: match channel.retention {
                Retention::Keep => "keep",
                Retention::ReleaseOnAck => "release",
            },
            peer: channel.peer.as_ref().map(|peer| {
                called(
                    peer.alias.as_ref().map(kusanagi_kernel::Alias::as_str),
                    &peer.handle().to_string(),
                )
            }),
            alias: channel
                .peer
                .as_ref()
                .and_then(|peer| peer.alias.as_ref())
                .map(|alias| alias.as_str().to_owned()),
            can,
            expires_at,
            expires_in,
            refused,
            peer_refused,
        }
    }
}
