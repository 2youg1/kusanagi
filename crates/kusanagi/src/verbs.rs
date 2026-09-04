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
use kusanagi::{Cadence, Complaint, HOST_ADDRESS, Habit, Request, Retention, Whose};
use kusanagi_grant::{Abilities, Ability};

use crate::intake;

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

/// Reads `send,read` into a set of abilities.
///
/// An unknown word is refused rather than ignored: an invitation that silently
/// granted less than it was asked for would be discovered by the person it was
/// given to, days later, as a failure they cannot explain.
/// Turns the two flags a channel is opened with into the value they mean.
///
/// Absent is the default in both cases, and the default is the one that promises
/// nothing: write when asked, keep everything.
fn habit(every: Option<NonZeroU32>, release: bool) -> Habit {
    Habit {
        cadence: every.map_or(Cadence::OnDemand, |period| Cadence::Slotted { period }),
        retention: if release {
            Retention::ReleaseOnAck
        } else {
            Retention::Keep
        },
    }
}

fn abilities(text: &str) -> Result<Abilities, Complaint> {
    let mut abilities = Abilities::NONE;
    for word in text
        .split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
    {
        match word {
            "send" => abilities = abilities.with(Ability::Send),
            "read" => abilities = abilities.with(Ability::Read),
            other => {
                return Err(Complaint::Argument {
                    what: "--can",
                    reason: format!("does not know the ability `{other}`"),
                    instead: "pass a comma-separated list of send and read",
                });
            }
        }
    }
    Ok(abilities)
}

pub(crate) fn request(verb: Verb) -> Result<Request, Complaint> {
    Ok(match verb {
        Verb::Id => Request::Identity,
        Verb::Channels => Request::Channels,
        Verb::Invite {
            name,
            waypoint,
            lifetime,
            can,
            every,
            release,
        } => Request::Invite {
            name: intake::channel(name)?,
            waypoint,
            lifetime,
            abilities: abilities(&can)?,
            habit: habit(every, release),
        },
        Verb::Join {
            name,
            every,
            release,
        } => {
            let (name, invite) = intake::invited(name)?;
            Request::Join {
                invite,
                name,
                habit: habit(every, release),
            }
        }
        Verb::Tick { name } => Request::Tick {
            name: intake::channel(name)?,
        },
        // Two destinations, and the command line carries at most one of them.
        // clap refuses both at once; the case it cannot express is neither, and
        // saying so here gives that a stable code and a way out.
        Verb::Send { name, group, text } => match (name, group) {
            (Some(name), _) => {
                let (name, payload) = intake::addressed(name, text)?;
                Request::Send { name, payload }
            }
            (None, Some(group)) => {
                let (group, payload) = intake::addressed(group, text)?;
                Request::Fanout { group, payload }
            }
            (None, None) => {
                return Err(Complaint::Argument {
                    what: "send",
                    reason: "was given nobody to send to".to_owned(),
                    instead: "pass --to NAME for one channel, or --to-group NAME for a group",
                });
            }
        },
        Verb::Doctor { waypoint, here } => match waypoint {
            Some(waypoint) => Request::Doctor { waypoint },
            // `required_unless_present` has already refused the empty case, so
            // reaching here means `--here` was given.
            None if here => Request::Here,
            None => {
                return Err(Complaint::Argument {
                    what: "doctor",
                    reason: "was given nothing to measure".to_owned(),
                    instead: "pass a waypoint to measure a host, or --here for this machine",
                });
            }
        },
        Verb::Group { name } => {
            let (name, members) = intake::enrolled(name)?;
            Request::Group { name, members }
        }
        Verb::Read { name, after, mine } => Request::Read {
            name: intake::channel(name)?,
            after,
            // The flag is a flag because that is what a command line has; the
            // enum starts here so that nothing below carries an unnamed bool.
            whose: if mine { Whose::Mine } else { Whose::Peer },
        },
        Verb::Revoke { name } => Request::Revoke {
            name: intake::channel(name)?,
        },
        Verb::Forget { name } => Request::Forget {
            name: intake::channel(name)?,
        },
        Verb::Port => Request::Port,
        Verb::Export => Request::Export,
        Verb::Import => {
            let (recovery, archive) = intake::restored()?;
            Request::Import { recovery, archive }
        }
        Verb::Host {
            bind,
            directory,
            capacity,
        } => Request::Host {
            bind,
            directory: match directory {
                Some(named) => named,
                None => kusanagi::default_host_dir()?,
            },
            capacity,
        },
    })
}
