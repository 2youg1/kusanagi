// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What the canonical bytes of a segment must do, asserted from outside.
//!
//! These assertions lived beside the code until `segment.rs` reached the
//! 500-line limit in `ARCHITECTURE.md` §5. Every one of them was already written
//! against the public interface, so nothing was weakened by the move: a segment
//! is still built only by signing and read only by verifying, and that is what
//! these check byte by byte.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_kernel::{MAX_PAYLOAD, Segment, SegmentError, Signer, VerifyingKey};

fn alice() -> Signer {
    Signer::from_seed(&[1_u8; 32])
}

fn hers() -> VerifyingKey {
    alice().verifying_key()
}

fn genesis() -> Segment {
    Segment::genesis(&alice(), b"first".to_vec()).unwrap()
}

#[test]
fn canonical_bytes_are_stable() {
    let segment = genesis();
    assert_eq!(segment.to_canonical_bytes(), segment.to_canonical_bytes());
}

#[test]
fn genesis_round_trips() {
    let segment = genesis();
    let decoded = Segment::from_canonical_bytes(&segment.to_canonical_bytes(), &hers()).unwrap();
    assert_eq!(decoded, segment);
    assert_eq!(decoded.id(), segment.id());
    assert_eq!(decoded.author(), alice().handle());
}

#[test]
fn extend_round_trips_and_links() {
    let first = genesis();
    let second = Segment::extend(&alice(), b"second".to_vec(), first.head()).unwrap();
    assert_eq!(second.index(), 1);
    assert_eq!(second.previous(), Some(first.id()));

    let decoded = Segment::from_canonical_bytes(&second.to_canonical_bytes(), &hers()).unwrap();
    assert_eq!(decoded, second);
}

#[test]
fn identity_follows_every_field() {
    let base = genesis();
    let other_author = Segment::genesis(&Signer::from_seed(&[2; 32]), b"first".to_vec()).unwrap();
    let other_payload = Segment::genesis(&alice(), b"second".to_vec()).unwrap();
    let higher = Segment::extend(&alice(), b"first".to_vec(), base.head()).unwrap();

    assert_ne!(base.id(), other_author.id());
    assert_ne!(base.id(), other_payload.id());
    assert_ne!(base.id(), higher.id());
}

#[test]
fn empty_input_is_truncated() {
    assert!(matches!(
        Segment::from_canonical_bytes(&[], &hers()),
        Err(SegmentError::Truncated(_))
    ));
}

#[test]
fn tag_only_is_truncated() {
    assert!(matches!(
        Segment::from_canonical_bytes(&[0], &hers()),
        Err(SegmentError::Truncated(_))
    ));
}

#[test]
fn unknown_tag_is_named() {
    let mut bytes = genesis().to_canonical_bytes();
    bytes[0] = 2;
    assert_eq!(
        Segment::from_canonical_bytes(&bytes, &hers()),
        Err(SegmentError::UnknownTag { tag: 2 })
    );
}

#[test]
fn genesis_with_a_height_is_refused() {
    let mut bytes = genesis().to_canonical_bytes();
    bytes[8] = 7;
    assert_eq!(
        Segment::from_canonical_bytes(&bytes, &hers()),
        Err(SegmentError::GenesisIndexNotZero { index: 7 })
    );
}

#[test]
fn trailing_bytes_are_refused() {
    let mut bytes = genesis().to_canonical_bytes();
    bytes.push(0);
    assert_eq!(
        Segment::from_canonical_bytes(&bytes, &hers()),
        Err(SegmentError::TrailingBytes { count: 1 })
    );
}

#[test]
fn a_lying_length_is_truncated_not_panicking() {
    let mut bytes = genesis().to_canonical_bytes();
    let length_at = bytes.len() - 64 - 5 - 4;
    bytes[length_at..length_at + 4].copy_from_slice(&1000_u32.to_be_bytes());
    assert!(matches!(
        Segment::from_canonical_bytes(&bytes, &hers()),
        Err(SegmentError::Truncated(_))
    ));
}

#[test]
fn an_oversized_payload_is_refused() {
    let payload = vec![0_u8; usize::try_from(MAX_PAYLOAD).unwrap() + 1];
    assert!(matches!(
        Segment::genesis(&alice(), payload),
        Err(SegmentError::PayloadTooLarge { .. })
    ));
}

#[test]
fn every_flipped_payload_byte_breaks_the_signature() {
    let segment = genesis();
    let canonical = segment.to_canonical_bytes();
    for at in 0..canonical.len() {
        let mut tampered = canonical.clone();
        tampered[at] ^= 0x01;
        if tampered == canonical {
            continue;
        }
        assert!(
            Segment::from_canonical_bytes(&tampered, &hers()).is_err(),
            "flipping byte {at} produced a segment that still decoded"
        );
    }
}

#[test]
fn a_segment_cannot_be_re_authored() {
    // Take alice's segment, relabel the author as bob, and offer bob's key so
    // that the name matches: the signature no longer covers the body, so the
    // bytes stop being a segment at all.
    let bob = Signer::from_seed(&[2; 32]);
    let mut bytes = genesis().to_canonical_bytes();
    let author_at = 9;
    bytes[author_at..author_at + 32].copy_from_slice(bob.handle().as_bytes());
    assert!(matches!(
        Segment::from_canonical_bytes(&bytes, &bob.verifying_key()),
        Err(SegmentError::NotAuthentic(_))
    ));
}

#[test]
fn a_segment_read_under_the_wrong_key_names_both_parties() {
    // A host serving a drop from a stream nobody asked for produces this, and it
    // is not a forgery: the bytes are genuine and belong to somebody else. The
    // two failures stay apart because they call for different actions.
    let bob = Signer::from_seed(&[2; 32]);
    assert_eq!(
        Segment::from_canonical_bytes(&genesis().to_canonical_bytes(), &bob.verifying_key()),
        Err(SegmentError::NotTheAuthor {
            expected: bob.handle(),
            found: alice().handle(),
        })
    );
}

#[test]
fn the_author_field_is_a_name_that_no_key_fits_inside() {
    // Load-bearing for every width in the format: the segment carries 32 bytes
    // of author whatever the signature scheme is, because what it carries is a
    // hash. A build that put the key here would widen every segment the day the
    // scheme changed.
    let bytes = genesis().to_canonical_bytes();
    let author_at = 9;
    assert_eq!(
        &bytes[author_at..author_at + 32],
        alice().handle().as_bytes()
    );
    assert_ne!(
        &bytes[author_at..author_at + 32],
        alice().verifying_key().as_bytes()
    );
}
