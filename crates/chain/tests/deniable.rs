// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a transcript proves, and what it does not.
//!
//! Every other test in this workspace asks whether an honest reading succeeds.
//! These ask the opposite question, the one a person asks after their peer has
//! been compromised or served with an order: **can what my peer holds be used
//! against me?**
//!
//! Three answers, and only the third is comfortable.
//!
//! * A peer holding the channel secret still cannot write at a height its author
//!   has not reached, because doing so needs a preimage of a commitment the
//!   author has not opened.
//! * A peer who has *read* a height can put any words at it afterwards, rebuild
//!   every link above it, and produce a transcript that verifies exactly as well
//!   as the real one. **A property that cannot forge has not shown deniability,
//!   it has asserted it** — so this file forges, byte by byte, and checks that
//!   the forgery passes the shipped verifier.
//! * What follows is that a quotation is an assertion by whoever quotes it. Two
//!   transcripts of one stream, both verifying, disagreeing about every word: a
//!   judge holding them has no reason to prefer either.
//!
//! The forgery is assembled from canonical bytes by hand rather than through
//! `Segment::extend`, because the shipped constructors deliberately take a
//! `Trail` and an attacker has none. Hand-assembly is the attacker's real
//! capability, so it is the one that has to be tried.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_chain::{ChainError, Verifier, verify};
use kusanagi_kernel::{Segment, Signer, Trail, VerifyingKey};

fn alice() -> Signer {
    Signer::from_seed(&[1_u8; 32])
}

fn hers() -> VerifyingKey {
    alice().verifying_key()
}

fn trail() -> Trail {
    Trail::from_seed([7_u8; 32])
}

/// A real stream: what alice actually wrote and what bob actually read.
fn transcript(words: &[&str]) -> Vec<Segment> {
    let mut said = words.iter();
    let first = said.next().expect("a stream opens with something");
    let mut chain = vec![Segment::genesis(&alice(), &trail(), first.as_bytes().to_vec()).unwrap()];
    for word in said {
        let head = chain.last().unwrap().head();
        chain.push(
            Segment::extend(&trail(), alice().handle(), word.as_bytes().to_vec(), head).unwrap(),
        );
    }
    chain
}

/// Where the payload sits in a following segment's canonical bytes.
///
/// tag 1 + index 8 + previous 32 + author 32 + reveal 32 + commit 32 + len 4.
const FOLLOWS_PAYLOAD_AT: usize = 141;

/// Where the payload sits in a genesis segment's canonical bytes.
///
/// tag 1 + index 8 + author 32 + commit 32 + len 4, and 64 bytes of signature
/// follow the payload rather than precede it.
const GENESIS_PAYLOAD_AT: usize = 77;

/// Rewrites what a segment says, leaving every field that authenticates it.
///
/// This is the forger's whole toolkit: the proof, the promise and the author are
/// copied untouched, and only the words change. Whether that is enough is the
/// question the file exists to answer.
fn reworded(segment: &Segment, words: &str) -> Vec<u8> {
    let original = segment.to_canonical_bytes();
    let at = if segment.previous().is_some() {
        FOLLOWS_PAYLOAD_AT
    } else {
        GENESIS_PAYLOAD_AT
    };
    let mut forged = original[..at - 4].to_vec();
    let payload = words.as_bytes();
    forged.extend_from_slice(&u32::try_from(payload.len()).unwrap().to_be_bytes());
    forged.extend_from_slice(payload);
    if segment.previous().is_none() {
        // The signature trails the payload and covers neither it nor its length.
        forged.extend_from_slice(&original[original.len() - 64..]);
    }
    forged
}

/// Points a forged following segment at the predecessor the forger just made.
fn pointed_at(bytes: &mut [u8], previous: &Segment) {
    bytes[9..41].copy_from_slice(previous.id().as_bytes());
}

#[test]
fn a_peer_cannot_write_at_a_height_its_author_has_not_reached() {
    // The peer holds everything a peer ever holds: the channel secret, the
    // author's key, and every segment published so far. What they do not hold is
    // the proof for the next height, and the commitment that names it is a hash.
    let real = transcript(&["one", "two"]);
    let stranger = Trail::from_seed([8_u8; 32]);
    let ahead = Segment::extend(
        &stranger,
        alice().handle(),
        b"alice never said this".to_vec(),
        real[1].head(),
    )
    .unwrap();

    // It decodes: it is well-formed and names the right author, because a name is
    // public. It does not verify, and the failure names exactly what went wrong.
    let decoded = Segment::from_canonical_bytes(&ahead.to_canonical_bytes(), &hers()).unwrap();
    let mut verifier = Verifier::new();
    verifier.accept(&real[0]).unwrap();
    verifier.accept(&real[1]).unwrap();
    assert!(matches!(
        verifier.accept(&decoded),
        Err(ChainError::ProofRefused { index: 2, .. })
    ));
}

#[test]
fn a_peer_who_read_a_stream_can_afterwards_put_any_words_in_it() {
    let real = transcript(&["i agree", "send the money", "done"]);
    assert!(verify(&real).is_ok(), "the real transcript must verify");

    // Bob rewrites all three heights and rebuilds the links between them. He
    // reuses alice's signature, her reveals and her commitments untouched — he
    // has them because she published them, and none of them covers a word.
    let genesis = Segment::from_canonical_bytes(&reworded(&real[0], "i refuse"), &hers()).unwrap();

    let mut second = reworded(&real[1], "keep the money");
    pointed_at(&mut second, &genesis);
    let second = Segment::from_canonical_bytes(&second, &hers()).unwrap();

    let mut third = reworded(&real[2], "never spoke to you");
    pointed_at(&mut third, &second);
    let third = Segment::from_canonical_bytes(&third, &hers()).unwrap();

    let forged = [genesis, second, third];
    assert!(
        verify(&forged).is_ok(),
        "a peer could not fabricate, so the stream is evidence rather than a claim"
    );

    // And the two transcripts agree about nothing a court would care about.
    for (real, forged) in real.iter().zip(forged.iter()) {
        assert_eq!(real.index(), forged.index());
        assert_eq!(real.author(), forged.author());
        assert_ne!(real.payload(), forged.payload());
    }
}

#[test]
fn two_transcripts_of_one_stream_verify_and_contradict_each_other() {
    // The consequence, stated as the thing a reader of this repository should
    // take away: holding a verifying transcript is not holding evidence, because
    // holding two verifying transcripts that disagree is equally easy.
    let real = transcript(&["yes"]);
    let forged = Segment::from_canonical_bytes(&reworded(&real[0], "no"), &hers()).unwrap();

    assert!(verify(&real).is_ok());
    assert!(verify(std::slice::from_ref(&forged)).is_ok());
    assert_eq!(real[0].author(), forged.author());
    assert_ne!(real[0].payload(), forged.payload());
}

#[test]
fn a_forger_still_cannot_open_a_stream_in_somebody_elses_name() {
    // Deniability is not forgeability of identity. Genesis is signed, and the
    // signature covers the author and the first commitment — so a peer cannot
    // invent a stream alice never opened, nor re-point an existing one at a
    // trail of their own.
    let real = transcript(&["one"]);
    let mut bytes = real[0].to_canonical_bytes();
    // Replace the commitment with one the forger controls.
    bytes[41..73].copy_from_slice(Trail::from_seed([8_u8; 32]).commitment(1).as_bytes());
    assert!(
        Segment::from_canonical_bytes(&bytes, &hers()).is_err(),
        "a forged commitment at genesis would let a peer own the whole stream above it"
    );
}
