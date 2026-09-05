// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The verb set as a command line, and what each verb becomes.
//!
//! Apart from `main.rs` because the two change for different reasons: a new verb
//! adds a variant and a translation here, while how a result reaches a terminal
//! is settled next door and does not move when the verb set grows.
//!
//! **The enum is the authority and clap is one reading of it.** `kusanagi::Request`
//! defines what this program can be asked to do, so a second front end — a
//! socket, an MCP server — is an addition rather than a second parser that has
//! to be taught the same verbs again.

use std::path::PathBuf;

use core::num::NonZeroU32;

use clap::{Parser, Subcommand};
use kusanagi::HOST_ADDRESS;

/// A decentralised collaboration network for agents.
#[derive(Parser, Debug)]
// `bin_name` is spelled rather than taken from argv[0], so that the usage lines
// a person copies say `kusanagi` on every platform instead of `kusanagi.exe` on
// one of them.
#[command(name = "kusanagi", bin_name = "kusanagi", version, about, long_about = None)]
pub(crate) struct Cli {
    /// Where this endpoint keeps its identity and channels.
    ///
    /// Defaults to `%LOCALAPPDATA%\kusanagi` on Windows,
    /// `$XDG_DATA_HOME/kusanagi` elsewhere.
    //
    // No clap default: clap's defaults are static strings, and this one is a
    // question for the operating system that only `kusanagi::assembly` is
    // allowed to ask.
    #[arg(long, global = true)]
    pub(crate) root: Option<PathBuf>,

    /// Emit JSON instead of prose.
    #[arg(long, global = true)]
    pub(crate) json: bool,

    #[command(subcommand)]
    pub(crate) command: Option<Verb>,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Verb {
    /// Show this endpoint's handle, creating an identity if there is none.
    Id,
    /// List the channels this endpoint has.
    Channels,
    /// Open a channel and mint the one line that invites somebody to it.
    Invite {
        /// What to call the channel here, or `-` to read it from stdin.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Where the drops will live: a path, an http:// url, or s3://…
        #[arg(long, value_name = "LOCATOR")]
        waypoint: String,
        /// How many seconds the invitation and its grant remain valid.
        #[arg(long = "for", default_value_t = 604_800, value_name = "SECONDS")]
        lifetime: u64,
        /// What the invitee may do, as a comma-separated list of send and read.
        #[arg(long = "can", default_value = "send,read", value_name = "ABILITIES")]
        can: String,
        /// Write one drop every SECONDS whether or not there is anything to say.
        ///
        /// Turns `send` into a queue and `tick` into what empties it, so that
        /// how often this endpoint speaks stops depending on what it has to
        /// say. Costs one drop per period per direction and up to one period of
        /// latency. A scheduler outside this program runs the ticks.
        #[arg(long = "every", value_name = "SECONDS")]
        every: Option<NonZeroU32>,
        /// Delete each drop once the peer says they have read it.
        ///
        /// The keys go with it, so a host that kept a copy holds bytes nobody
        /// can open. **This site then becomes the only copy of the
        /// conversation**: run `kusanagi export` and keep the archive.
        #[arg(long)]
        release: bool,
    },
    /// Accept an invitation, read from stdin.
    ///
    /// The invitation is not an argument. It carries the channel secret and a
    /// signing key, and a command line is public: on Linux any account on the
    /// machine can read another process's arguments out of `/proc`, and the
    /// shell writes them to a history file that outlives the channel.
    Join {
        /// What to call the channel here, or `-` to read it from the first
        /// line of stdin, ahead of the invitation.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Write one drop every SECONDS whether or not there is anything to say.
        #[arg(long = "every", value_name = "SECONDS")]
        every: Option<NonZeroU32>,
        /// Delete each drop once the peer says they have read it, and burn the
        /// key. **This site then becomes the only copy.**
        #[arg(long)]
        release: bool,
    },
    /// Fill this channel's current slot, and look once.
    ///
    /// What a scheduler runs on a channel opened with `--every`. It writes
    /// exactly one drop per slot — whatever `send` queued, or a filler carrying
    /// nothing — so that an endpoint with everything to say and one with nothing
    /// produce the same traffic. Running it twice in one slot writes nothing the
    /// second time.
    Tick {
        /// Which channel, or `-` to read the name from stdin.
        #[arg(long = "from", value_name = "NAME")]
        name: String,
    },
    /// Append one segment to your stream on a channel, or on every channel in a
    /// group.
    Send {
        /// Which channel, or `-` to read the name from the first line of stdin
        /// and the text from the rest of it.
        #[arg(long = "to", value_name = "NAME", conflicts_with = "group")]
        name: Option<String>,
        /// Which group, or `-` to read its name from the first line of stdin.
        ///
        /// One segment per member, each on its own channel under its own key.
        /// A member whose host is unreachable is one row of the report, not a
        /// failure of the send.
        #[arg(long = "to-group", value_name = "NAME")]
        group: Option<String>,
        /// What the segment carries. Omit it to read the payload from stdin.
        text: Option<String>,
    },
    /// Say which channels one name stands for, replacing whatever it stood for.
    ///
    /// The members arrive on stdin, one per line, because a roster is the
    /// relationship graph and a command line is public. An empty list is a group
    /// that reaches nobody, which is how one is taken out of use.
    Group {
        /// What to call the group here, or `-` to read it from the first line of
        /// stdin ahead of the members.
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// Open a room: one secret, one ward every member sweeps, one signed roster.
    Room {
        /// What to call the room here, or `-` to read it from stdin.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Where the drops will live: a path, an http:// url, or s3://…
        #[arg(long, value_name = "LOCATOR")]
        waypoint: String,
    },
    /// Mint the one line that invites somebody into a room.
    RoomInvite {
        /// Which room, or `-` to read the name from stdin.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// How many seconds the invitation remains valid.
        #[arg(long = "for", default_value_t = 604_800, value_name = "SECONDS")]
        lifetime: u64,
    },
    /// Accept a room invitation, read from stdin.
    RoomJoin {
        /// What to call the room here, or `-` to read it from the first
        /// line of stdin, ahead of the invitation.
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// Append one segment to your stream in a room.
    RoomSend {
        /// Which room, or `-` to read the name from the first line of stdin
        /// and the text from the rest of it.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// What the segment carries. Omit it to read the payload from stdin.
        text: Option<String>,
    },
    /// Read a room: sweep its ward once, verify every member's stream.
    RoomRead {
        /// Which room, or `-` to read the name from stdin.
        #[arg(long, value_name = "NAME")]
        name: String,
        /// Report only what follows HEIGHT on HANDLE's stream; repeat per
        /// author, or give `-` once to read the floors from stdin, one per
        /// line after the name. Every stream is verified in full either way.
        #[arg(long, value_name = "HANDLE=HEIGHT")]
        after: Vec<String>,
    },
    /// Read the peer's stream on a channel, verifying it end to end.
    Read {
        /// Which channel, or `-` to read the name from stdin.
        #[arg(long = "from", value_name = "NAME")]
        name: String,
        /// Report only what follows this height. The stream is verified in full
        /// either way.
        #[arg(long, value_name = "HEIGHT")]
        after: Option<u64>,
        /// Read back your own stream on this channel instead of the peer's.
        ///
        /// This is how a program that was interrupted finds out how far it got
        /// without writing a segment to find out.
        #[arg(long)]
        mine: bool,
    },
    /// Say whether this endpoint may reach a host without a proxy, or ask.
    ///
    /// `--require` makes every verb that would reach a host refuse when
    /// `KUSANAGI_PROXY` is not set — a setting that fails closed survives a new
    /// shell or a scheduler task that forgot the variable. `--optional` lifts it.
    Proxy {
        /// Refuse to reach any host without `KUSANAGI_PROXY`.
        #[arg(long, conflicts_with = "optional")]
        require: bool,
        /// Reach a host directly when no proxy is set (the default).
        #[arg(long)]
        optional: bool,
    },
    /// Say what you want to be called, or ask.
    ///
    /// The name is signed by your key and travels inside every invitation and
    /// greeting you make from now on, so a peer sees it beside your handle and
    /// can check it is yours. It is one printable line of at most 32 bytes. It
    /// is not a proof of who you are — the handle and the check code are — and
    /// peers you met before you set it will not see it.
    Name {
        /// The name, or `-` to read it from stdin.
        #[arg(long = "as", value_name = "NAME", conflicts_with = "clear")]
        alias: Option<String>,
        /// Stop declaring a name.
        #[arg(long)]
        clear: bool,
    },
    /// Say how many hex digits of your ward a read names, or ask.
    ///
    /// Four is your ward alone. Each digit fewer hides your reads among sixteen
    /// times as many wards and downloads what all of them received; `0` is the
    /// whole host. Nobody else is told, and a scheduler task sweeps the same
    /// width as you do.
    Sweep {
        /// How many of the four digits to name, 0 through 4.
        #[arg(long, value_name = "N")]
        digits: Option<u8>,
    },
    /// Cut the peer of a channel off, immediately and permanently.
    Revoke {
        /// Which channel, or `-` to read the name from stdin.
        #[arg(long = "from", value_name = "NAME")]
        name: String,
    },
    /// Delete a channel from this endpoint. It cannot be re-entered afterwards.
    Forget {
        /// Which channel, or `-` to read the name from stdin.
        #[arg(long = "channel", value_name = "NAME")]
        name: String,
    },
    /// Measure what a host actually does before trusting it with anything.
    Doctor {
        /// The waypoint to measure.
        #[arg(required_unless_present = "here", conflicts_with = "here")]
        waypoint: Option<String>,
        /// Measure this machine instead: where the site is, how its records are
        /// sealed, whether a proxy is set, and what this binary hashes to.
        #[arg(long)]
        here: bool,
    },
    /// Seal this endpoint's identity, channels and progress into one archive.
    ///
    /// The archive goes to stdout; the key that opens it goes to stderr, once.
    /// Nothing keeps a copy of that key, so a lost one is a lost archive.
    Export,
    /// Restore an archive into a `--root` that has nothing in it.
    ///
    /// The recovery key is the first line of stdin and the archive is the rest,
    /// because a command line is public while the process runs and is written to
    /// a history file afterwards.
    Import,
    /// Answer an agent over the Model Context Protocol, on stdin and stdout.
    ///
    /// The same verbs as this command line, through the door an agent is
    /// already standing at. Every call opens the site, does one thing and
    /// closes it, so killing this loses nothing.
    Port,
    /// Hold other people's drops. This is the untrusted half of the network.
    Host {
        /// The address to listen on: HOST:PORT, a bare port, or 0 for any free
        /// port.
        ///
        /// A bare port means loopback, so `--bind 9000` is `127.0.0.1:9000`.
        /// Reaching this host from another machine takes the long form, which is
        /// how that decision stays visible.
        #[arg(long, default_value = HOST_ADDRESS, value_name = "ADDRESS")]
        bind: String,
        /// The directory to keep drops in.
        ///
        /// Defaults to the site directory with `-host` after it.
        #[arg(long = "dir", value_name = "PATH")]
        directory: Option<PathBuf>,
        /// The most this host will hold, in bytes.
        ///
        /// A write that would take it over the ceiling is dropped, and answered
        /// exactly like every other write: a host that reported being full would
        /// be telling a stranger how much of it they had used.
        #[arg(long = "cap", default_value_t = kusanagi_box::CAPACITY, value_name = "BYTES")]
        capacity: u64,
    },
}
