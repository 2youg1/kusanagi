// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! The only place that knows which concrete things exist.
//!
//! Everything below this module is written against traits and values. This is
//! where a directory becomes *the* waypoint, and it is where the clock will be
//! sampled when there is one. Keeping that knowledge in one file is what lets the
//! rest of the binary be driven by a test.

use std::path::Path;

use kusanagi_kernel::{Handle, PutOutcome, Segment, Waypoint, public_v0};
use kusanagi_waypoint::DirWaypoint;

use crate::Command;
use crate::complaint::Complaint;
use crate::report::Outcome;
use crate::walk;

/// Carries out one command against a directory waypoint.
///
/// # Errors
///
/// Any [`Complaint`] the command produces, already carrying the command that
/// would recover from it.
pub fn run(root: &Path, command: &Command) -> Result<Outcome, Complaint> {
    let waypoint = DirWaypoint::new(root);
    match command {
        Command::Send { author, payload } => send(&waypoint, author, payload.as_bytes().to_vec()),
        Command::Read { author } => read(&waypoint, author),
    }
}

fn send(waypoint: &impl Waypoint, name: &str, payload: Vec<u8>) -> Result<Outcome, Complaint> {
    let author = Handle::from_name(name);

    // The height comes from the waypoint, not from a file on this disk. Stage 0
    // therefore keeps no local state at all, which is the strongest form of the
    // rule that killing this process changes nothing.
    let walked = walk::chain(waypoint, &author)?;
    let segment = match walked.head() {
        None => Segment::genesis(author, payload),
        Some(head) => Segment::extend(author, payload, head),
    }?;

    let address = public_v0(&author, segment.index());
    let outcome = waypoint.put_if_absent(&address, &segment.to_canonical_bytes())?;
    if outcome == PutOutcome::AlreadyPresent {
        return Err(Complaint::DropTaken {
            address: address.to_string(),
            name: name.to_owned(),
        });
    }

    Ok(Outcome::sent(name, &segment, &address))
}

fn read(waypoint: &impl Waypoint, name: &str) -> Result<Outcome, Complaint> {
    let author = Handle::from_name(name);
    let walked = walk::chain(waypoint, &author)?;
    Ok(Outcome::read(name, &author, &walked))
}
