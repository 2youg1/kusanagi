// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! The one door into kusanagi.
//!
//! Every command renders the same facts twice — once for a person and once as
//! JSON — from one structure, so the two can never disagree. That is not a
//! convenience: the caller on the other side of this door is usually an agent,
//! and an agent that has to parse prose is an agent that will parse it wrongly.

mod assembly;
mod complaint;
mod report;
mod walk;

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

/// A decentralised collaboration network for agents.
#[derive(Parser, Debug)]
#[command(name = "kusanagi", version, about, long_about = None)]
struct Cli {
    /// The directory used as a waypoint.
    #[arg(long, global = true, default_value = ".kusanagi")]
    root: PathBuf,

    /// Emit JSON instead of prose.
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Append one segment to a handle's chain.
    Send {
        /// The name whose chain is extended.
        #[arg(long = "as", value_name = "NAME")]
        author: String,
        /// What the segment carries.
        payload: String,
    },
    /// Read a handle's chain and verify it end to end.
    Read {
        /// The name whose chain is read.
        #[arg(long = "from", value_name = "NAME")]
        author: String,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match assembly::run(&cli.root, &cli.command) {
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
