// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a decoder does with bytes nobody here wrote.
//!
//! `Segment::from_canonical_bytes` is fed by a host, and a host is not trusted.
//! The lints already forbid panicking constructs; what they cannot forbid is
//! **allocating on a declared length** — a four-byte field saying "the payload is
//! four gigabytes" costs four bytes to send and four gigabytes to believe — or a
//! decoder that accepts two spellings of one segment.
//!
//! The third property draws a line that matters more than it looks: a flipped
//! bit in the header or the signature is refused here, and **a flipped bit in the
//! payload is not**. The signature deliberately does not cover the payload,
//! because a signature over what was said is transferable proof of what was said.
//! What protects the payload is the sealed envelope it arrives in
//! (`seal::envelope`), and this test is where that division is written down in
//! runnable form rather than in prose.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::let_underscore_must_use,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use std::sync::LazyLock;

use kusanagi_kernel::{Segment, Signer, Trail, VerifyingKey};
use proptest::prelude::*;

/// Everything a genesis segment carries before its payload: tag, height,
/// author, commitment, and the declared payload length.
const HEADER: usize = 1 + 8 + 32 + 32 + 4;

/// What this endpoint says in the test, so the payload's length is known.
const SAID: &[u8] = b"a payload of ordinary size";

/// One valid genesis segment, encoded, and the key that signed it.
///
/// Built once. ML-DSA-87 key generation and signing cost tens of milliseconds
/// each, and a property that pays them per case is timing the signature scheme
/// rather than testing the decoder.
static GENESIS: LazyLock<(Vec<u8>, VerifyingKey)> = LazyLock::new(|| {
    let signer = Signer::from_seed(&[9; 32]);
    let trail = Trail::from_seed([4; 32]);
    let segment = Segment::genesis(&signer, &trail, SAID.to_vec()).unwrap();
    (segment.to_canonical_bytes(), signer.verifying_key())
});

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Anything at all decodes to an answer, never to a crash.
    #[test]
    fn any_bytes_at_all_produce_an_answer(bytes in prop::collection::vec(any::<u8>(), 0..600)) {
        let (_, author) = &*GENESIS;
        // The value is discarded: what is asserted is that the call returns.
        let _ = Segment::from_canonical_bytes(&bytes, author);
    }

    /// A payload length is compared against what arrived before anything is
    /// allocated for it.
    ///
    /// The declared length is the last field before the payload, so a valid
    /// header followed by a large number and nothing else is the whole attack.
    /// It must be refused in the time it takes to compare two integers.
    #[test]
    fn a_declared_length_is_never_believed(declared in 1_u32..=u32::MAX) {
        let (whole, author) = &*GENESIS;
        let mut bytes = whole[..HEADER - 4].to_vec();
        bytes.extend_from_slice(&declared.to_be_bytes());
        let error = Segment::from_canonical_bytes(&bytes, author)
            .expect_err("a length nothing backs was accepted");
        prop_assert!(
            matches!(error.code(), "segment.truncated" | "segment.payload_too_large"),
            "a declared length of {declared} was answered with {}",
            error.code()
        );
    }
}

#[test]
fn one_spelling_per_segment() {
    let (bytes, author) = &*GENESIS;
    let again = Segment::from_canonical_bytes(bytes, author).unwrap();
    assert_eq!(&again.to_canonical_bytes(), bytes);

    // Trailing bytes are a second spelling of the same segment, so they are
    // refused rather than ignored.
    let mut padded = bytes.clone();
    padded.push(0);
    assert!(Segment::from_canonical_bytes(&padded, author).is_err());
}

#[test]
fn a_flipped_bit_in_what_is_signed_is_refused_and_one_in_the_payload_is_not() {
    let (bytes, author) = &*GENESIS;
    let payload = HEADER..HEADER + SAID.len();

    // The header exhaustively; the signature sampled, because verifying a
    // lattice signature four thousand times over would buy nothing the first
    // hundred did not already say.
    let sampled = (payload.end..bytes.len()).step_by(64);
    for index in (0..HEADER).chain(sampled) {
        let mut damaged = bytes.clone();
        damaged[index] ^= 0b0001_0000;
        assert!(
            Segment::from_canonical_bytes(&damaged, author).is_err(),
            "a bit flipped at byte {index}, inside what the author signed, was accepted"
        );
    }

    // And the other half of the boundary, stated rather than assumed: this layer
    // does not authenticate the payload, and a reader that skipped the envelope
    // would not notice a changed one. `seal::envelope` is what does.
    let mut altered = bytes.clone();
    altered[payload.start] ^= 0b0001_0000;
    let read = Segment::from_canonical_bytes(&altered, author)
        .expect("the payload is outside the signature, by design");
    assert_ne!(read.payload(), SAID);
}
