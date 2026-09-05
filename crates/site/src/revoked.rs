// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who this endpoint has cut off, and will go on refusing.
//!
//! One step identifier per line at `<root>/revoked`, in a file of its own rather
//! than inside the channel records. It has to **outlive** them: forgetting a
//! channel and joining the same name again must not bring a revoked grant back,
//! and a list stored inside the thing it revokes cannot promise that.
//!
//! Plain text because the whole file is public knowledge in the only sense that
//! matters — a revocation is a statement this endpoint makes to the world, and
//! the identifiers in it are hashes of steps whoever holds the grant already
//! has.

use std::path::Path;

use kusanagi_grant::{Revocations, StepId};

use crate::error::SiteError;
use kusanagi_vault as vault;

/// Every step revoked at this site.
///
/// # Errors
///
/// [`SiteError::Local`] when the list cannot be read, and
/// [`SiteError::BadRecord`] when a line is not a step identifier.
pub(crate) fn all(root: &Path) -> Result<Revocations, SiteError> {
    let Some(bytes) = vault::read(&root.join("revoked"), "read the revocation list")? else {
        return Ok(Revocations::new());
    };
    let text = core::str::from_utf8(&bytes).map_err(|_| SiteError::BadRecord {
        what: "the revocation list",
        reason: "this file is not text".to_owned(),
    })?;

    let mut revoked = Revocations::new();
    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        revoked = revoked.revoking(line.parse::<StepId>()?);
    }
    Ok(revoked)
}

/// Adds one step to the list, rewriting it whole.
///
/// Whole rather than appended, so that the file on the disk is always a complete
/// list: an append that is torn in half leaves a line nobody can parse, and this
/// is the one record whose failure to parse must not be recoverable by ignoring
/// it.
///
/// # Errors
///
/// [`SiteError::Local`] when the list cannot be written.
pub(crate) fn add(root: &Path, step: StepId) -> Result<(), SiteError> {
    let revoked = all(root)?.revoking(step);
    let lines: Vec<String> = revoked.iter().map(ToString::to_string).collect();
    vault::create_dir(root, "create the site directory")?;
    vault::write(
        &root.join("revoked"),
        lines.join("\n").as_bytes(),
        "write the revocation list",
    )
    .map_err(Into::into)
}
