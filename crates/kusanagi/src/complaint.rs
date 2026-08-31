// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! What this binary says when it cannot do what was asked.
//!
//! Every complaint carries four things: what failed, what it failed on, a stable
//! code, and **the command that would recover**. The first three come from the
//! layer that failed; the fourth can only come from here, because only this layer
//! knows what the caller typed.

use kusanagi_chain::ChainError;
use kusanagi_kernel::{SegmentError, WaypointError};
use serde::Serialize;

/// A failure, in the shape a caller can act on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Complaint {
    /// The waypoint could not be read or written.
    #[error(transparent)]
    Waypoint(#[from] WaypointError),
    /// Bytes at an address are not a segment.
    #[error(transparent)]
    Segment(#[from] SegmentError),
    /// The segments found do not form a chain.
    #[error(transparent)]
    Chain(#[from] ChainError),
    /// Somebody else claimed the next address first.
    #[error("the next drop for {name} is already taken")]
    DropTaken {
        /// The address that was already occupied.
        address: String,
        /// Whose chain was being extended.
        name: String,
    },
}

/// A complaint rendered for both readers.
#[derive(Serialize)]
struct Rendered<'a> {
    error: &'a str,
    code: &'a str,
    recover: String,
}

impl Complaint {
    /// The stable code of the underlying failure.
    fn code(&self) -> &'static str {
        match self {
            Self::Waypoint(error) => error.code(),
            Self::Segment(error) => error.code(),
            Self::Chain(error) => error.code(),
            Self::DropTaken { .. } => "kusanagi.drop_taken",
        }
    }

    /// The command that would move the caller forward from here.
    fn recover(&self) -> String {
        match self {
            Self::Waypoint(_) => {
                "check that --root names a writable directory, then run the command again"
                    .to_owned()
            }
            Self::Segment(_) | Self::Chain(_) => {
                "run `kusanagi read --from <name>` to see how far the chain verifies".to_owned()
            }
            Self::DropTaken { name, .. } => {
                format!(
                    "run `kusanagi read --from {name}` to pick up the new head, then send again"
                )
            }
        }
    }

    /// Renders this complaint for a person or for a machine.
    #[must_use]
    pub fn render(&self, json: bool) -> String {
        let rendered = Rendered {
            error: &self.to_string(),
            code: self.code(),
            recover: self.recover(),
        };
        if json {
            return serde_json::to_string_pretty(&rendered)
                .unwrap_or_else(|_| format!(r#"{{"code":"{}"}}"#, rendered.code));
        }
        format!(
            "error: {}\n  code: {}\n  try:  {}",
            rendered.error, rendered.code, rendered.recover
        )
    }
}
