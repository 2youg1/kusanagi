// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Everything this program can be asked to do.
//!
//! This enum, and not the command-line parser, is the authority on the verb set.
//! `main.rs` translates arguments into one of these and does nothing else, which
//! is what lets a test drive the whole program without a shell, and what would
//! let a second front end — a socket, an MCP server — arrive without teaching the
//! verbs to a second parser.

use std::path::PathBuf;

use kusanagi_grant::Abilities;

/// One thing to do.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum Request {
    /// Report this endpoint's handle, creating an identity if there is none.
    Identity,
    /// List the channels this endpoint has.
    Channels,
    /// Mint an invitation to a new channel.
    Invite {
        /// What to call the channel here.
        name: String,
        /// Where the drops will live.
        waypoint: String,
        /// How long the invitation and the grant it carries remain valid.
        lifetime: u64,
        /// What the invitee will be permitted to do.
        abilities: Abilities,
    },
    /// Accept an invitation.
    Join {
        /// The invitation, as one line.
        invite: String,
        /// What to call the channel here.
        name: String,
    },
    /// Append one segment to this endpoint's stream on a channel.
    Send {
        /// Which channel.
        name: String,
        /// What the segment carries.
        text: String,
    },
    /// Read the peer's stream on a channel, verifying it end to end.
    Read {
        /// Which channel.
        name: String,
    },
    /// Cut the peer of a channel off, immediately and permanently.
    Revoke {
        /// Which channel.
        name: String,
    },
    /// Measure what a host actually does, and issue a certificate.
    Doctor {
        /// The waypoint to measure.
        waypoint: String,
    },
    /// Be a host for other people's drops.
    Host {
        /// The address to listen on.
        bind: String,
        /// The directory to keep drops in.
        directory: PathBuf,
    },
}
