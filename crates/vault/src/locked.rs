// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Bytes that stay in physical memory until they are erased.

use zeroize::Zeroize as _;

use crate::platform;

/// Bytes that stay in physical memory until they are erased.
///
/// Borrowed as a slice everywhere, so a caller reads it exactly as it would read
/// a `Vec<u8>` and cannot accidentally take a copy that outlives the pin. What a
/// caller *decodes* out of it — a channel secret, an expanded key — lives in
/// ordinary pages and erases itself instead; `vault-SPEC.md` §3 records that
/// boundary rather than leaving it to be discovered.
#[derive(Debug)]
pub struct Locked {
    bytes: Vec<u8>,
}

impl Locked {
    /// Pins `bytes` for as long as this value lives.
    pub(crate) fn holding(bytes: Vec<u8>) -> Self {
        platform::lock(&bytes);
        Self { bytes }
    }
}

impl Drop for Locked {
    /// Erases first, unpins second. The other order would let the operating
    /// system evict the page between the two, which is the whole thing being
    /// prevented.
    fn drop(&mut self) {
        self.bytes.as_mut_slice().zeroize();
        platform::unlock(&self.bytes);
    }
}

impl core::ops::Deref for Locked {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}
