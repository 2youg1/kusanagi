// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Reading a whole directory of a site's records.
//!
//! Channels and groups are filed the same way — a keyed hash for a name, the
//! name itself inside the record — so listing them is one walk with one rule
//! about what to skip, and it lives here rather than once per kind.

use std::fs;
use std::path::Path;

use crate::channel::Channel;
use crate::error::SiteError;
use crate::room::Room;
use crate::site::Site;
use kusanagi_vault::{self as vault, Locked};

/// The bytes of every record in `directory`, or none when it does not exist.
///
/// A record that cannot be read is reported rather than skipped: something that
/// quietly stops being listed is something its owner believes they no longer
/// have. The one thing skipped is a staged file left behind by a write that did
/// not live to finish, which is the only name here that starts with a dot.
///
/// # Errors
///
/// [`SiteError::Local`] when the directory cannot be listed or a record cannot
/// be read.
pub(crate) fn each(
    root: &Path,
    directory: &str,
    action: &'static str,
) -> Result<Vec<Locked>, SiteError> {
    let entries = match fs::read_dir(root.join(directory)) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => return Err(SiteError::Local { action, source }),
        Ok(entries) => entries,
    };
    let mut found = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| SiteError::Local { action, source })?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if let Some(bytes) = vault::read(&entry.path(), action)? {
            found.push(bytes);
        }
    }
    Ok(found)
}

impl Site {
    /// Every channel name here, in a stable order.
    ///
    /// Each name is read out of its record, because the file is no longer
    /// named after it. That costs one read per channel and buys the property
    /// the file names used to give away; a listing is rare and short, and
    /// every caller of this opens each record immediately afterwards anyway.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when a record cannot be read, and
    /// [`SiteError::BadRecord`] when one does not decode.
    pub fn names(&self) -> Result<Vec<String>, SiteError> {
        let mut names = each(self.root(), "channels", "list the channels")?
            .iter()
            .map(|bytes| Channel::from_bytes(bytes).map(|channel| channel.name))
            .collect::<Result<Vec<String>, SiteError>>()?;
        names.sort();
        Ok(names)
    }

    /// Every room name here, in a stable order.
    ///
    /// Each name is read out of its record, because the file is no longer
    /// named after it — the same rule as [`Site::names`], for the same reason.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when a record cannot be read, and
    /// [`SiteError::BadRecord`] when one does not decode.
    pub fn room_names(&self) -> Result<Vec<String>, SiteError> {
        let mut names = each(self.root(), "rooms", "list the rooms")?
            .iter()
            .map(|bytes| Room::from_bytes(bytes).map(|room| room.name))
            .collect::<Result<Vec<String>, SiteError>>()?;
        names.sort();
        Ok(names)
    }
}
