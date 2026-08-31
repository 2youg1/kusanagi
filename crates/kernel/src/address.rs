// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! Where a segment is left for its reader.

use crate::digest::identifier;
use crate::handle::Handle;

/// Domain separation for the stage-0 public derivation.
///
/// The `v0` is deliberate and is not a format version. It marks a path that will
/// be **deleted**, not upgraded: stage 1 derives addresses from a shared secret,
/// and this function goes with it.
const DROP_DOMAIN_V0: &[u8] = b"kusanagi.drop.v0";

identifier! {
    /// An opaque address that receives exactly one segment.
    ///
    /// 160 bits: wide enough that two independently derived addresses do not
    /// collide, narrow enough that the text form is 40 characters, which is a
    /// comfortable object-storage key.
    DropAddr, 20
}

/// Derives a drop address that anyone can compute.
///
/// **This is deliberately linkable.** Anyone who knows a handle can enumerate
/// every address that handle will ever write to, which is precisely what stage 1
/// removes by deriving from a shared secret instead. It exists so that stage 0
/// can carry a segment end to end without any cryptography at all, and it is
/// deleted — not upgraded — when `seal` lands.
#[must_use]
pub fn public_v0(author: &Handle, index: u64) -> DropAddr {
    let mut hasher = blake3::Hasher::new();
    hasher.update(DROP_DOMAIN_V0);
    hasher.update(author.as_bytes());
    hasher.update(&index.to_be_bytes());

    // An extendable-output read, not a truncated 32-byte hash: asking BLAKE3 for
    // exactly the width we need is what the construction is for.
    let mut narrowed = [0_u8; 20];
    hasher.finalize_xof().fill(&mut narrowed);
    DropAddr::from_bytes(narrowed)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{DropAddr, public_v0};
    use crate::handle::Handle;
    use core::str::FromStr;

    #[test]
    fn derivation_is_deterministic() {
        let alice = Handle::from_name("alice");
        assert_eq!(public_v0(&alice, 0), public_v0(&alice, 0));
    }

    #[test]
    fn each_height_gets_its_own_address() {
        let alice = Handle::from_name("alice");
        assert_ne!(public_v0(&alice, 0), public_v0(&alice, 1));
    }

    #[test]
    fn each_author_gets_its_own_address() {
        let alice = Handle::from_name("alice");
        let bob = Handle::from_name("bob");
        assert_ne!(public_v0(&alice, 0), public_v0(&bob, 0));
    }

    #[test]
    fn survives_a_text_round_trip() {
        let addr = public_v0(&Handle::from_name("alice"), 3);
        assert_eq!(DropAddr::from_str(&addr.to_string()).unwrap(), addr);
        assert_eq!(addr.to_string().len(), 40);
    }
}
