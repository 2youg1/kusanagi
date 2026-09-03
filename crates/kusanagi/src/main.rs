// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The one door into kusanagi.
//!
//! This file translates arguments into a [`Request`] and prints the result. It
//! holds no logic of its own, deliberately: the verb set is defined by the
//! library, so the program a test drives and the program a person runs are the
//! same program.

mod intake;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory as _, Parser, Subcommand};
use kusanagi::{Complaint, Request, Site, Whose};
use kusanagi_grant::{Abilities, Ability};

/// A decentralised collaboration network for agents.
#[derive(Parser, Debug)]
// `bin_name` is spelled rather than taken from argv[0], so that the usage lines
// a person copies say `kusanagi` on every platform instead of `kusanagi.exe` on
// one of them.
#[command(name = "kusanagi", bin_name = "kusanagi", version, about, long_about = None)]
struct Cli {
    /// Where this endpoint keeps its identity and channels.
    ///
    /// Defaults to `%LOCALAPPDATA%\kusanagi` on Windows,
    /// `$XDG_DATA_HOME/kusanagi` elsewhere.
    //
    // No clap default: clap's defaults are static strings, and this one is a
    // question for the operating system that only `kusanagi::assembly` is
    // allowed to ask.
    #[arg(long, global = true)]
    root: Option<PathBuf>,

    /// Emit JSON instead of prose.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Option<Verb>,
}

#[derive(Subcommand, Debug)]
enum Verb {
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
    },
    /// Append one segment to your stream on a channel.
    Send {
        /// Which channel, or `-` to read the name from the first line of stdin
        /// and the text from the rest of it.
        #[arg(long = "to", value_name = "NAME")]
        name: String,
        /// What the segment carries. Omit it to read the payload from stdin.
        text: Option<String>,
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
        waypoint: String,
    },
    /// Hold other people's drops. This is the untrusted half of the network.
    Host {
        /// The address to listen on.
        #[arg(long, default_value = "127.0.0.1:8443", value_name = "ADDRESS")]
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

fn request(verb: Verb) -> Result<Request, Complaint> {
    Ok(match verb {
        Verb::Id => Request::Identity,
        Verb::Channels => Request::Channels,
        Verb::Invite {
            name,
            waypoint,
            lifetime,
            can,
        } => Request::Invite {
            name: intake::channel(name)?,
            waypoint,
            lifetime,
            abilities: abilities(&can)?,
        },
        Verb::Join { name } => {
            let (name, invite) = intake::invited(name)?;
            Request::Join { invite, name }
        }
        Verb::Send { name, text } => {
            let (name, payload) = intake::addressed(name, text)?;
            Request::Send { name, payload }
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
        Verb::Doctor { waypoint } => Request::Doctor { waypoint },
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

/// Whether `--json` was asked for, read from the raw arguments.
///
/// Used only when parsing failed, which is exactly when the parsed value is not
/// available and the caller's choice of rendering still has to be honoured. An
/// agent that mistypes a flag must not be answered in prose.
fn asked_for_json() -> bool {
    std::env::args().any(|argument| argument == "--json")
}

/// Turns a parse failure into the one shape every other failure has.
///
/// Help and version are not failures: clap reports them as errors because they
/// stop the program, so they are printed as asked and the program leaves
/// successfully. Everything else becomes a [`Complaint`] with a stable code and
/// a way out, because a caller that cannot read the answer has not been
/// answered — and the caller here is usually a program.
fn misread(error: &clap::Error) -> Result<String, Complaint> {
    if matches!(
        error.kind(),
        ErrorKind::DisplayHelp
            | ErrorKind::DisplayVersion
            | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        return Ok(error.render().to_string());
    }
    let rendered = error.render().to_string();
    // Clap's own text carries the useful part — which argument, and what it
    // resembles — followed by a usage block this door replaces with a command.
    let said = rendered
        .split("\nUsage:")
        .next()
        .unwrap_or(&rendered)
        .trim()
        .trim_start_matches("error: ");
    // Clap separates its tip with a blank line, which would leave a hole in the
    // middle of a three-line complaint. The tip itself is kept: knowing that
    // `read` exists is most of the way out of having typed `rea`.
    let reason: Vec<&str> = said
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.trim().is_empty())
        .collect();
    Err(Complaint::Argument {
        what: "the command line",
        reason: format!(
            "is not one this program can act on: {}",
            reason.join("\n  ")
        ),
        instead: "run `kusanagi --help` for the verbs, or `kusanagi <VERB> --help` for one verb's flags",
    })
}

/// How much stack one command is given.
///
/// ML-DSA-87 is a lattice scheme: signing and verifying hold several matrices of
/// 256-coefficient polynomials as ordinary locals, and rejection sampling calls
/// into them repeatedly. That is tens of kilobytes per call in a call chain that
/// is already several crates deep, and the main thread of a process on Windows
/// gets one megabyte by default — which this overran the day the signature
/// scheme changed, as a stack overflow with no error to report.
///
/// So the work runs on a thread this program sizes itself. Eight megabytes is
/// what Rust gives a spawned thread by default on the platforms that have a
/// choice, and the cost is one thread per command in a program that exits.
const STACK: usize = 8 * 1_024 * 1_024;

fn main() -> ExitCode {
    // A thread rather than a linker flag, because a linker flag is per-target
    // and this has to hold on every platform this ships to.
    let Ok(worker) = std::thread::Builder::new().stack_size(STACK).spawn(run) else {
        eprintln!("error: this program could not start a thread to work on");
        return ExitCode::FAILURE;
    };
    worker.join().unwrap_or(ExitCode::FAILURE)
}

fn run() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => match misread(&error) {
            // Help and version go to stdout, where a result goes.
            Ok(text) => {
                print!("{text}");
                return ExitCode::SUCCESS;
            }
            Err(complaint) => {
                eprintln!("{}", complaint.render(asked_for_json()));
                return ExitCode::FAILURE;
            }
        },
    };
    // A command line with no verb is a question, not a failure: it is what a
    // person types to find out what this program does.
    let Some(command) = cli.command else {
        print!("{}", Cli::command().render_help());
        return ExitCode::SUCCESS;
    };
    let root = match cli.root.map_or_else(kusanagi::default_root, Ok) {
        Ok(root) => root,
        Err(complaint) => {
            eprintln!("{}", complaint.render(cli.json));
            return ExitCode::FAILURE;
        }
    };
    let request = match request(command) {
        Ok(request) => request,
        // An argument this program cannot act on is a failure like any other, so
        // it leaves by the same door: a stable code and the way forward, in
        // whichever of the two renderings the caller asked for.
        Err(complaint) => {
            eprintln!("{}", complaint.render(cli.json));
            return ExitCode::FAILURE;
        }
    };

    match kusanagi::run(&Site::at(&root), &request) {
        Ok(outcome) => {
            println!("{}", outcome.render(cli.json));
            ExitCode::SUCCESS
        }
        Err(complaint) => {
            eprintln!("{}", complaint.render(cli.json));
            ExitCode::FAILURE
        }
    }
}
