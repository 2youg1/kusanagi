// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How many hex digits of its ward this site names when it sweeps.
//!
//! Four is one ward; every digit dropped multiplies by sixteen both the wards a
//! read is indistinguishable among and the bytes it downloads. The choice is the
//! reader's alone — no writer, host or peer is told — and it is a site record
//! rather than a flag so that a scheduler task and a person at a terminal sweep
//! the same width: a reader that widened once and then narrowed would be the
//! one reader in the ward whose requests changed shape.
//!
//! One ASCII digit, `0` through `4`. Absent means the build's default.

use std::path::Path;

use crate::error::SiteError;
use crate::site::Site;
use kusanagi_vault as vault;

const FILE: &str = "sweep";

/// The most digits a ward has, and so the most this record may say.
pub const MOST_DIGITS: u8 = 4;

fn read(root: &Path) -> Result<Option<u8>, SiteError> {
    let Some(bytes) = vault::read(&root.join(FILE), "read the sweep record")? else {
        return Ok(None);
    };
    match bytes.trim_ascii() {
        [digit @ b'0'..=b'4'] => Ok(Some(digit.saturating_sub(b'0'))),
        _ => Err(SiteError::BadRecord {
            what: "the sweep record",
            reason: "this file does not hold one digit from 0 to 4".to_owned(),
        }),
    }
}

fn write(root: &Path, digits: u8) -> Result<(), SiteError> {
    vault::create_dir(root, "create the site directory")?;
    let digit = [b'0'.saturating_add(digits.min(MOST_DIGITS))];
    vault::write(&root.join(FILE), &digit, "write the sweep record").map_err(Into::into)
}

impl Site {
    /// How many hex digits of its ward this site names when it sweeps, when it
    /// has said.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be read, and
    /// [`SiteError::BadRecord`] when it holds something other than one digit.
    pub fn sweep_digits(&self) -> Result<Option<u8>, SiteError> {
        read(&self.root)
    }

    /// Records how many hex digits of its ward this site names when it sweeps,
    /// saturating at [`MOST_DIGITS`].
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn set_sweep_digits(&self, digits: u8) -> Result<(), SiteError> {
        write(&self.root, digits)
    }
}
