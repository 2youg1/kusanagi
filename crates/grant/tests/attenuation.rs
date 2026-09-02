// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The property the whole permission model rests on: **delegation never widens**.
//!
//! `tests/chain.rs` checks the cases somebody thought of. This checks the ones
//! nobody thought of, over arbitrary chains of arbitrary requests, including
//! requests for far more than the delegator holds.
//!
//! `ARCHITECTURE.md` names `kani` as the tool that should eventually prove this
//! against real MIR rather than sample it. The proof harness is committed beside
//! the code in `src/chain.rs` under `#[cfg(kani)]`; `kani` is not installed on
//! this machine, so what actually runs today is this.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use kusanagi_grant::{Abilities, Ability, Grant, MAX_STEPS, Revocations, Scope};
use kusanagi_kernel::{Instant, Signer};
use proptest::prelude::{Strategy, any, prop};
use proptest::{prop_assert, prop_assert_eq, proptest};

/// An arbitrary scope, including ones that ask for everything forever.
fn scope() -> impl Strategy<Value = Scope> {
    (0_u8..=0b11, any::<u64>()).prop_map(|(bits, seconds)| {
        Scope::new(
            Abilities::from_bits(bits).unwrap_or(Abilities::NONE),
            Instant::from_unix_seconds(seconds),
        )
    })
}

proptest! {
    /// Whatever is requested at each hop, what comes out is inside what went in.
    #[test]
    fn no_chain_of_requests_ever_widens(
        root_scope in scope(),
        requests in prop::collection::vec(scope(), 0..MAX_STEPS),
    ) {
        let root = Signer::from_seed(&[0; 32]);
        let first = Signer::from_seed(&[1; 32]);
        let mut grant = Grant::issue(&root, &first.handle(), root_scope);
        let mut holder = Signer::from_seed(&[1; 32]);

        for (hop, request) in requests.into_iter().enumerate() {
            let seed = u8::try_from(hop).unwrap_or(u8::MAX).saturating_add(2);
            let next = Signer::from_seed(&[seed; 32]);
            let Ok(wider) = grant.attenuate(&holder, &next.handle(), request) else {
                break;
            };
            grant = wider;
            holder = Signer::from_seed(&[seed; 32]);

            let last = grant.steps().last().unwrap();
            let parent = grant.steps()[grant.depth() - 2].scope();
            prop_assert!(
                last.scope().is_within(&parent),
                "a hop produced a scope outside the one above it"
            );
            prop_assert!(
                last.scope().is_within(&root_scope),
                "a hop escaped the scope the root issued"
            );
        }

        // and the same holds for what a verifier concludes, not just for what
        // the builder wrote down
        let verified = grant.verify(&root.handle(), Instant::EPOCH, &Revocations::new());
        if let Ok(scope) = verified {
            prop_assert!(scope.is_within(&root_scope));
        }
    }

    /// Revoking any step in a chain voids every grant that passes through it.
    #[test]
    fn revoking_any_step_voids_everything_beneath_it(cut in 0_usize..4) {
        let root = Signer::from_seed(&[0; 32]);
        let holders: Vec<Signer> = (1..=4_u8).map(|seed| Signer::from_seed(&[seed; 32])).collect();
        let wide = Scope::new(Abilities::ALL, Instant::from_unix_seconds(1_000));

        let mut grant = Grant::issue(&root, &holders[0].handle(), wide);
        let mut chain = vec![grant.clone()];
        for hop in 1..holders.len() {
            grant = grant
                .attenuate(&holders[hop - 1], &holders[hop].handle(), wide)
                .unwrap();
            chain.push(grant.clone());
        }

        let revoked = Revocations::new().revoking(chain[cut].steps().last().unwrap().id());
        for (depth, link) in chain.iter().enumerate() {
            let outcome = link.verify(&root.handle(), Instant::EPOCH, &revoked);
            prop_assert_eq!(
                outcome.is_err(),
                depth >= cut,
                "revoking step {} left depth {} in the wrong state",
                cut,
                depth
            );
        }
    }

    /// A grant survives the wire exactly, or does not decode at all.
    #[test]
    fn tampering_with_the_wire_form_is_always_caught(at in 0_usize..(1 + 2 * 170)) {
        let root = Signer::from_seed(&[0; 32]);
        let first = Signer::from_seed(&[1; 32]);
        let wide = Scope::new(Abilities::ALL, Instant::from_unix_seconds(1_000));
        let grant = Grant::issue(&root, &first.handle(), wide)
            .attenuate(&first, &Signer::from_seed(&[2; 32]).handle(), wide)
            .unwrap();

        let mut bytes = grant.to_canonical_bytes();
        bytes[at] ^= 0x01;
        let outcome = Grant::from_canonical_bytes(&bytes)
            .and_then(|decoded| decoded.verify(&root.handle(), Instant::EPOCH, &Revocations::new()));
        prop_assert!(outcome.is_err(), "a flipped bit at {} still verified", at);
    }
}

/// The abilities lattice: `meet` is a greatest lower bound, checked exhaustively
/// over the whole domain, which is small enough that sampling would be a waste.
#[test]
fn meet_is_a_greatest_lower_bound_over_every_ability_set() {
    let sets: Vec<Abilities> = (0_u8..=0b11)
        .filter_map(|bits| Abilities::from_bits(bits).ok())
        .collect();
    for &left in &sets {
        for &right in &sets {
            let met = left.meet(right);
            assert!(met.is_within(left), "meet escaped its left operand");
            assert!(met.is_within(right), "meet escaped its right operand");
            assert_eq!(met, right.meet(left), "meet is not commutative");
            assert_eq!(
                met.meet(left),
                met,
                "meet is not idempotent under its operand"
            );
            for ability in Ability::ALL {
                assert_eq!(
                    met.contains(ability),
                    left.contains(ability) && right.contains(ability)
                );
            }
        }
    }
}
