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

/// Which stream a read reports.
///
/// An enum rather than a flag because the two readings answer different
/// questions — *what was I told* and *what did I say* — and a `bool` named
/// `mine` at a call site three functions away says neither.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Whose {
    /// The peer's stream: what the other end wrote.
    Peer,
    /// This endpoint's own stream: what it wrote itself.
    Mine,
}

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
        /// What the segment carries, as bytes.
        ///
        /// Bytes rather than text because a caller sends what it has: quotes,
        /// newlines, and sequences that are not UTF-8 at all. What reaches the
        /// peer is what was handed over here.
        payload: Vec<u8>,
    },
    /// Read one stream on a channel, verifying it end to end.
    Read {
        /// Which channel.
        name: String,
        /// Report only the segments above this height.
        ///
        /// `None` reports the whole stream. This narrows the *report* and
        /// nothing else: the chain is still verified from genesis, because a
        /// reader that trusted a prefix it did not check would be trusting the
        /// host.
        after: Option<u64>,
        /// Whose stream to report.
        whose: Whose,
    },
    /// Cut the peer of a channel off, immediately and permanently.
    Revoke {
        /// Which channel.
        name: String,
    },
    /// Delete one channel from this endpoint, keeping nothing.
    ///
    /// Local and one-sided: the peer is not told, the host keeps every byte it
    /// already holds, and the revocation list is untouched.
    Forget {
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
        /// The most this host will hold, in bytes.
        capacity: u64,
    },
    /// Seal everything this endpoint holds into one archive.
    Export,
    /// Restore an archive into a root that has nothing in it.
    Import {
        /// The recovery key the archive was sealed under.
        recovery: [u8; 32],
        /// The archive itself.
        archive: Vec<u8>,
    },
}
