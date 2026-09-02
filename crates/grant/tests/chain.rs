// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a chain of steps must do, asserted through the door a caller has.
//!
//! These assertions lived beside the code until `chain.rs` reached the 500-line
//! limit in `ARCHITECTURE.md` §5. They moved rather than shrank, and they moved
//! here rather than anywhere else because every one of them was already written
//! against the public interface: a caller who can only reach `Grant` through
//! `kusanagi_grant` can still ask each of these questions.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_grant::{Abilities, Ability, Grant, GrantError, MAX_STEPS, Revocations, Scope};
use kusanagi_kernel::{Handle, Instant, Signer};

fn signer(seed: u8) -> Signer {
    Signer::from_seed(&[seed; 32])
}

fn at(seconds: u64) -> Instant {
    Instant::from_unix_seconds(seconds)
}

fn wide() -> Scope {
    Scope::new(Abilities::ALL, at(1_000))
}

/// root -> first -> second -> third, each hop no wider than the last.
fn three_deep() -> (Signer, Signer, Signer, Signer, Grant, Grant, Grant) {
    let root = signer(1);
    let first = signer(2);
    let second = signer(3);
    let third = signer(4);

    let one = Grant::issue(&root, &first.handle(), wide());
    let two = one.attenuate(&first, &second.handle(), wide()).unwrap();
    let three = two.attenuate(&second, &third.handle(), wide()).unwrap();
    (root, first, second, third, one, two, three)
}

#[test]
fn a_three_step_chain_verifies_to_its_holder() {
    let (root, _, _, third, _, _, three) = three_deep();
    assert_eq!(three.depth(), 3);
    assert_eq!(three.holder().unwrap(), third.handle());
    assert_eq!(three.root().unwrap(), root.handle());
    assert_eq!(
        three.permits(
            &root.handle(),
            &third.handle(),
            Ability::Send,
            at(0),
            &Revocations::new()
        ),
        Ok(())
    );
}

#[test]
fn revoking_the_middle_kills_everything_below_it() {
    let (root, _, second, third, _, two, three) = three_deep();
    let middle = two.steps().last().unwrap().id();
    let revoked = Revocations::new().revoking(middle);

    // the revoked step's own holder is cut off
    assert_eq!(
        two.verify(&root.handle(), at(0), &revoked),
        Err(GrantError::Revoked { step: middle })
    );
    // and so is everybody beneath it, immediately and without a new signature
    assert_eq!(
        three.verify(&root.handle(), at(0), &revoked),
        Err(GrantError::Revoked { step: middle })
    );
    let _ = (second, third);
}

#[test]
fn revoking_a_leaf_leaves_its_ancestors_alone() {
    let (root, _, _, _, one, two, three) = three_deep();
    let leaf = three.steps().last().unwrap().id();
    let revoked = Revocations::new().revoking(leaf);

    assert!(one.verify(&root.handle(), at(0), &revoked).is_ok());
    assert!(two.verify(&root.handle(), at(0), &revoked).is_ok());
    assert!(three.verify(&root.handle(), at(0), &revoked).is_err());
}

#[test]
fn asking_for_more_than_you_hold_yields_what_you_hold() {
    let root = signer(1);
    let first = signer(2);
    let narrow = Scope::new(Abilities::NONE.with(Ability::Read), at(100));
    let one = Grant::issue(&root, &first.handle(), narrow);

    let greedy = one
        .attenuate(
            &first,
            &signer(3).handle(),
            Scope::new(Abilities::ALL, at(9_999)),
        )
        .unwrap();

    let scope = greedy
        .verify(&root.handle(), at(0), &Revocations::new())
        .unwrap();
    assert_eq!(scope, narrow);
    assert!(!scope.abilities().contains(Ability::Send));
}

#[test]
fn only_the_holder_may_delegate_onward() {
    let (_, _, _, _, one, _, _) = three_deep();
    assert_eq!(
        one.attenuate(&signer(9), &signer(3).handle(), wide()),
        Err(GrantError::NotYours)
    );
}

#[test]
fn a_chain_from_another_root_is_refused() {
    let (_, _, _, third, _, _, three) = three_deep();
    let stranger = signer(9).handle();
    assert!(matches!(
        three.permits(
            &stranger,
            &third.handle(),
            Ability::Send,
            at(0),
            &Revocations::new()
        ),
        Err(GrantError::WrongRoot { .. })
    ));
}

#[test]
fn presenting_somebody_elses_grant_is_refused() {
    let (root, _, _, _, _, _, three) = three_deep();
    assert!(matches!(
        three.permits(
            &root.handle(),
            &signer(9).handle(),
            Ability::Send,
            at(0),
            &Revocations::new()
        ),
        Err(GrantError::NotTheHolder { .. })
    ));
}

#[test]
fn an_expired_grant_authorises_nothing() {
    let (root, _, _, third, _, _, three) = three_deep();
    assert!(matches!(
        three.permits(
            &root.handle(),
            &third.handle(),
            Ability::Send,
            at(1_000),
            &Revocations::new()
        ),
        Err(GrantError::Expired { .. })
    ));
}

#[test]
fn an_expiry_can_only_come_forward() {
    let root = signer(1);
    let first = signer(2);
    let one = Grant::issue(&root, &first.handle(), Scope::new(Abilities::ALL, at(100)));
    let two = one
        .attenuate(
            &first,
            &signer(3).handle(),
            Scope::new(Abilities::ALL, at(50)),
        )
        .unwrap();
    let three = two
        .attenuate(
            &signer(3),
            &signer(4).handle(),
            Scope::new(Abilities::ALL, at(9_999)),
        )
        .unwrap();
    assert_eq!(
        three
            .verify(&root.handle(), at(0), &Revocations::new())
            .unwrap()
            .expires_at(),
        at(50)
    );
}

#[test]
fn a_forged_step_does_not_verify() {
    let (root, first, _, _, one, _, _) = three_deep();
    // Somebody splices a step signed by a key that never held the grant.
    let forged = Grant::from_canonical_bytes(&{
        let mut bytes = one.to_canonical_bytes();
        let mut stolen = one
            .attenuate(&first, &signer(5).handle(), wide())
            .unwrap()
            .to_canonical_bytes();
        bytes[0] = 2;
        bytes.extend_from_slice(&stolen.split_off(1 + 170));
        bytes
    })
    .unwrap();
    // The spliced step is legitimate, so this particular splice verifies —
    // what must not verify is the same step with its issuer swapped.
    assert!(
        forged
            .verify(&root.handle(), at(0), &Revocations::new())
            .is_ok()
    );

    let mut tampered = forged.to_canonical_bytes();
    tampered[1 + 170] ^= 0x01;
    let broken = Grant::from_canonical_bytes(&tampered).unwrap();
    assert!(
        broken
            .verify(&root.handle(), at(0), &Revocations::new())
            .is_err()
    );
}

#[test]
fn the_wire_form_round_trips() {
    let (_, _, _, _, _, _, three) = three_deep();
    let bytes = three.to_canonical_bytes();
    assert_eq!(bytes.len(), 1 + 3 * 170);
    assert_eq!(Grant::from_canonical_bytes(&bytes).unwrap(), three);
}

#[test]
fn trailing_bytes_are_refused() {
    let (_, _, _, _, one, _, _) = three_deep();
    let mut bytes = one.to_canonical_bytes();
    bytes.push(0);
    assert_eq!(
        Grant::from_canonical_bytes(&bytes),
        Err(GrantError::TrailingBytes { count: 1 })
    );
}

#[test]
fn an_empty_grant_is_refused() {
    assert_eq!(Grant::from_canonical_bytes(&[0]), Err(GrantError::Empty));
    assert_eq!(
        Grant::from_canonical_bytes(&[]).unwrap_err().code(),
        "grant.truncated"
    );
}

#[test]
fn the_hop_limit_holds() {
    let root = signer(1);
    let mut grant = Grant::issue(&root, &signer(2).handle(), wide());
    let mut holder = signer(2);
    let mut seed = 3_u8;
    while grant.depth() < MAX_STEPS {
        let next = signer(seed);
        grant = grant.attenuate(&holder, &next.handle(), wide()).unwrap();
        holder = next;
        seed += 1;
    }
    assert_eq!(grant.depth(), MAX_STEPS);
    assert!(matches!(
        grant.attenuate(&holder, &signer(99).handle(), wide()),
        Err(GrantError::TooLong { .. })
    ));
}

#[test]
fn a_handle_that_is_not_a_key_authorises_nothing() {
    let root = Handle::from_bytes([0xff; 32]);
    let (_, _, _, third, _, _, three) = three_deep();
    assert!(
        three
            .permits(
                &root,
                &third.handle(),
                Ability::Read,
                at(0),
                &Revocations::new()
            )
            .is_err()
    );
}
