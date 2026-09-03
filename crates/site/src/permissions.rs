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
//! system is `0644`: every user on the machine, every process in the container,
//! every layer of the image, and every backup that preserves modes gets the
//! channel secret. Nothing in the threat model requires an attacker with root, a
//! stolen laptop or a seized disk — a second account on a shared build machine is
//! enough.
//!
//! So every file this crate writes is created `0600` and every directory `0700`.
//!
//! **The mode is established at creation and never adjusted afterwards, and that
//! rule is a security property rather than an implementation detail.**
//! `set_permissions` takes a path and follows symbolic links, so chmod-ing a
//! file this build did not create hands anybody who can write into a site
//! directory a way to aim that chmod at a file its owner cares about.
//!
//! So a write that replaces a record replaces the **inode**: it is staged beside
//! the target and renamed over it, which acts on the name, so a symbolic link
//! sitting there is replaced rather than followed and a reader never sees half a
//! record. `waypoint::dir` makes a drop appear whole in the same shape.
//!
//! A directory this build did not create keeps the mode it has. Every file
//! inside it is `0600` regardless, so such a site exposes the set of channel
//! names and nothing in them.
//!
//! **On Windows these functions do nothing, and that is stated rather than
//! hidden.** The Unix mode bits have no counterpart there; restricting a file
//! means writing a discretionary ACL, which needs an API this workspace cannot
//! reach without `unsafe` or a crate that brings one. A Windows site therefore
//! relies on the profile directory it sits in, and closing that properly is a
//! separate change with its own dependency argument.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::SiteError;

/// Creates `path` and every missing parent, readable by nobody else.
///
/// # Errors
///
/// [`SiteError::Local`] when the directory cannot be created.
pub(crate) fn create_dir(path: &Path, action: &'static str) -> Result<(), SiteError> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt as _;
        builder.mode(0o700);
    }
    builder
        .create(path)
        .map_err(|source| SiteError::Local { action, source })
}

/// Writes `bytes` to `path`, readable by nobody else, replacing what was there.
///
/// Staged beside the target and renamed over it, so what is replaced is the
/// name. This never opens, truncates or chmods whatever was there before, and a
/// symbolic link planted at the target is overwritten rather than followed.
///
/// # Errors
///
/// [`SiteError::Local`] when the file cannot be written or cannot be moved into
/// place.
pub(crate) fn write(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), SiteError> {
    let staged = staging(path, action)?;
    let mut options = opening();
    options.create_new(true);
    put(&options, &staged, bytes, action)?;
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
    let mut options = opening();
    options.create_new(true);
    put(&options, path, bytes, action)
}

/// How this crate opens anything it is going to write.
///
/// The one place the mode is chosen. Asking for it at creation is the half of
/// this with no race in it: the file never exists, even briefly, in a state
/// somebody else could open.
fn opening() -> fs::OpenOptions {
    let mut options = fs::OpenOptions::new();
    options.write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    options
}

/// Creates the file and puts the bytes on the disk.
///
/// Every caller passes options carrying `create_new`, so this only ever opens a
/// file that did not exist a moment ago — which is also what makes it safe on a
/// path somebody else can reach: `O_CREAT | O_EXCL` refuses an existing name,
/// including a symbolic link, rather than following it.
fn put(
    options: &fs::OpenOptions,
    path: &Path,
    bytes: &[u8],
    action: &'static str,
) -> Result<(), SiteError> {
    use std::io::Write as _;
    let local = |source| SiteError::Local { action, source };
    let mut file = options.open(path).map_err(local)?;
    file.write_all(bytes).map_err(local)?;
    file.sync_all().map_err(local)
}
