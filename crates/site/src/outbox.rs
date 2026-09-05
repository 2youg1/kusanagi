// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a slotted channel has been asked to say, waiting for its slot.
//!
//! On an on-demand channel `send` writes a drop and returns. On a slotted one it
//! cannot: writing when the caller asks is exactly the rhythm a slot exists to
//! hide. So `send` leaves the payload here and `tick` takes one out when the
//! slot comes round.
//!
//! **This is state, and it is allowed to be, because a site is the one place
//! state is allowed to live** (`ARCHITECTURE.md` §4). It does not weaken law 1:
//! killing a `tick` either leaves the payload queued or leaves it written, and
//! both are states the next `tick` reads correctly from the host and the disk.
//! What it must never do is decide a *height* — that still comes from the
//! waypoint, so a queue is a list of things to say and never a claim about what
//! has been said.
//!
//! ```text
//! <root>/outbox/<filed>/<20 digits>   one queued payload
//! ```
//!
//! The name is the sequence number in decimal, zero-padded to twenty digits so
//! that the order the operating system lists them in is the order they were
//! queued. Nothing else about the file says anything: the payload inside is
//! sealed at rest with every other record.

use std::fs;
use std::path::{Path, PathBuf};

use crate::error::SiteError;
use kusanagi_vault as vault;

/// How wide a sequence number is written, which is `u64::MAX` in decimal.
const WIDTH: usize = 20;

/// One payload waiting for a slot.
#[derive(Debug, Clone)]
pub struct Queued {
    /// Where in the queue it sits. Passed back to [`clear`] once it is written.
    pub ticket: String,
    /// What the caller asked to send.
    pub payload: Vec<u8>,
}

/// Where one channel's queue sits, under the same filed name as its record.
fn dir(root: &Path, filed: &str) -> PathBuf {
    root.join("outbox").join(filed)
}

/// Adds `payload` to the end of the queue.
///
/// The ticket is one above the highest already there, so a queue survives a
/// process that was killed between two sends without a counter anywhere.
///
/// # Errors
///
/// [`SiteError::Local`] when the queue cannot be read or the record written.
pub(crate) fn push(root: &Path, filed: &str, payload: &[u8]) -> Result<(), SiteError> {
    let directory = dir(root, filed);
    vault::create_dir(&directory, "create the outbox")?;
    let next = tickets(&directory, "read the outbox")?
        .last()
        .and_then(|ticket| ticket.parse::<u64>().ok())
        .map_or(0, |highest| highest.saturating_add(1));
    vault::write_new(
        &directory.join(format!("{next:0WIDTH$}")),
        payload,
        "queue a payload",
    )
    .map_err(Into::into)
}

/// Everything waiting on this channel, oldest first.
///
/// One function rather than a `front` and a `depth` and a `list`: a caller that
/// wants the next payload takes the first of these, and a caller writing a
/// backup takes all of them. Three readers of one directory would be three
/// chances to disagree about what order it is in.
///
/// # Errors
///
/// [`SiteError::Local`] when the queue cannot be read.
pub(crate) fn all(root: &Path, filed: &str) -> Result<Vec<Queued>, SiteError> {
    let directory = dir(root, filed);
    let action = "read the outbox";
    let mut waiting = Vec::new();
    for ticket in tickets(&directory, action)? {
        if let Some(payload) = vault::read(&directory.join(&ticket), action)? {
            waiting.push(Queued {
                ticket,
                payload: payload.to_vec(),
            });
        }
    }
    Ok(waiting)
}

/// Removes one payload, once it is on the host.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be removed. It is an error rather
/// than a shrug: a payload that stays queued after it was written is one that
/// will be written again at the next slot, under a height that is already taken.
pub(crate) fn clear(root: &Path, filed: &str, ticket: &str) -> Result<(), SiteError> {
    match fs::remove_file(dir(root, filed).join(ticket)) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(SiteError::Local {
            action: "clear a queued payload",
            source,
        }),
    }
}

/// Every ticket in the queue, in the order they were added.
///
/// A staged file left behind by a write that did not finish starts with a dot
/// and is skipped, exactly as it is when records are listed.
fn tickets(directory: &Path, action: &'static str) -> Result<Vec<String>, SiteError> {
    let entries = match fs::read_dir(directory) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(SiteError::Local { action, source }),
        Ok(entries) => entries,
    };
    let mut found = Vec::new();
    for entry in entries {
        let name = entry
            .map_err(|source| SiteError::Local { action, source })?
            .file_name()
            .to_string_lossy()
            .into_owned();
        if !name.starts_with('.') {
            found.push(name);
        }
    }
    // Zero-padded to one width, so lexical order is numeric order.
    found.sort();
    Ok(found)
}
