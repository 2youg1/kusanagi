// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How far one lane's keys have been burned, on a channel that releases.
//!
//! Filed exactly like a cairn — one directory per channel, one file per author —
//! and governed by the opposite failure policy, which is why it is not stored
//! with one. **A cairn can always be rebuilt by reading the stream again, so
//! every way of failing to read one means "not having one". A ratchet cannot be
//! rebuilt by anything: that is what it is for.** Losing one loses every message
//! below its floor, permanently, which is why `export` carries it and why
//! `Retention::ReleaseOnAck` says out loud that a backup has stopped being
//! optional.
//!
//! A missing ratchet is therefore not the same as a lost one, and this module
//! does not try to tell them apart. `Keyring` decides what an absent ratchet
//! means for a given lane: at height zero it means the lane has not started, and
//! above it the caller has a cairn saying otherwise and refuses.

use std::path::{Path, PathBuf};

use kusanagi_seal::Ratchet;

use crate::error::SiteError;
use kusanagi_vault as vault;

/// Where one channel's ratchets sit, under the same filed name as its record.
pub(crate) fn dir(root: &Path, filed: &str) -> PathBuf {
    root.join("ratchets").join(filed)
}

/// How far one author's lane has been burned, if this endpoint has a record.
///
/// # Errors
///
/// [`SiteError::Local`] when the record exists and cannot be read, and
/// [`SiteError::BadRecord`] when it is not a ratchet. Neither is softened into
/// "no record": an unreadable ratchet is a lane that must not be walked, and
/// treating it as absent would silently start a second one at height zero.
pub(crate) fn read(
    root: &Path,
    filed: &str,
    filed_author: &str,
) -> Result<Option<Ratchet>, SiteError> {
    let path = dir(root, filed).join(filed_author);
    let Some(bytes) = vault::read(&path, "read a ratchet")? else {
        return Ok(None);
    };
    Ratchet::from_bytes(&bytes)
        .map(Some)
        .ok_or(SiteError::BadRecord {
            what: "a ratchet",
            reason: "this record is not a ratchet this build wrote".to_owned(),
        })
}

/// Writes down that everything below `ratchet`'s floor is now unopenable.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be written.
pub(crate) fn write(
    root: &Path,
    filed: &str,
    filed_author: &str,
    ratchet: &Ratchet,
) -> Result<(), SiteError> {
    let directory = dir(root, filed);
    vault::create_dir(&directory, "create the ratchet directory")?;
    vault::write(
        &directory.join(filed_author),
        &ratchet.to_bytes(),
        "write a ratchet",
    )
    .map_err(Into::into)
}
