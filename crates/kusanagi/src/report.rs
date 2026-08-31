// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! What a command reports, in one structure rendered two ways.

use kusanagi_kernel::{DropAddr, Handle, Segment};
use serde::Serialize;

use crate::walk::Walked;

/// One segment as it is reported.
#[derive(Serialize)]
pub struct Entry {
    index: u64,
    id: String,
    address: String,
    /// Payloads are opaque bytes; this rendering is lossy on purpose and is for
    /// eyes only. Nothing downstream should parse it back.
    text: String,
}

/// What a command produced.
#[derive(Serialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum Outcome {
    /// A segment was appended.
    Sent {
        author: String,
        handle: String,
        index: u64,
        id: String,
        address: String,
    },
    /// A chain was read and verified.
    Read {
        author: String,
        handle: String,
        height: Option<u64>,
        head: Option<String>,
        segments: Vec<Entry>,
    },
}

impl Outcome {
    /// Reports an appended segment.
    #[must_use]
    pub fn sent(name: &str, segment: &Segment, address: &DropAddr) -> Self {
        Self::Sent {
            author: name.to_owned(),
            handle: segment.author().to_string(),
            index: segment.index(),
            id: segment.id().to_string(),
            address: address.to_string(),
        }
    }

    /// Reports a verified chain.
    #[must_use]
    pub fn read(name: &str, handle: &Handle, walked: &Walked) -> Self {
        let segments = walked
            .segments()
            .iter()
            .map(|(address, segment)| Entry {
                index: segment.index(),
                id: segment.id().to_string(),
                address: address.to_string(),
                text: String::from_utf8_lossy(segment.payload()).into_owned(),
            })
            .collect();
        Self::Read {
            author: name.to_owned(),
            handle: handle.to_string(),
            height: walked.head().map(|head| head.index()),
            head: walked.head().map(|head| head.id().to_string()),
            segments,
        }
    }

    /// Renders this outcome for a person or for a machine.
    #[must_use]
    pub fn render(&self, json: bool) -> String {
        if json {
            return serde_json::to_string_pretty(self)
                .unwrap_or_else(|error| format!(r#"{{"error":"{error}"}}"#));
        }
        match self {
            Self::Sent {
                author,
                index,
                id,
                address,
                ..
            } => format!("sent  {author} #{index}\n  id      {id}\n  address {address}"),
            Self::Read {
                author,
                height,
                segments,
                ..
            } => {
                let header = match height {
                    None => format!("{author} has no chain yet"),
                    Some(height) => format!(
                        "{author} verifies to height {height} ({} segment(s))",
                        segments.len()
                    ),
                };
                let mut lines = vec![header];
                lines.extend(
                    segments
                        .iter()
                        .map(|entry| format!("  #{:<3} {}", entry.index, entry.text)),
                );
                lines.join("\n")
            }
        }
    }
}
