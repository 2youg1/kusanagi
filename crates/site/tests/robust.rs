// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What the two decoders here do with bytes they did not write.
//!
//! `Invite::parse` reads a line somebody was handed and pasted, which is the
//! only text in this program that arrives from outside and is not sealed.
//! `Channel::from_bytes` reads a file off a disk that a backup tool, a sync
//! client or a second process may have touched.
//!
//! Both carry length-prefixed blocks — a name and a locator — and a length
//! prefix read out of untrusted bytes is the classic way to be told how much
//! memory to allocate. That is the property with teeth here.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::let_underscore_must_use,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_kernel::{Handle, Signer};
use kusanagi_seal::Secret;
use kusanagi_site::{Channel, Invite, Peer, Standing};

/// A standing of `Root` on the wire: a tag and an empty length-prefixed block.
const ROOT_STANDING: usize = 1 + 2;

/// Where the peer's key sits in `bytes`, present or not.
///
/// The block is written whether or not there is a peer, so that **the size of a
/// record does not say whether the invitation was ever taken**. When there is
/// no peer it is zeroes that nothing reads.
fn peer_key(bytes: &[u8]) -> std::ops::Range<usize> {
    let end = bytes.len() - ROOT_STANDING;
    end - kusanagi_kernel::VerifyingKey::WIDTH..end
}
use proptest::prelude::*;

/// One channel record, made without touching a signature scheme twice.
fn channel() -> Channel {
    Channel {
        name: "peer".to_owned(),
        secret: Secret::from_bytes([7; 32]),
        root: Handle::from_bytes([3; 32]),
        introduction: Signer::from_seed(&[2; 32]).verifying_key(),
        locator: "http://box.example:8963".to_owned(),
        standing: Standing::Root,
        peer: None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Any bytes at all decode to an answer, never to a crash.
    #[test]
    fn any_record_bytes_produce_an_answer(bytes in prop::collection::vec(any::<u8>(), 0..400)) {
        let _ = Channel::from_bytes(&bytes);
    }

    /// Any text at all parses to an answer, never to a crash.
    #[test]
    fn any_text_at_all_produces_an_answer(text in ".{0,200}") {
        prop_assert!(Invite::parse(&text).is_err(), "arbitrary text parsed as an invitation");
    }

    /// Hexadecimal behind the right prefix is still refused, and refused fast.
    ///
    /// This is the shape an attacker sends: the prefix costs nothing to copy,
    /// and everything after it is a decoder's problem.
    #[test]
    fn the_prefix_buys_nothing(hex in "[0-9a-f]{0,300}") {
        let offered = format!("kusanagi2:{hex}");
        prop_assert!(Invite::parse(&offered).is_err());
    }

    /// A block length is compared against what arrived before it is allocated.
    ///
    /// The record begins with a version byte and then the name as a length-
    /// prefixed block. A record claiming a name of four gigabytes is a handful
    /// of bytes to write into a site directory.
    #[test]
    fn a_declared_block_is_never_believed(declared in 1_u32..=u32::MAX) {
        let whole = channel().to_bytes();
        let mut bytes = vec![whole[0]];
        bytes.extend_from_slice(&declared.to_be_bytes());
        prop_assert!(
            Channel::from_bytes(&bytes).is_err(),
            "a name length of {declared} with nothing behind it was accepted"
        );
    }
}

#[test]
fn one_spelling_per_record() {
    let bytes = channel().to_bytes();
    assert_eq!(Channel::from_bytes(&bytes).unwrap().to_bytes(), bytes);

    let mut padded = bytes.clone();
    padded.push(0);
    assert!(
        Channel::from_bytes(&padded).is_err(),
        "trailing bytes gave a record a second spelling"
    );
}

#[test]
fn every_byte_the_decoder_reads_matters() {
    let bytes = channel().to_bytes();
    // Everything except the peer's key, which is the one block this record
    // carries and does not read when there is no peer.
    let placeholder = peer_key(&bytes);
    let looked_at = (0..placeholder.start).chain(placeholder.end..bytes.len());
    for index in looked_at {
        let mut damaged = bytes.clone();
        damaged[index] ^= 0b0001_0000;
        match Channel::from_bytes(&damaged) {
            Err(_) => {}
            // A changed byte that still decodes must decode to something else.
            // The opaque fields — the secret, the root handle — are exactly
            // that, and what catches a changed one is the address it derives
            // failing to hold anything.
            Ok(read) => assert_ne!(read.to_bytes(), bytes, "byte {index} changed nothing"),
        }
    }
}

#[test]
fn a_record_is_the_same_size_whether_or_not_anybody_has_joined() {
    let alone = channel();
    let met = Channel {
        peer: Some(Peer {
            key: Signer::from_seed(&[8; 32]).verifying_key(),
            standing: Standing::Root,
        }),
        ..channel()
    };
    assert_eq!(
        alone.to_bytes().len(),
        met.to_bytes().len(),
        "the length of a channel record says whether the invitation was taken"
    );

    // The other half of that: what fills the space when nobody has joined is
    // never read, so it cannot be made to mean anything either.
    let bytes = alone.to_bytes();
    for index in peer_key(&bytes) {
        let mut different = bytes.clone();
        different[index] ^= 0b0001_0000;
        let read = Channel::from_bytes(&different)
            .expect("the placeholder is not read, so nothing in it can be malformed");
        assert_eq!(
            read.to_bytes(),
            bytes,
            "byte {index} of the placeholder reached the record"
        );
    }
}
