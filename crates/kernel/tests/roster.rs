// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A room's roster is believed only under the founder's key.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use kusanagi_kernel::{Roster, RosterError, Signer};

fn founders() -> (Signer, Signer, Signer) {
    (
        Signer::from_seed(&[7; 32]),
        Signer::from_seed(&[8; 32]),
        Signer::from_seed(&[9; 32]),
    )
}

#[test]
fn a_roster_verifies_under_the_founders_key_and_no_other() {
    let (alice, bob, carol) = founders();
    let keys = vec![bob.verifying_key(), carol.verifying_key()];
    let roster = Roster::sign(&alice, keys.clone()).unwrap();
    let carried = Roster::from_bytes(&roster.to_bytes().unwrap()).unwrap();
    assert_eq!(
        carried.verify(&alice.verifying_key()).unwrap(),
        keys.as_slice()
    );
    // The same bytes moved under another founder are a forgery, not a roster.
    let mallory = Signer::from_seed(&[10; 32]);
    assert_eq!(
        carried.verify(&mallory.verifying_key()),
        Err(RosterError::Forged)
    );
}

#[test]
fn a_roster_with_a_member_swapped_or_a_byte_added_is_refused() {
    let (alice, bob, carol) = founders();
    let roster = Roster::sign(&alice, vec![bob.verifying_key(), carol.verifying_key()]).unwrap();
    let bytes = roster.to_bytes().unwrap();
    // One member swapped: the signature no longer covers the list.
    let swapped = Roster::sign(&alice, vec![carol.verifying_key(), bob.verifying_key()]).unwrap();
    assert_ne!(swapped.to_bytes().unwrap(), bytes);
    assert!(Roster::from_bytes(&swapped.to_bytes().unwrap()).is_ok());
    // A byte added past the end is damage, not a longer roster.
    let mut longer = bytes;
    longer.push(0);
    assert_eq!(Roster::from_bytes(&longer), Err(RosterError::Malformed));
}
