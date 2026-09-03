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

use kusanagi_kernel::{MAX_PAYLOAD, Segment, SegmentError, Signer, Trail, VerifyingKey};

fn alice() -> Signer {
    Signer::from_seed(&[1_u8; 32])
}

fn hers() -> VerifyingKey {
    alice().verifying_key()
}

fn trail() -> Trail {
    Trail::from_seed([9_u8; 32])
}

fn genesis() -> Segment {
    Segment::genesis(&alice(), &trail(), b"first".to_vec()).unwrap()
}

fn follower(payload: &[u8], head: kusanagi_kernel::ChainHead) -> Segment {
    Segment::extend(&trail(), alice().handle(), payload.to_vec(), head).unwrap()
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
    let second = follower(b"second", first.head());
    assert_eq!(second.index(), 1);
    assert_eq!(second.previous(), Some(first.id()));

    let decoded = Segment::from_canonical_bytes(&second.to_canonical_bytes(), &hers()).unwrap();
    assert_eq!(decoded, second);
    assert_eq!(decoded.reveal(), Some(trail().reveal(1)));
    assert_eq!(decoded.commit(), trail().commitment(2));
}

/// The envelope above a segment sees one length whichever shape it is, so the
/// two overheads have to be equal rather than merely close.
#[test]
fn both_shapes_carry_the_same_overhead() {
    let first = genesis();
    let second = follower(b"first", first.head());
    let overhead = |segment: &Segment| segment.to_canonical_bytes().len() - segment.payload().len();
    assert_eq!(overhead(&first), 141);
    assert_eq!(overhead(&first), overhead(&second));
}

/// A following segment carries no signature at all, and that is the property
/// the whole Trail exists for: there is nothing in it a coerced peer could show
/// a third party to prove who wrote it.
#[test]
fn nothing_above_genesis_is_signed() {
    let first = genesis();
    let second = follower(b"second", first.head());
    let bytes = second.to_canonical_bytes();
    let signed = first.to_canonical_bytes();
    let signature = &signed[signed.len() - 64..];
    assert!(
        !bytes.windows(64).any(|window| window == signature),
        "a following segment carried the genesis signature"
    );
    // And the only 32-byte fields it holds are a link, a name, a proof and a
    // promise: 1 + 8 + 32 + 32 + 32 + 32 + 4 = 141.
    assert_eq!(bytes.len() - second.payload().len(), 141);
}

/// A genesis segment fixes what height one must show, so a chain cannot be
/// continued by anybody who did not derive the same trail.
#[test]
fn a_genesis_commits_to_the_height_above_it() {
    assert_eq!(genesis().commit(), trail().commitment(1));
    assert_eq!(genesis().reveal(), None);
    assert_eq!(
        follower(b"x", genesis().head()).reveal(),
        Some(trail().reveal(1))
    );
}

#[test]
fn identity_follows_every_field() {
    let base = genesis();
    let other_author =
        Segment::genesis(&Signer::from_seed(&[2; 32]), &trail(), b"first".to_vec()).unwrap();
    let other_payload = Segment::genesis(&alice(), &trail(), b"second".to_vec()).unwrap();
    let higher = follower(b"first", base.head());

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
        Segment::genesis(&alice(), &trail(), payload),
        Err(SegmentError::PayloadTooLarge { .. })
    ));
}

/// Every byte of a genesis segment except its payload is under the signature or
/// under the shape, so flipping any of them refuses.
///
/// The payload is the exception, it is deliberate, and it is the whole of what
/// makes a stream deniable rather than evidential: a signature over what was
/// said would be transferable proof of what was said. The exception is asserted
/// here rather than left as an absence, so that a build which starts signing the
/// payload turns this red instead of quietly making every conversation
/// admissible. `kusanagi_chain`'s `deniable.rs` is where the consequence is
/// exercised end to end.
#[test]
fn every_flipped_byte_outside_the_payload_breaks_a_genesis_segment() {
    let segment = genesis();
    let canonical = segment.to_canonical_bytes();
    let payload_at = 77;
    let payload_ends = payload_at + segment.payload().len();

    for at in 0..canonical.len() {
        let mut tampered = canonical.clone();
        tampered[at] ^= 0x01;
        if tampered == canonical {
            continue;
        }
        let decoded = Segment::from_canonical_bytes(&tampered, &hers());
        if (payload_at..payload_ends).contains(&at) {
            assert!(
                decoded.is_ok(),
                "byte {at} is inside the payload, and a payload a peer can rewrite \
                 is what stops a transcript being proof"
            );
            assert_ne!(decoded.unwrap().payload(), segment.payload());
        } else {
            assert!(
                decoded.is_err(),
                "flipping byte {at} produced a segment that still decoded"
            );
        }
    }
}

/// A following segment carries no signature, so nothing in it is refused by
/// cryptography alone — the chain refuses it instead.
///
/// This is the shape of the trade the Trail makes, stated where somebody
/// reviewing the decoder will meet it: `from_canonical_bytes` on a follower is a
/// parser, and belief happens one layer up.
#[test]
fn a_follower_is_parsed_here_and_believed_in_the_chain() {
    let second = follower(b"second", genesis().head());
    let mut tampered = second.to_canonical_bytes();
    let payload_at = 141;
    tampered[payload_at] ^= 0x01;
    let decoded = Segment::from_canonical_bytes(&tampered, &hers())
        .expect("a follower carries no signature to break");
    assert_ne!(decoded.payload(), second.payload());
    assert_eq!(decoded.reveal(), second.reveal());
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
