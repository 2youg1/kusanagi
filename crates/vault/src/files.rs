// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Staging, renaming, and refusing to touch what this build did not create.
//!
//! The half of a vault that every platform answers the same way. What the
//! platform decides — the mode or the access list a thing is created with — is
//! reached through `platform`, one file per system.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::at_rest::{open_at_rest, seal_at_rest};
use crate::error::VaultError;
use crate::locked::Locked;
use crate::platform;

/// Creates `path` and every missing parent, readable by nobody else.
///
/// # Errors
///
/// [`VaultError::Local`] when the directory cannot be created, and
/// [`VaultError::Permissions`] when the operating system will not attach the
/// restriction this vault needs.
pub fn create_dir(path: &Path, action: &'static str) -> Result<(), VaultError> {
    platform::create_dir(path, action)
}

/// Writes `bytes` to `path`, readable by nobody else, replacing what was there.
///
/// Staged beside the target and renamed over it, so what is replaced is the
/// name. This never opens, truncates or re-permissions whatever was there
/// before, and a link planted at the target is overwritten rather than followed.
///
/// # Errors
///
/// [`VaultError::Local`] when the file cannot be written or cannot be moved into
/// place.
pub fn write(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), VaultError> {
    let staged = staging(path, action)?;
    put(&staged, &seal_at_rest(bytes)?, action)?;
    match fs::rename(&staged, path) {
        Ok(()) => Ok(()),
        Err(source) => {
            // The staged file is this crate's own and holds the same bytes the
            // caller was writing, so leaving it behind would leave a second copy
            // of a channel secret under a name nothing reads.
            fs::remove_file(&staged).ok();
            Err(VaultError::Local { action, source })
        }
    }
}

/// A name beside `path` that nothing else in this process will choose.
///
/// Beside it rather than in a temporary directory, because `rename` is only
/// atomic within one filesystem and a site may be on any of them.
fn staging(path: &Path, action: &'static str) -> Result<PathBuf, VaultError> {
    static TICKET: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().ok_or_else(|| VaultError::Local {
        action,
        source: std::io::Error::other("a site record needs a directory to sit in"),
    })?;
    let ticket = TICKET.fetch_add(1, Ordering::Relaxed);
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    Ok(parent.join(format!(".{name}.{}-{ticket}", std::process::id())))
}

/// Writes a file that must not already exist, readable by nobody else.
///
/// # Errors
///
/// [`VaultError::Local`] when the file exists or cannot be written.
pub fn write_new(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), VaultError> {
    put(path, &seal_at_rest(bytes)?, action)
}

/// Reads back what [`write`] or [`write_new`] put there.
///
/// **The one place a site record is read.** Sealing on the way out and opening
/// on the way in are one decision, so they live behind one pair of functions and
/// no caller of this module has to remember either.
///
/// # Errors
///
/// [`VaultError::Local`] when the file cannot be read, and whatever
/// [`open_at_rest`] reports for a record this platform has no
/// store for.
pub fn read(path: &Path, action: &'static str) -> Result<Option<Locked>, VaultError> {
    match fs::read(path) {
        Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(source) => Err(VaultError::Local { action, source }),
        Ok(stored) => open_at_rest(&stored).map(|plain| Some(Locked::holding(plain))),
    }
}

/// Creates the file and puts the bytes on the disk.
///
/// The platform creates it, and every platform creates it exclusively — a name
/// that already exists is refused rather than followed, which is what makes this
/// safe on a path somebody else can reach.
fn put(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), VaultError> {
    let local = |source| VaultError::Local { action, source };
    let mut file = platform::create_file(path, action)?;
    file.write_all(bytes).map_err(local)?;
    file.sync_all().map_err(local)
}
