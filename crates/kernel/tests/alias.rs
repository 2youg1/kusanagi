// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A name is believed only under the key that signed it for its own handle.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use kusanagi_kernel::{Alias, AliasError, Declaration, Signer};

#[test]
fn a_declaration_verifies_under_its_own_key_and_no_other() {
    let bob = Signer::from_seed(&[2; 32]);
    let mallory = Signer::from_seed(&[3; 32]);
    let declared = Declaration::sign(&bob, Alias::new("Bob").unwrap());
    let carried = Declaration::from_bytes(&declared.to_bytes()).unwrap();
    assert_eq!(
        carried.verify(&bob.verifying_key()).unwrap().as_str(),
        "Bob"
    );
    // The same bytes moved under another identity are a forgery, not a name:
    // the handle is inside what was signed.
    assert_eq!(
        carried.verify(&mallory.verifying_key()),
        Err(AliasError::Forged)
    );
}

#[test]
fn a_name_is_one_printable_line_of_at_most_thirty_two_bytes() {
    assert!(Alias::new("Bob").is_ok());
    assert!(Alias::new("鮑伯").is_ok());
    assert!(Alias::new("").is_err());
    assert!(Alias::new(" Bob").is_err());
    assert!(Alias::new("Bob\nHello").is_err());
    assert!(Alias::new("Bob\u{202E}").is_err());
    assert!(Alias::new("\u{1b}[31mBob").is_err());
    assert!(Alias::new(&"x".repeat(32)).is_ok());
    assert!(Alias::new(&"x".repeat(33)).is_err());
}

#[test]
fn a_declaration_with_a_bit_flipped_or_a_byte_added_is_refused() {
    let bob = Signer::from_seed(&[2; 32]);
    let bytes = Declaration::sign(&bob, Alias::new("Bob").unwrap()).to_bytes();
    let mut flipped = bytes.clone();
    flipped[1] ^= 0x20;
    let read = Declaration::from_bytes(&flipped).unwrap();
    assert_eq!(read.verify(&bob.verifying_key()), Err(AliasError::Forged));
    let mut longer = bytes;
    longer.push(0);
    assert_eq!(Declaration::from_bytes(&longer), Err(AliasError::Malformed));
}
