// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What `envelope::open` does with bytes a host chose.
//!
//! This is the first code in the read path to touch a drop, and everything it is
//! handed came from somewhere that is not trusted. Two things could go wrong that
//! the type system does not stop: a length inside the padding could be believed
//! and allocated for, and a shorter-than-a-drop input could be read past.
//!
//! The unit tests beside `envelope.rs` already sample bit flips in a valid drop.
//! What is here is the other direction — inputs that were never valid at all, in
//! every length around the fixed one.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_seal::{DROP, Secret, derive, open, seal};
use proptest::prelude::*;

/// The key every case here opens under, derived the way the read path does.
fn key() -> kusanagi_seal::Key {
    let stream = Secret::from_bytes([5; 32]).stream(&kusanagi_kernel::Handle::from_bytes([6; 32]));
    derive(&stream, 0).1
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    /// Anything at all opens to an answer, never to a crash.
    ///
    /// Lengths straddle `DROP`: one short and one long are where a fixed-width
    /// reader is wrong, and a host chooses the length. Sixty-four cases rather
    /// than the usual figure, because each one generates a drop-sized vector.
    #[test]
    fn any_bytes_at_all_produce_an_answer(
        bytes in prop::collection::vec(any::<u8>(), 0..DROP + 8)
    ) {
        prop_assert!(open(&key(), &bytes).is_err(), "random bytes opened");
    }

    /// A drop of the right length made of the wrong bytes is still refused.
    #[test]
    fn a_drop_shaped_thing_is_not_a_drop(fill in any::<u8>()) {
        prop_assert!(open(&key(), &vec![fill; DROP]).is_err());
    }
}

#[test]
fn what_was_sealed_is_what_comes_back() {
    let said = b"the payload this drop carries".to_vec();
    let sealed = seal(&key(), &said).unwrap();
    assert_eq!(sealed.len(), DROP);
    assert_eq!(open(&key(), &sealed).unwrap(), said);
}

#[test]
fn a_drop_one_byte_from_the_right_length_is_refused() {
    let sealed = seal(&key(), b"anything").unwrap();
    assert!(open(&key(), &sealed[..DROP - 1]).is_err());
    let mut longer = sealed.clone();
    longer.push(0);
    assert!(open(&key(), &longer).is_err());
}
