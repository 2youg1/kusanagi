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
use kusanagi_site::{Cadence, Retention};

/// The two habits a channel is opened with, settled once at both ends.
///
/// One value rather than two parameters because `invite` and `join` each carry
/// both and neither means anything alone: a channel that releases its history
/// without a rhythm still leaks when it speaks, and a rhythm without release
/// still leaves every word on the host. Grouping them is also what keeps the two
/// verbs' signatures from growing a parameter every time a policy is added.
///
/// **Neither end tells the other.** Both are local decisions about what *this*
/// endpoint does, so two ends may disagree — one may fill slots while the other
/// answers on demand, and the protocol is unchanged. What an observer learns
/// about an endpoint is decided by that endpoint alone, which is the only shape
/// in which the choice is worth anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Habit {
    /// How often this endpoint writes here.
    pub cadence: Cadence,
    /// What becomes of a drop once the peer has read it.
    pub retention: Retention,
}

impl Default for Habit {
    /// Write when asked, and keep everything.
    ///
    /// The default is the one that costs nothing and promises nothing: no
    /// scheduler, no backup duty, and law 1 unqualified.
    fn default() -> Self {
        Self {
            cadence: Cadence::OnDemand,
            retention: Retention::Keep,
        }
    }
}

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

/// What to do about this endpoint's name.
///
/// Three cases rather than an `Option<Option<String>>`: asking, setting and
/// clearing are three different things a caller means, and a nested option
/// would make two of them look alike at every call site.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Naming {
    /// Report the name as it stands.
    Ask,
    /// Record this name. It is checked before it is written.
    Set(String),
    /// Record that this endpoint has no name.
    Clear,
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
        /// How this endpoint will write here, and what it will keep.
        habit: Habit,
    },
    /// Accept an invitation.
    Join {
        /// The invitation, as one line.
        invite: String,
        /// What to call the channel here.
        name: String,
        /// How this endpoint will write here, and what it will keep.
        habit: Habit,
    },
    /// Fill this channel's current slot, writing a filler if there is nothing
    /// queued, and look once.
    ///
    /// The verb a scheduler runs. It is one-shot like every other, so the
    /// schedule lives in `schtasks`, `cron` or `launchd` and never in a process
    /// of this program's.
    Tick {
        /// Which channel.
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
    /// Read, or change, whether this endpoint may reach a host without a proxy.
    Proxy {
        /// `None` reads; `Some` records.
        require: Option<bool>,
    },
    /// Read, change or clear what this endpoint asks to be called.
    ///
    /// The name travels, signed by this endpoint's key, in every invitation
    /// and greeting made after it is set; peers met before see no change.
    Name {
        /// What to do about it.
        naming: Naming,
    },
    /// Read, or change, how many hex digits of its ward a read names.
    Sweep {
        /// `None` reads; `Some` records. Fewer digits hide among more wards.
        digits: Option<u8>,
    },
    /// Cut the peer of a channel off, immediately and permanently.
    Revoke {
        /// Which channel.
        name: String,
    },
    /// Replace the roster of a group, creating it if there is none.
    ///
    /// Whole rather than incremental: a group is these channels, and there is
    /// no add or remove to disagree with that. An empty roster is a group that
    /// reaches nobody, which is how one is taken out of use.
    Group {
        /// What the group is called here.
        name: String,
        /// The channels it stands for.
        members: Vec<String>,
    },
    /// Append one segment to every member of a group.
    ///
    /// One drop per member, each on its own channel under its own key. The cost
    /// is linear and so is the privacy: a member learns nothing about the others,
    /// because there is nothing shared for them to learn.
    Fanout {
        /// Which group.
        group: String,
        /// What every member's segment carries.
        payload: Vec<u8>,
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
    /// Measure this machine instead of a host.
    ///
    /// Apart from [`Request::Doctor`] rather than an option on it, because the
    /// two answer different questions and produce different reports: one is
    /// about somebody else's promise and needs the network, the other is about
    /// this side and needs nothing.
    Here,
    /// Answer an agent over the Model Context Protocol, on stdin and stdout.
    ///
    /// A transport rather than a daemon: every call inside it opens the site,
    /// does one thing and closes it, so killing this changes no result.
    Port,
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
