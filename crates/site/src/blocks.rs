// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Length-prefixed blocks: the one framing every record on this disk shares.
//!
//! Channels, rosters and archives all carry variable-length fields, and they all
//! carry them the same way. Written once here so that the three cannot drift —
//! a record written with one framing and read with another is a record that
//! decodes into something nobody wrote.

use kusanagi_kernel::Reader;

use crate::error::SiteError;

/// Writes a length-prefixed block.
///
/// The length is `u16`, which caps a locator and a grant at 64 KiB each. A grant
/// is bounded by its hop limit and a locator is a URL, so the cap is unreachable;
/// saturating rather than wrapping is what keeps an unreachable case from
/// becoming a silently truncated one.
pub(crate) fn put_block(out: &mut Vec<u8>, block: &[u8]) {
    let len = u16::try_from(block.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(block);
}

pub(crate) fn take_block(reader: &mut Reader<'_>) -> Result<Vec<u8>, SiteError> {
    let len = usize::from(u16::from_be_bytes(
        reader.take_array::<2>().map_err(malformed)?,
    ));
    Ok(reader.take(len).map_err(malformed)?.to_vec())
}

pub(crate) fn take_text(reader: &mut Reader<'_>, what: &'static str) -> Result<String, SiteError> {
    String::from_utf8(take_block(reader)?).map_err(|error| SiteError::BadRecord {
        what,
        reason: error.to_string(),
    })
}

pub(crate) fn malformed(error: kusanagi_kernel::Incomplete) -> SiteError {
    SiteError::BadRecord {
        what: "a stored record",
        reason: error.to_string(),
    }
}
