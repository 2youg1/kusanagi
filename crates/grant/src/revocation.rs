// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The steps that no longer count.
//!
//! Revocation is a set of step identifiers and nothing more. It has no signature
//! and no expiry of its own because it is not a claim that travels — it is what a
//! verifier already knows, handed to [`Grant::verify`](crate::Grant::verify) as a
//! parameter. Publishing and distributing revocations is a transport problem, and
//! keeping it out of this type is what stops "who told you this was revoked" from
//! becoming a second authority on what a grant means.
//!
//! Revoking one step voids every step beneath it, because verification walks the
//! chain from the root: there is no separate cascade to keep in step.

use core::iter::FromIterator;
use std::collections::BTreeSet;

use crate::step::StepId;

/// A set of revoked steps.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct Revocations(BTreeSet<StepId>);

impl Revocations {
    /// Nothing is revoked.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a step, returning the larger set.
    ///
    /// Takes and returns ownership so that a revocation list reads as a value
    /// being built rather than a mutable thing being poked, which is how it is
    /// used at every call site.
    #[must_use]
    pub fn revoking(mut self, step: StepId) -> Self {
        self.0.insert(step);
        self
    }

    /// Whether this step has been revoked.
    #[must_use]
    pub fn contains(&self, step: &StepId) -> bool {
        self.0.contains(step)
    }

    /// How many steps are revoked.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether nothing is revoked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every revoked step, in a stable order.
    pub fn iter(&self) -> impl Iterator<Item = &StepId> {
        self.0.iter()
    }
}

impl FromIterator<StepId> for Revocations {
    fn from_iter<I: IntoIterator<Item = StepId>>(steps: I) -> Self {
        Self(steps.into_iter().collect())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::Revocations;
    use crate::step::StepId;

    #[test]
    fn a_revoked_step_is_remembered_once() {
        let step = StepId::from_bytes([1; 32]);
        let set = Revocations::new().revoking(step).revoking(step);
        assert_eq!(set.len(), 1);
        assert!(set.contains(&step));
        assert!(!set.contains(&StepId::from_bytes([2; 32])));
    }

    #[test]
    fn an_empty_set_revokes_nothing() {
        assert!(Revocations::new().is_empty());
        assert!(!Revocations::new().contains(&StepId::from_bytes([1; 32])));
    }

    #[test]
    fn the_order_is_stable() {
        let set: Revocations = [StepId::from_bytes([2; 32]), StepId::from_bytes([1; 32])]
            .into_iter()
            .collect();
        let order: Vec<_> = set.iter().copied().collect();
        assert_eq!(
            order,
            vec![StepId::from_bytes([1; 32]), StepId::from_bytes([2; 32])]
        );
    }
}
