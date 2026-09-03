// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who, besides this endpoint, can read this endpoint's disk.
//!
//! A site holds the identity seed and every channel secret. Everything the
//! network's privacy rests on — which addresses exist, what is written at them,
//! who may write — follows from those bytes, so a site that any local account can
//! read is a site that any local account has joined.
//!
//! The default is not good enough. A file created by `fs::write` on a typical
//! Unix system is `0644`; on Windows it inherits whatever list the parent
//! directory carries, which is the list of a directory this program did not
//! choose. Every user on the machine, every process in the container, every
//! layer of the image, and every backup that preserves permissions gets the
//! channel secret. Nothing in the threat model requires an attacker with root, a
//! stolen laptop or a seized disk — a second account on a shared build machine is
//! enough.
//!
//! **The permission is established at creation and never adjusted afterwards,
//! and that rule is a security property rather than an implementation detail.**
//! `set_permissions` and `SetNamedSecurityInfoW` both take a *path* and resolve
//! it, so adjusting a thing this build did not create hands anybody who can write
//! into a site directory a way to aim that adjustment at a file its owner cares
//! about — through a symbolic link on Unix, through a junction on Windows.
//!
//! So a write that replaces a record replaces the **inode**: it is staged beside
//! the target and renamed over it, which acts on the name, so a link sitting
//! there is replaced rather than followed and a reader never sees half a record.
//! `waypoint::dir` makes a drop appear whole in the same shape.
//!
//! A directory this build did not create keeps the permissions it has. Every
//! file inside it is closed regardless, so such a site exposes the set of
//! channel names and nothing in them.
//!
//! **The platform difference is a file, not a branch.** This module holds the
//! part that is the same everywhere — staging, renaming, and refusing to touch
//! what it did not create — and `unix.rs` and `windows.rs` each hold one answer
//! to the two questions the platform actually decides: how a directory is
//! created, and how a file is created. A third platform is a third file and one
//! line here.

#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::SiteError;

/// Creates `path` and every missing parent, readable by nobody else.
///
/// # Errors
///
/// [`SiteError::Local`] when the directory cannot be created, and
/// [`SiteError::Permissions`] when the operating system will not attach the
/// restriction this site needs.
pub(crate) fn create_dir(path: &Path, action: &'static str) -> Result<(), SiteError> {
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
/// [`SiteError::Local`] when the file cannot be written or cannot be moved into
/// place.
pub(crate) fn write(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), SiteError> {
    let staged = staging(path, action)?;
    put(&staged, bytes, action)?;
    match fs::rename(&staged, path) {
        Ok(()) => Ok(()),
        Err(source) => {
            // The staged file is this crate's own and holds the same bytes the
            // caller was writing, so leaving it behind would leave a second copy
            // of a channel secret under a name nothing reads.
            fs::remove_file(&staged).ok();
            Err(SiteError::Local { action, source })
        }
    }
}

/// A name beside `path` that nothing else in this process will choose.
///
/// Beside it rather than in a temporary directory, because `rename` is only
/// atomic within one filesystem and a site may be on any of them.
fn staging(path: &Path, action: &'static str) -> Result<PathBuf, SiteError> {
    static TICKET: AtomicU64 = AtomicU64::new(0);
    let parent = path.parent().ok_or_else(|| SiteError::Local {
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
/// [`SiteError::Local`] when the file exists or cannot be written.
pub(crate) fn write_new(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), SiteError> {
    put(path, bytes, action)
}

/// Creates the file and puts the bytes on the disk.
///
/// The platform creates it, and every platform creates it exclusively — a name
/// that already exists is refused rather than followed, which is what makes this
/// safe on a path somebody else can reach.
fn put(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), SiteError> {
    let local = |source| SiteError::Local { action, source };
    let mut file = platform::create_file(path, action)?;
    file.write_all(bytes).map_err(local)?;
    file.sync_all().map_err(local)
}
