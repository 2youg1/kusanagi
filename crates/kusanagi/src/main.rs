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
mod verbs;

use std::process::ExitCode;

use clap::error::ErrorKind;
use clap::{CommandFactory as _, Parser as _};
use kusanagi::{Complaint, Outcome, Site};

use crate::verbs::{Cli, request};

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

    // One fence per invocation, drawn before anything a peer wrote is rendered.
    let fence = match kusanagi::fresh_fence() {
        Ok(fence) => fence,
        Err(complaint) => {
            eprintln!("{}", complaint.render(cli.json));
            return ExitCode::FAILURE;
        }
    };
    match kusanagi::run(&Site::at(&root), &request) {
        // An archive is bytes, not text, so it goes to stdout on its own and
        // what happened goes to stderr beside it. Every other verb is the other
        // way round; this one is the only verb whose result is a file.
        Ok(outcome @ Outcome::Exported { .. }) => {
            eprintln!("{}", outcome.render(cli.json, fence));
            let Outcome::Exported { archive, .. } = &outcome else {
                return ExitCode::FAILURE;
            };
            match std::io::Write::write_all(&mut std::io::stdout(), archive) {
                Ok(()) => ExitCode::SUCCESS,
                Err(_) => ExitCode::FAILURE,
            }
        }
        Ok(outcome) => {
            println!("{}", outcome.render(cli.json, fence));
            ExitCode::SUCCESS
        }
        Err(complaint) => {
            eprintln!("{}", complaint.render(cli.json));
            ExitCode::FAILURE
        }
    }
}
