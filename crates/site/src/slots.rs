// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The last slot this endpoint wrote in, on a channel that has slots.
//!
//! One number per channel, and the only reason it exists is that a segment
//! carries no timestamp. It carries none on purpose: a time inside a sealed drop
//! is a time its author can be compelled to produce, and this network spends
//! every derived address to avoid holding that kind of evidence. So *when* a
//! height was written cannot be recovered from the stream, and the endpoint that
//! wrote it keeps the one fact it needs — which slot it has already filled.
//!
//! **It is written before the drop, not after.** A tick that dies between the
//! two therefore skips a slot rather than filling one twice. Both outcomes are
//! wrong; they are not equally wrong. A skipped slot is a gap, which is the
//! residual `cadence.rs` already writes down and which an observer sees whenever
//! this endpoint is offline. Two drops in one slot is a burst, and a burst is
//! precisely the shape slots exist to remove.

use std::path::Path;

use crate::error::SiteError;
use crate::permissions;

/// Which slot was last filled on this channel, if any has been.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be read, and
/// [`SiteError::BadRecord`] when it is not a slot number.
pub(crate) fn read(root: &Path, filed: &str) -> Result<Option<u64>, SiteError> {
    let path = root.join("slots").join(filed);
    let Some(bytes) = permissions::read(&path, "read the last slot")? else {
        return Ok(None);
    };
    <[u8; 8]>::try_from(&*bytes)
        .map(|number| Some(u64::from_be_bytes(number)))
        .map_err(|_| SiteError::BadRecord {
            what: "a slot record",
            reason: format!("a slot is eight bytes; this one is {}", bytes.len()),
        })
}

/// Writes down that `slot` is being filled.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be written.
pub(crate) fn write(root: &Path, filed: &str, slot: u64) -> Result<(), SiteError> {
    let directory = root.join("slots");
    permissions::create_dir(&directory, "create the slot directory")?;
    permissions::write(
        &directory.join(filed),
        &slot.to_be_bytes(),
        "write the last slot",
    )
}
