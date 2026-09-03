// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What `Cairn::from_bytes` does with a file it did not write.
//!
//! A cairn is this endpoint's note of where it got to, and it is read back off a
//! disk that a backup tool, a sync client or a second process may have touched.
//! It is fixed width, which is the easiest shape to decode safely and the easiest
//! to decode carelessly: every field here is split off with a checked length, and
//! this is where that is asserted rather than assumed.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::let_underscore_must_use,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_chain::Cairn;
use proptest::prelude::*;

/// A cairn as it appears on a disk: version, author, id, height, commitment.
///
/// Written out here rather than produced by a walk, because what is under test
/// is the decoder and a walk would drag a signature scheme in to reach it.
fn recorded() -> Vec<u8> {
    let mut bytes = vec![1_u8];
    bytes.extend_from_slice(&[3; 32]);
    bytes.extend_from_slice(&[7; 32]);
    bytes.extend_from_slice(&12_u64.to_be_bytes());
    bytes.extend_from_slice(&[9; 32]);
    assert_eq!(bytes.len(), Cairn::WIDTH);
    bytes
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    /// Anything at all decodes to an answer, never to a crash.
    ///
    /// The range spans both sides of the exact width, because "one byte short"
    /// and "one byte long" are where a fixed-width decoder goes wrong.
    #[test]
    fn any_bytes_at_all_produce_an_answer(bytes in prop::collection::vec(any::<u8>(), 0..200)) {
        let _ = Cairn::from_bytes(&bytes);
    }

    /// No length but the exact one is accepted.
    #[test]
    fn only_the_declared_width_is_read(extra in 1_usize..64) {
        let mut bytes = recorded();
        let width = bytes.len();
        bytes.extend(std::iter::repeat_n(0_u8, extra));
        prop_assert!(Cairn::from_bytes(&bytes).is_err(), "{extra} trailing bytes were ignored");
        prop_assert!(Cairn::from_bytes(&bytes[..width - 1]).is_err(), "a short cairn was read");
    }
}

#[test]
fn one_spelling_per_cairn() {
    let bytes = recorded();
    assert_eq!(Cairn::from_bytes(&bytes).unwrap().to_bytes(), bytes);
}

#[test]
fn every_byte_of_a_cairn_matters() {
    let bytes = recorded();
    for index in 0..bytes.len() {
        let mut damaged = bytes.clone();
        damaged[index] ^= 0b0001_0000;
        match Cairn::from_bytes(&damaged) {
            // The version byte is the one that must be refused outright: a
            // record this build does not understand is not a record to guess at.
            Err(_) => assert_eq!(index, 0, "byte {index} was refused, which was not expected"),
            // Every other byte is opaque content, and a changed one produces a
            // different cairn rather than an error. What catches that is the
            // walk it is used for: a head this endpoint never reached refuses to
            // extend, which `deniable.rs` and `resuming.rs` assert end to end.
            Ok(read) => assert_ne!(read.to_bytes(), bytes, "byte {index} changed nothing"),
        }
    }
}
