// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What this binary says when it cannot do what was asked.
//!
//! Every complaint carries four things: what failed, what it failed on, a stable
//! code, and **the command that would recover**. The first three come from the
//! layer that failed; the fourth can only come from here, because only this layer
//! knows what the caller was trying to do.
//!
//! The caller on the other side of this door is usually an agent. An agent that
//! has to infer the next step from prose will infer it wrongly, so the next step
//! is a field.

use kusanagi_chain::ChainError;
use kusanagi_grant::GrantError;
use kusanagi_kernel::{SegmentError, WaypointError};
use kusanagi_seal::OpenFailed;
use kusanagi_site::SiteError;
use kusanagi_waypoint::LocatorError;
use serde::Serialize;

/// A failure, in the shape a caller can act on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Complaint {
    /// The waypoint could not be read or written.
    #[error(transparent)]
    Waypoint(#[from] WaypointError),
    /// Bytes at an address are not a segment.
    #[error(transparent)]
    Segment(#[from] SegmentError),
    /// The segments found do not form a chain.
    #[error(transparent)]
    Chain(#[from] ChainError),
    /// Sealed bytes did not open under the key this address derives.
    #[error(transparent)]
    Sealed(#[from] OpenFailed),
    /// A grant does not authorise this.
    #[error(transparent)]
    Grant(#[from] GrantError),
    /// The waypoint locator does not name a place.
    #[error(transparent)]
    Locator(#[from] LocatorError),
    /// The operating system would not attach the restriction a site needs.
    ///
    /// Distinct from [`Complaint::Local`] because the write did not fail — it was
    /// refused. Going ahead would have left an identity seed or a channel secret
    /// readable by every account on the machine, and a filesystem with no access
    /// lists cannot be talked into having them.
    #[error("could not {what}: {source}")]
    Permissions {
        /// What was being attempted.
        what: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// Local state could not be read or written.
    #[error("could not {action}: {source}")]
    Local {
        /// What was being attempted.
        action: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A name the caller typed is not one a channel can have.
    #[error("`{name}` is not a channel name: {reason}")]
    BadName {
        /// What was typed.
        name: String,
        /// Why it cannot be used.
        reason: String,
    },
    /// A line offered as an invitation is not one.
    #[error("that is not an invitation: {reason}")]
    BadInvitation {
        /// What was wrong with it.
        reason: String,
    },
    /// Bytes already on this disk are not the structure they claim to be.
    #[error("{what} is malformed: {reason}")]
    BadRecord {
        /// Which structure.
        what: &'static str,
        /// What was wrong with it.
        reason: String,
    },
    /// The operating system does not say where this user's data lives.
    ///
    /// The default site is under the profile directory, which is named by one
    /// environment variable per platform. When that variable is absent there is
    /// nothing left to guess with, and guessing would put an identity somewhere
    /// nobody meant — which is the failure this default exists to prevent.
    #[error("there is no {variable} in this environment, so there is no default place for a site")]
    NoRoot {
        /// The variable that would have named the profile directory.
        variable: &'static str,
    },
    /// A channel was to be written before this endpoint had an identity.
    ///
    /// Every verb that writes one creates the identity first, so this is a
    /// caller of the library rather than of the command line — and it is a
    /// complaint rather than a panic for exactly that reason.
    #[error("this endpoint has no identity yet")]
    NoIdentity,
    /// No channel by that name has been joined.
    #[error("there is no channel called `{name}` here")]
    UnknownChannel {
        /// The name that was asked for.
        name: String,
    },
    /// A channel by that name already exists.
    #[error("a channel called `{name}` is already here")]
    ChannelExists {
        /// The name that was asked for.
        name: String,
    },
    /// The peer has not introduced itself yet.
    #[error("nobody has joined `{name}` yet, so there is nothing of theirs to read")]
    NoPeerYet {
        /// The channel that is still waiting.
        name: String,
    },
    /// Somebody else claimed the next address first.
    #[error("the next drop on `{name}` is already taken")]
    DropTaken {
        /// The address that was already occupied.
        address: String,
        /// Which channel was being extended.
        name: String,
    },
    /// The invitation has already been accepted by somebody.
    #[error("this invitation has already been used")]
    InviteSpent,
    /// The invitation was minted by this endpoint.
    ///
    /// Accepting it would give one endpoint two local names for one stream: the
    /// peer it discovered would be itself, and every read would hand back what
    /// it had just written as though somebody else had said it.
    #[error("this invitation is your own")]
    OwnInvitation,
    /// The peer of this channel is its root authority, which cannot be revoked.
    #[error(
        "the peer of `{name}` is the authority that invited you; there is nothing above it to revoke"
    )]
    CannotRevokeRoot {
        /// Which channel.
        name: String,
    },
    /// A segment on a peer's stream was not written by that peer.
    #[error("a segment on `{name}` is signed by somebody who is not the peer")]
    NotThePeer {
        /// Which channel.
        name: String,
    },
    /// The introduction on a channel is not one this build can read.
    ///
    /// A greeting announces the newcomer's key and the grant that admits them,
    /// and it is signed by the one-time key from the invitation. Reaching this
    /// means those bytes were written by something that authenticated correctly
    /// and then said something else — a build that disagrees about the format, or
    /// damage inside the envelope.
    #[error("the introduction on `{name}` cannot be read: {reason}")]
    BadGreeting {
        /// Which channel.
        name: String,
        /// What was wrong with the bytes.
        reason: String,
    },
    /// The host is serving a history that contradicts one already verified here.
    ///
    /// A host cannot forge a segment, but it can withhold one or replace one it
    /// promised not to. Neither is visible to a reader that starts from nothing,
    /// because what it is handed is a shorter chain that verifies perfectly. It
    /// is visible to a reader that wrote down where it got to, and refusing here
    /// is the whole value of having written that down.
    #[error("`{name}` no longer holds what this endpoint has already read: {what}")]
    HistoryChanged {
        /// Which channel.
        name: String,
        /// How this reading differs from the one already verified.
        what: String,
    },
    /// An argument was not something this verb can act on.
    ///
    /// This is the one variant that carries its own recovery. Every other
    /// failure's way out follows from what kind of failure it is; the way out of
    /// a bad argument is knowing what to pass instead, and only the code that
    /// named the flag knows that.
    #[error("{what} {reason}")]
    Argument {
        /// The argument, spelled the way a caller types it.
        what: &'static str,
        /// What was wrong with it.
        reason: String,
        /// What to pass instead.
        instead: &'static str,
    },
}

/// Gives a local failure the code and the way out that only the door can name.
///
/// The shapes are the same on both sides, and that is the point rather than an
/// accident: `kusanagi-site` says what was being done and what was wrong with the
/// bytes, and this is where that becomes a stable code plus a command a caller
/// can run. Merging the two types would put the words `kusanagi channels` inside
/// a crate that has no verbs.
impl From<SiteError> for Complaint {
    fn from(error: SiteError) -> Self {
        match error {
            SiteError::Local { action, source } => Self::Local { action, source },
            SiteError::Permissions { what, source } => Self::Permissions { what, source },
            SiteError::BadName { name, reason } => Self::BadName { name, reason },
            SiteError::BadInvitation { reason } => Self::BadInvitation { reason },
            SiteError::BadRecord { what, reason } => Self::BadRecord { what, reason },
            SiteError::UnknownChannel { name } => Self::UnknownChannel { name },
            SiteError::NoIdentity => Self::NoIdentity,
            SiteError::Grant(error) => Self::Grant(error),
        }
    }
}

/// A complaint rendered for both readers.
#[derive(Serialize)]
struct Rendered<'a> {
    /// The version of the shape a machine reads. Failures carry it too, because
    /// a caller that pins the contract pins it on every answer or on none.
    contract: u8,
    error: &'a str,
    code: &'a str,
    recover: String,
}

impl Complaint {
    /// The stable code of the underlying failure.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::Waypoint(error) => error.code(),
            Self::Segment(error) => error.code(),
            Self::Chain(error) => error.code(),
            Self::Sealed(error) => error.code(),
            Self::Grant(error) => error.code(),
            Self::Locator(error) => error.code(),
            Self::Local { .. } => "kusanagi.local",
            Self::Permissions { .. } => "site.permissions",
            // One published code for three shapes: what a caller does about a
            // malformed thing depends on which thing, and that is the recovery's
            // job. Splitting the code would break every script that matches it.
            Self::BadName { .. } | Self::BadInvitation { .. } | Self::BadRecord { .. } => {
                "kusanagi.malformed"
            }
            Self::NoIdentity => "kusanagi.no_identity",
            Self::NoRoot { .. } => "kusanagi.no_root",
            Self::UnknownChannel { .. } => "kusanagi.unknown_channel",
            Self::ChannelExists { .. } => "kusanagi.channel_exists",
            Self::NoPeerYet { .. } => "kusanagi.no_peer_yet",
            Self::DropTaken { .. } => "kusanagi.drop_taken",
            Self::NotThePeer { .. } => "kusanagi.not_the_peer",
            Self::BadGreeting { .. } => "kusanagi.bad_greeting",
            Self::HistoryChanged { .. } => "kusanagi.history_changed",
            Self::InviteSpent => "kusanagi.invite_spent",
            Self::OwnInvitation => "kusanagi.own_invitation",
            Self::CannotRevokeRoot { .. } => "kusanagi.cannot_revoke_root",
            Self::Argument { .. } => "kusanagi.argument",
        }
    }

    /// The command that would move the caller forward from here.
    fn recover(&self) -> String {
        match self {
            // Two of the waypoint's failures have a way out of their own, and
            // both are about the host rather than the network: one sent this
            // endpoint somewhere it did not choose, the other said nothing at
            // all. Telling somebody to run `doctor` against a host that is not
            // answering wastes the one thing they have, which is a guess.
            Self::Waypoint(WaypointError::Redirected { .. }) => {
                "this host is not a box: it answered with somewhere else to go, and that was \
                 refused rather than followed. Check the waypoint url"
                    .to_owned()
            }
            Self::Waypoint(WaypointError::Unanswered { .. }) => {
                "retry; if it persists the host is down".to_owned()
            }
            Self::Waypoint(_) => {
                "run `kusanagi doctor <waypoint>` to see what the host actually does".to_owned()
            }
            Self::Segment(_)
            | Self::Chain(_)
            | Self::Sealed(_)
            | Self::NotThePeer { .. }
            | Self::BadGreeting { .. } => {
                "the bytes at that address are not what this channel expects; \
                 keep them and open an issue — this is either damage or an attack"
                    .to_owned()
            }
            Self::Grant(_) => {
                "ask whoever invited you for a new invitation: this one no longer authorises it"
                    .to_owned()
            }
            Self::Locator(_) => {
                "a waypoint is a path, an http:// url, or s3://ENDPOINT/BUCKET[?region=R]"
                    .to_owned()
            }
            Self::Local { .. } => {
                "check that --root names a writable directory, then run the command again"
                    .to_owned()
            }
            Self::Permissions { .. } => "choose a --root on a local disk that keeps per-file \
                 permissions: NTFS on Windows, any ordinary filesystem elsewhere. FAT, exFAT \
                 and most network shares cannot keep a channel secret from other accounts"
                .to_owned(),
            Self::BadName { .. } => "pick a name of 1 to 32 characters from a-z, 0-9 and -, \
                 not starting with -, and run the command again"
                .to_owned(),
            // The advice names the pipe because there is no other way in. An
            // invitation carries the channel secret, so it is not an argument,
            // and telling somebody to "copy the invitation" without saying where
            // to put it sends them looking for a flag that does not exist.
            Self::BadInvitation { .. } => "pipe the whole invitation in, including the \
                 `kusanagi1:` prefix: pbpaste | kusanagi join --name NAME"
                .to_owned(),
            Self::BadRecord { .. } => "this file is not one this build can read; keep it and \
                 report it, because a record written here should not fail to parse"
                .to_owned(),
            Self::InviteSpent => {
                "ask for a fresh invitation; each one admits exactly one endpoint".to_owned()
            }
            Self::OwnInvitation => "hand this line to the endpoint you mean to admit; \
                 the channel it opens is already here under the name you gave it"
                .to_owned(),
            Self::CannotRevokeRoot { name } => {
                format!(
                    "leave instead: `kusanagi forget --channel {name}` drops the channel here, \
                     and nothing on the host is touched"
                )
            }
            Self::UnknownChannel { .. } | Self::NoPeerYet { .. } => {
                "run `kusanagi channels` to see what is here".to_owned()
            }
            Self::NoIdentity => {
                "run `kusanagi id` to create this endpoint's identity, then try again".to_owned()
            }
            Self::NoRoot { .. } => {
                "pass --root to say where this endpoint should keep its identity and channels"
                    .to_owned()
            }
            Self::HistoryChanged { .. } => "run `kusanagi doctor <waypoint>`: only a write-once \
                 host can promise this cannot happen, and this one just did it"
                .to_owned(),
            Self::ChannelExists { name } => {
                format!(
                    "pick another name, or read the one you have with `kusanagi read --from {name}`"
                )
            }
            Self::DropTaken { name, .. } => {
                format!(
                    "run `kusanagi read --from {name}` to pick up the new head, then send again"
                )
            }
            Self::Argument { instead, .. } => (*instead).to_owned(),
        }
    }

    /// Renders this complaint for a person or for a machine.
    #[must_use]
    pub fn render(&self, json: bool) -> String {
        let rendered = Rendered {
            contract: crate::CONTRACT,
            error: &self.to_string(),
            code: self.code(),
            recover: self.recover(),
        };
        if json {
            return serde_json::to_string_pretty(&rendered)
                .unwrap_or_else(|_| format!(r#"{{"code":"{}"}}"#, rendered.code));
        }
        format!(
            "error: {}\n  code: {}\n  try:  {}",
            rendered.error, rendered.code, rendered.recover
        )
    }
}
