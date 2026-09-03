// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What somebody who has the disk gets.
//!
//! File permissions stop another account on a running machine. They stop nothing
//! at all once the disk is somewhere else: a backup, a folder a sync client
//! uploads, a sample submitted to a cloud scanner, a drive pulled out of a
//! laptop. `ARCHITECTURE.md` §9 called that gap on-disk deniability, and on
//! Windows this closes most of it.
//!
//! **Without adding a vendor and without this project inventing any
//! cryptography.** DPAPI derives its key from the user's logon credentials and
//! keeps it where a process running as that user can reach it and nothing else
//! can. A disk without the account's password — or the domain's backup key — is
//! noise. What it is not is protection from the account itself, from a machine
//! that is running and unlocked, or from full-disk-level forensics; that is
//! `BitLocker`'s job and the ruling in `ARCHITECTURE.md` §8 stands.
//!
//! **Every file starts with a tag that says how to open it.**
//!
//! | tag | meaning |
//! |---|---|
//! | `0x00` | the bytes follow in the clear |
//! | `0x01` | DPAPI, bound to a Windows account |
//! | `0x02`… | reserved for the next platform's store |
//!
//! The tag is a byte rather than a build assumption because **a site outlives
//! the machine it was made on**. A record this platform cannot open is refused
//! by name, with the one instruction that works: export it where it was made and
//! import it here. `archive.rs` writes plaintext records for exactly that reason,
//! so the migration path needs no code per platform pair.

use crate::error::SiteError;
#[cfg(windows)]
use crate::permissions;

/// The bytes follow in the clear.
const PLAIN: u8 = 0x00;

/// The bytes are a DPAPI blob, bound to one Windows account.
const DPAPI: u8 = 0x01;

/// Which store this build seals with.
#[cfg(windows)]
const HERE: u8 = DPAPI;
#[cfg(not(windows))]
const HERE: u8 = PLAIN;

/// What this build seals with, as one word a report can print.
///
/// Named rather than derived from the tag byte by the caller: the mapping from a
/// store to its number lives in this file, and a second reader of that byte is a
/// second place to get it wrong.
#[must_use]
pub const fn store() -> &'static str {
    match HERE {
        DPAPI => "dpapi",
        _ => "plain",
    }
}

/// Seals `plain` for this platform's disk, tag first.
///
/// # Errors
///
/// [`SiteError::Permissions`] when the platform store refuses. It is a refusal
/// rather than a fallback: writing in the clear because encryption failed would
/// silently withdraw the property this exists for.
pub(crate) fn seal_at_rest(plain: &[u8]) -> Result<Vec<u8>, SiteError> {
    let mut out = vec![HERE];
    #[cfg(windows)]
    out.extend_from_slice(&permissions::protect(plain)?);
    #[cfg(not(windows))]
    out.extend_from_slice(plain);
    Ok(out)
}

/// Opens what [`seal_at_rest`] wrote, on the platform that wrote it.
///
/// # Errors
///
/// [`SiteError::ForeignRecord`] for a tag this platform has no store for, and
/// [`SiteError::Permissions`] when the store has one and refuses \u2014 which on
/// Windows means a different account, or the same account after an
/// administrator reset its password.
pub(crate) fn open_at_rest(stored: &[u8]) -> Result<Vec<u8>, SiteError> {
    let (tag, body) = stored
        .split_first()
        .ok_or(SiteError::ForeignRecord { tag: PLAIN })?;
    match *tag {
        PLAIN => Ok(body.to_vec()),
        #[cfg(windows)]
        DPAPI => permissions::unprotect(body),
        other => Err(SiteError::ForeignRecord { tag: other }),
    }
}
