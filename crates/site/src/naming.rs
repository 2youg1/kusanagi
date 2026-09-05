// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a channel may be called here, and what its file is called instead.
//!
//! Two rules that look like one. [`check`] says which names a person may take,
//! and [`filed`] says what appears on the disk when they take one — and the
//! answer to the second is never the first.

use kusanagi_kernel::{Handle, Hex, Ward};

use crate::error::SiteError;

/// The longest a channel name may be.
const MAX_NAME: usize = 32;

/// Separates the file-naming key from every other secret derived from the seed.
///
/// BLAKE3's own convention: a context string names one purpose, globally and
/// forever, so two derivations from one seed can never collide.
const FILING: &str = "kusanagi 2026 channel file name v1";
const AUTHOR_FILING: &str = "kusanagi 2026 author file name v1";
const SWEEP_FILING: &str = "kusanagi 2026 sweep file name v2";

/// Refuses anything that is not plainly a name.
///
/// The rule is deliberately narrower than any filesystem's: a name that is safe
/// in a path, safe in a shell, and safe in a URL is safe everywhere this network
/// might carry it, and the ways of getting escaping wrong all start with allowing
/// something interesting.
///
/// A name may not begin with `-`. Every command line ever written reads a
/// leading hyphen as a flag, and this program reads a bare `-` as "the name
/// arrives on stdin" — so a name that starts with one is a name nobody can type.
///
/// # Errors
///
/// [`SiteError::BadName`] with the rule it broke, in the words a person can act
/// on.
pub(crate) fn check(name: &str) -> Result<(), SiteError> {
    let plain = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
    let usable = name.len() <= MAX_NAME
        && name.bytes().all(plain)
        && name.bytes().next().is_some_and(|first| first != b'-');
    if usable {
        return Ok(());
    }
    Err(SiteError::BadName {
        name: name.to_owned(),
        reason: format!(
            "a name is 1 to {MAX_NAME} characters of a-z, 0-9 and -, and does not start with -"
        ),
    })
}

/// What this site calls the file that holds the channel named `name`.
///
/// **Keyed, not plain.** A plain hash of a name is a name to anybody willing to
/// hash a list of first names, and that list is short. The key comes from this
/// endpoint's identity seed, so the same name at two endpoints is filed under
/// two different strings and no table computed once works anywhere.
///
/// 64 hexadecimal characters, the same shape a handle renders as, so nothing in
/// the directory carries a length or an alphabet that distinguishes one entry
/// from another.
pub(crate) fn filed(seed: &[u8; 32], name: &str) -> String {
    let key = blake3::derive_key(FILING, seed);
    Hex(blake3::keyed_hash(&key, name.as_bytes()).as_bytes()).to_string()
}

/// What this site calls the file that holds one author's cairn or ratchet on
/// the channel filed as `filed`.
///
/// A handle is a public key's hash, so a file named after one is a file named
/// after a person: two channels with one peer would share a file name, and so
/// would two seized disks. Keyed under this endpoint's seed and mixed with the
/// channel's filed name, the same author leaves a different name on every
/// channel and every site, and a listing gives up a count and nothing else.
pub(crate) fn filed_author(seed: &[u8; 32], filed: &str, author: &Handle) -> String {
    within(AUTHOR_FILING, seed, filed, author.as_bytes())
}

/// What this site calls the file that holds one ward's sweep record for the
/// channel or room filed as `filed`.
///
/// A different key from [`filed_author`] on purpose: a cairn and a sweep record
/// for one stream must not share a name, or whoever holds the disk can pair
/// the two and learn that two files describe one stream. No two files in a
/// site share a name; `adversary/` holds that as a property.
pub(crate) fn filed_sweep(seed: &[u8; 32], filed: &str, ward: Ward) -> String {
    within(SWEEP_FILING, seed, filed, &ward.bits().to_be_bytes())
}

fn within(context: &str, seed: &[u8; 32], filed: &str, under: &[u8]) -> String {
    let key = blake3::derive_key(context, seed);
    let mut hasher = blake3::Hasher::new_keyed(&key);
    hasher.update(filed.as_bytes());
    hasher.update(under);
    Hex(hasher.finalize().as_bytes()).to_string()
}
