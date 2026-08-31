// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! Who wrote a segment.

use crate::digest::identifier;

/// Domain separation for stage-0 handle derivation.
///
/// This prefix disappears entirely at stage 2, when a handle becomes a public key
/// rather than the hash of a name. The width does not change, so the wire format
/// does not change either.
const HANDLE_DOMAIN: &[u8] = b"kusanagi.handle.v1";

identifier! {
    /// The identity that authored a segment.
    ///
    /// At stage 0 a handle is derived from a name and carries **no authority**: it
    /// names a writer, it does not prove one. Stage 2 replaces the derivation with
    /// a public key of the same width.
    Handle, 32
}

impl Handle {
    /// Derives a handle from a name.
    #[must_use]
    pub fn from_name(name: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(HANDLE_DOMAIN);
        hasher.update(name.as_bytes());
        Self::from_bytes(*hasher.finalize().as_bytes())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::Handle;
    use core::str::FromStr;

    #[test]
    fn derivation_is_deterministic() {
        assert_eq!(Handle::from_name("alice"), Handle::from_name("alice"));
    }

    #[test]
    fn different_names_are_different_handles() {
        assert_ne!(Handle::from_name("alice"), Handle::from_name("bob"));
    }

    #[test]
    fn survives_a_text_round_trip() {
        let handle = Handle::from_name("alice");
        assert_eq!(Handle::from_str(&handle.to_string()).unwrap(), handle);
    }
}
