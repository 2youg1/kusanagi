// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a message divided into several segments must do, asserted from outside.
//!
//! The wire keeps one authority for the header (`kernel::parts`), and the read
//! side keeps one for turning a run back into a message (`walk::messages`). This
//! file asserts the first through complete segments: divide, seal into a chain,
//! verify, and read the headers back off the verified bytes.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_kernel::{MAX_PAYLOAD, PART_ROOM, Part, Segment, SegmentError, Signer, divide};

fn alice() -> Signer {
    Signer::from_seed(&[1_u8; 32])
}

fn trail() -> kusanagi_kernel::Trail {
    kusanagi_kernel::Trail::from_seed([9_u8; 32])
}

/// The freight `divide` decided on, sealed into a genuine chain and verified,
/// as payloads the reader side would meet them.
fn sealed_payloads(payload: &[u8], most: u16) -> Vec<Vec<u8>> {
    let me = alice();
    let chain = trail();
    divide(payload, most)
        .unwrap()
        .into_iter()
        .scan(None, |head: &mut Option<Segment>, freight| {
            let segment = match head {
                None => Segment::genesis(&me, &chain, freight).unwrap(),
                Some(before) => {
                    Segment::extend(&chain, me.handle(), freight, before.head()).unwrap()
                }
            };
            let verified =
                Segment::from_canonical_bytes(&segment.to_canonical_bytes(), &me.verifying_key())
                    .unwrap();
            *head = Some(verified.clone());
            Some(verified)
        })
        .map(|segment| segment.payload().to_vec())
        .collect()
}

#[test]
fn what_fits_in_one_segment_is_never_divided() {
    // No header and no new purpose: a message of today keeps its exact shape.
    // The single segment is asserted through the same chain the two-part test
    // below uses.
    let brim = usize::try_from(MAX_PAYLOAD).unwrap();
    let payloads = sealed_payloads(&vec![b'm'; brim], 32);
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads.first(), Some(&vec![b'm'; brim]));
    let one = divide(&vec![b'm'; brim], 32).unwrap();
    assert_eq!(one.len(), 1);
}

#[test]
fn one_byte_over_one_segment_becomes_two_parts() {
    let brim = usize::try_from(MAX_PAYLOAD).unwrap();
    let payloads = sealed_payloads(&vec![b'm'; brim + 1], 32);
    assert_eq!(payloads.len(), 2);
    let first = Part::of(&payloads[0]).unwrap();
    let second = Part::of(&payloads[1]).unwrap();
    assert_eq!((first.index, first.total), (0, 2));
    assert_eq!((second.index, second.total), (1, 2));
    assert_eq!(first.bytes.len() + second.bytes.len(), brim + 1);
}

#[test]
fn a_header_that_is_not_a_header_is_not_a_part() {
    assert!(Part::of(&[]).is_none());
    assert!(Part::of(&[0, 0]).is_none());
    assert!(Part::of(&[0, 0, 0, 1]).is_none());
    assert!(Part::of(&[5, 5, 0, 2, b'x']).is_none());
}

#[test]
fn past_the_limit_is_refused_with_the_limit_in_it() {
    let room = usize::try_from(PART_ROOM).unwrap();
    let past = room * 2 + 1;
    match divide(&vec![b'm'; past], 2) {
        Err(SegmentError::MessageTooLarge { len, limit }) => {
            assert_eq!(len, past);
            assert_eq!(limit, room * 2);
        }
        other => panic!("a message past the limit was not refused: {other:?}"),
    }
}
