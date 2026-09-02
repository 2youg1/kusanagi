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

use std::io::{IsTerminal, Read};
use std::path::PathBuf;
use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory as _, Parser, Subcommand};
use kusanagi::{Complaint, Request, Site, Whose};
use kusanagi_grant::{Abilities, Ability};
use kusanagi_kernel::MAX_PAYLOAD;

/// A decentralised collaboration network for agents.
#[derive(Parser, Debug)]
// `bin_name` is spelled rather than taken from argv[0], so that the usage lines
// a person copies say `kusanagi` on every platform instead of `kusanagi.exe` on
// one of them.
#[command(name = "kusanagi", bin_name = "kusanagi", version, about, long_about = None)]
struct Cli {
    /// Where this endpoint keeps its identity and channels.
    #[arg(long, global = true, default_value = ".kusanagi")]
    root: PathBuf,

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
        /// What to call the channel here.
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
    /// Accept an invitation.
    Join {
        /// The invitation, as one line.
        invite: String,
        /// What to call the channel here.
        #[arg(long, value_name = "NAME")]
        name: String,
    },
    /// Append one segment to your stream on a channel.
    Send {
        /// Which channel.
        #[arg(long = "to", value_name = "NAME")]
        name: String,
        /// What the segment carries. Omit it to read the payload from stdin.
        text: Option<String>,
    },
    /// Read the peer's stream on a channel, verifying it end to end.
    Read {
        /// Which channel.
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
        /// Which channel.
        #[arg(long = "from", value_name = "NAME")]
        name: String,
    },
    /// Delete a channel from this endpoint. It cannot be re-entered afterwards.
    Forget {
        /// Which channel.
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
        #[arg(long = "dir", default_value = ".kusanagi-host", value_name = "PATH")]
        directory: PathBuf,
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

/// The bytes a segment will carry: what was typed, or what was piped in.
///
/// Reading stdin when no text is given is what lets a caller send a payload with
/// quotes, newlines, or bytes that are not text at all — none of which survive a
/// command line intact. The read is bounded at one byte past the limit a segment
/// accepts, so an oversized payload is refused by the rule that owns that limit
/// instead of being buffered here first.
fn payload(text: Option<String>) -> Result<Vec<u8>, Complaint> {
    if let Some(text) = text {
        return Ok(text.into_bytes());
    }
    let input = std::io::stdin();
    if input.is_terminal() {
        return Err(Complaint::Argument {
            what: "the text to send",
            reason: "was not given, and stdin is a terminal".to_owned(),
            instead: "pass it as an argument, or pipe it in: \
                      echo hello | kusanagi send --to NAME",
        });
    }
    let mut bytes = Vec::new();
    input
        .take(u64::from(MAX_PAYLOAD).saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|source| Complaint::Local {
            action: "read the payload from stdin",
            source,
        })?;
    Ok(bytes)
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
            name,
            waypoint,
            lifetime,
            abilities: abilities(&can)?,
        },
        Verb::Join { invite, name } => Request::Join { invite, name },
        Verb::Send { name, text } => Request::Send {
            name,
            payload: payload(text)?,
        },
        Verb::Read { name, after, mine } => Request::Read {
            name,
            after,
            // The flag is a flag because that is what a command line has; the
            // enum starts here so that nothing below carries an unnamed bool.
            whose: if mine { Whose::Mine } else { Whose::Peer },
        },
        Verb::Revoke { name } => Request::Revoke { name },
        Verb::Forget { name } => Request::Forget { name },
        Verb::Doctor { waypoint } => Request::Doctor { waypoint },
        Verb::Host { bind, directory } => Request::Host { bind, directory },
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

fn main() -> ExitCode {
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

    match kusanagi::run(&Site::at(&cli.root), &request) {
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
