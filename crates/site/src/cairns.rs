// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How far one author's stream on one channel has been verified.
//!
//! A file of its own because the failure policy here is the opposite of the one
//! every other record on this disk follows. A cairn is the only thing a site
//! keeps that can be recomputed — walking the stream from height zero rebuilds
//! it exactly — so **every way of failing to read one is reported as not having
//! one**, while failing to write one is reported as a failure.
//!
//! What the read side gives up is a signal: an endpoint whose cairns are being
//! deleted walks from genesis every time and does not complain. It is given up
//! because refusing would not buy it back — whoever can corrupt a cairn can
//! delete it, and a deleted cairn is indistinguishable from a channel that has
//! never been read. Losing every cairn costs requests and privacy, never
//! correctness.
//!
//! What the write side refuses to give up is the operator's chance to learn.
//! A disk that will not take a cairn makes every later read pay a full walk, and
//! staying quiet about it would leave nothing to explain why.

use std::path::{Path, PathBuf};

use kusanagi_chain::Cairn;

use crate::error::SiteError;
use kusanagi_vault as vault;

/// Where one channel's cairns sit, under the same filed name as its record.
pub(crate) fn dir(root: &Path, filed: &str) -> PathBuf {
    root.join("cairns").join(filed)
}

/// What one author's stream on this channel has been verified to, if anything.
///
/// `filed_author` is [`crate::naming::filed_author`]'s rule: 64 hexadecimal
/// characters that need no checking against [`crate::naming::check`], cannot
/// escape a directory, and name nobody.
pub(crate) fn read(root: &Path, filed: &str, filed_author: &str) -> Option<Cairn> {
    vault::read(&dir(root, filed).join(filed_author), "read a cairn")
        .ok()
        .flatten()
        .and_then(|bytes| Cairn::from_bytes(&bytes).ok())
}

/// Writes down how far `cairn`'s author has been verified on this channel.
///
/// `filed_author` is derived by the caller from the author inside the cairn,
/// so a record cannot end up describing a stream other than the one it is
/// filed under.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be written.
pub(crate) fn write(
    root: &Path,
    filed: &str,
    filed_author: &str,
    cairn: &Cairn,
) -> Result<(), SiteError> {
    let directory = dir(root, filed);
    vault::create_dir(&directory, "create the cairn directory")?;
    vault::write(
        &directory.join(filed_author),
        &cairn.to_bytes(),
        "write a cairn",
    )
    .map_err(Into::into)
}
