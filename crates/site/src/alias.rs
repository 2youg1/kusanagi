// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What this endpoint calls itself.
//!
//! One file, the name as UTF-8, held to the rule `kernel::Alias` states when it
//! is read back. It is a site setting rather than a field of the identity record
//! because the two change for different reasons: the seed is who this endpoint
//! is and never changes, the name is what it asks to be called and may. The
//! signed form is made when it is needed — at `invite` and at `join` — from the
//! identity key, so nothing signed is stored.

use std::path::Path;

use kusanagi_kernel::Alias;

use crate::error::SiteError;
use crate::site::Site;
use kusanagi_vault as vault;

const FILE: &str = "alias";

/// The recorded name, or `None` when this endpoint declared nothing.
fn read(root: &Path) -> Result<Option<Alias>, SiteError> {
    let Some(bytes) = vault::read(&root.join(FILE), "read the alias record")? else {
        return Ok(None);
    };
    let unfit = |reason: String| SiteError::BadRecord {
        what: "the alias record",
        reason,
    };
    let text = core::str::from_utf8(&bytes).map_err(|error| unfit(error.to_string()))?;
    Alias::new(text)
        .map(Some)
        .map_err(|error| unfit(error.to_string()))
}

/// Records `alias`, or removes the record when there is none to record.
fn write(root: &Path, alias: Option<&Alias>) -> Result<(), SiteError> {
    vault::create_dir(root, "create the site directory")?;
    match alias {
        Some(alias) => vault::write(
            &root.join(FILE),
            alias.as_str().as_bytes(),
            "write the alias record",
        )
        .map_err(Into::into),
        None => match std::fs::remove_file(root.join(FILE)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SiteError::Local {
                action: "remove the alias record",
                source,
            }),
        },
    }
}

impl Site {
    /// What this endpoint calls itself, if it has said.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be read, and
    /// [`SiteError::BadRecord`] when it is not a name an alias may be.
    pub fn alias(&self) -> Result<Option<Alias>, SiteError> {
        read(&self.root)
    }

    /// Records what this endpoint calls itself; `None` clears it.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn set_alias(&self, alias: Option<&Alias>) -> Result<(), SiteError> {
        write(&self.root, alias)
    }
}
