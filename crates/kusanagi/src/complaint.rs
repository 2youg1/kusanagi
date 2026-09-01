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
use kusanagi_kernel::{DigestParseError, HexError, SegmentError, WaypointError};
use kusanagi_seal::OpenFailed;
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
    /// Local state could not be read or written.
    #[error("could not {action}: {source}")]
    Local {
        /// What was being attempted.
        action: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A stored or supplied structure is not well formed.
    #[error("{what} is malformed: {reason}")]
    Malformed {
        /// Which structure.
        what: &'static str,
        /// What was wrong with it.
        reason: String,
    },
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

impl From<HexError> for Complaint {
    fn from(error: HexError) -> Self {
        Self::Malformed {
            what: "an invitation",
            reason: error.to_string(),
        }
    }
}

impl From<DigestParseError> for Complaint {
    fn from(error: DigestParseError) -> Self {
        Self::Malformed {
            what: "an identifier",
            reason: error.to_string(),
        }
    }
}

/// A complaint rendered for both readers.
#[derive(Serialize)]
struct Rendered<'a> {
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
            Self::Malformed { .. } => "kusanagi.malformed",
            Self::UnknownChannel { .. } => "kusanagi.unknown_channel",
            Self::ChannelExists { .. } => "kusanagi.channel_exists",
            Self::NoPeerYet { .. } => "kusanagi.no_peer_yet",
            Self::DropTaken { .. } => "kusanagi.drop_taken",
            Self::NotThePeer { .. } => "kusanagi.not_the_peer",
            Self::InviteSpent => "kusanagi.invite_spent",
            Self::OwnInvitation => "kusanagi.own_invitation",
            Self::CannotRevokeRoot { .. } => "kusanagi.cannot_revoke_root",
            Self::Argument { .. } => "kusanagi.argument",
        }
    }

    /// The command that would move the caller forward from here.
    fn recover(&self) -> String {
        match self {
            Self::Waypoint(_) => {
                "run `kusanagi doctor <waypoint>` to see what the host actually does".to_owned()
            }
            Self::Segment(_) | Self::Chain(_) | Self::Sealed(_) | Self::NotThePeer { .. } => {
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
            Self::Malformed { .. } => {
                "copy the whole invitation, including the `kusanagi1:` prefix".to_owned()
            }
            Self::InviteSpent => {
                "ask for a fresh invitation; each one admits exactly one endpoint".to_owned()
            }
            Self::OwnInvitation => "hand this line to the endpoint you mean to admit; \
                 the channel it opens is already here under the name you gave it"
                .to_owned(),
            Self::CannotRevokeRoot { .. } => {
                "leave the channel instead: delete it from `channels` and stop reading it"
                    .to_owned()
            }
            Self::UnknownChannel { .. } | Self::NoPeerYet { .. } => {
                "run `kusanagi channels` to see what is here".to_owned()
            }
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
