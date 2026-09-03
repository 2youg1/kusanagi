// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Unix: the mode is chosen at creation, and `0077` is always clear.
//!
//! Asking for the mode at creation is the half of this with no race in it: the
//! file never exists, even briefly, in a state somebody else could open.

use std::fs::{DirBuilder, File, OpenOptions};
use std::os::unix::fs::{DirBuilderExt as _, OpenOptionsExt as _};
use std::path::Path;

use crate::error::SiteError;

/// Creates `path` and every missing parent as `0700`.
pub(super) fn create_dir(path: &Path, action: &'static str) -> Result<(), SiteError> {
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|source| SiteError::Local { action, source })
}

/// Creates a file that must not already exist, as `0600`.
///
/// `O_CREAT | O_EXCL` refuses an existing name, including a symbolic link,
/// rather than following it.
pub(super) fn create_file(path: &Path, action: &'static str) -> Result<File, SiteError> {
    OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .map_err(|source| SiteError::Local { action, source })
}
