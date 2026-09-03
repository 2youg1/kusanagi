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
    /// This machine would not hand over the address a host was told to take.
    ///
    /// Three causes, one action. The port is already held by another program,
    /// the address names an interface this machine does not have, or the
    /// operating system reserves it. A caller can do exactly one thing about
    /// any of them — name a different address — so they share one code, and
    /// which of the three it was stays in `source` where a person reads it.
    /// **A distinction that does not change what the caller does next does not
    /// earn a second code.**
    #[error("could not listen on {address}: {source}")]
    Listening {
        /// The address, as it was resolved from what the caller typed.
        address: String,
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
    /// No group by that name has been made here.
    ///
    /// Apart from [`Complaint::UnknownChannel`] because the two are recovered
    /// from differently: a missing channel is somebody to be invited, and a
    /// missing group is a roster to be written.
    #[error("there is no group called `{name}` here")]
    UnknownGroup {
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
    /// An archive did not open under the recovery key that was offered.
    #[error("this archive did not open under that recovery key")]
    BadRecovery,
    /// A record on this disk was sealed by a platform store this one has not.
    #[error("this record was sealed by a store this platform does not have (tag {tag:#04x})")]
    ForeignRecord {
        /// The tag the record carries.
        tag: u8,
    },
    /// The invitation has already been accepted by somebody.
    #[error("this invitation has already been used")]
    InviteSpent,
    /// The invitation points at a drop that is not there.
    ///
    /// The line carries the key to a drop the inviter left on the host, and it
    /// is gone: expired with the invitation's own lifetime, swept, or never
    /// written because the inviter's own `invite` did not finish.
    #[error("the invitation points at something the host does not have")]
    NoInvitation,
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
            SiteError::BadRecovery => Self::BadRecovery,
            SiteError::ForeignRecord { tag } => Self::ForeignRecord { tag },
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
            Self::Listening { .. } => "kusanagi.address_unavailable",
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
            Self::UnknownGroup { .. } => "kusanagi.unknown_group",
            Self::ChannelExists { .. } => "kusanagi.channel_exists",
            Self::NoPeerYet { .. } => "kusanagi.no_peer_yet",
            Self::DropTaken { .. } => "kusanagi.drop_taken",
            Self::NotThePeer { .. } => "kusanagi.not_the_peer",
            Self::BadGreeting { .. } => "kusanagi.bad_greeting",
            Self::HistoryChanged { .. } => "kusanagi.history_changed",
            Self::InviteSpent => "kusanagi.invite_spent",
            Self::NoInvitation => "kusanagi.no_invitation",
            Self::BadRecovery => "kusanagi.bad_recovery_key",
            Self::ForeignRecord { .. } => "site.foreign_record",
            Self::OwnInvitation => "kusanagi.own_invitation",
            Self::CannotRevokeRoot { .. } => "kusanagi.cannot_revoke_root",
            Self::Argument { .. } => "kusanagi.argument",
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
