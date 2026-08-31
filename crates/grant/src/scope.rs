// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a grant permits, and the one operation that narrows it.
//!
//! A scope is a point in a lattice: a set of abilities ordered by inclusion, and
//! an expiry ordered by time. [`Scope::meet`] is the greatest lower bound, and it
//! is the *only* way a delegated scope is produced anywhere in this crate. That
//! is what makes "attenuation cannot widen" a fact about the code rather than a
//! rule the code checks — asking for more than you hold does not fail, it simply
//! yields what you hold.

use kusanagi_kernel::Instant;

/// One thing a holder may do.
///
/// A closed set. A new ability is a wire-format change and a new bit, decided
/// here, rather than a string that any caller can invent.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[non_exhaustive]
pub enum Ability {
    /// Append segments to this channel.
    Send,
    /// Read the other side's segments from this channel.
    Read,
}

impl Ability {
    /// Every ability, in wire order.
    pub const ALL: [Self; 2] = [Self::Send, Self::Read];

    /// This ability's bit in the wire encoding.
    const fn bit(self) -> u8 {
        match self {
            Self::Send => 0b0000_0001,
            Self::Read => 0b0000_0010,
        }
    }

    /// The published name, used in error text and in `doctor` output.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Send => "send",
            Self::Read => "read",
        }
    }
}

/// A set of abilities.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Abilities(u8);

/// Every bit that means something today. Anything else is refused on decode.
const KNOWN_BITS: u8 = Ability::Send.bit() | Ability::Read.bit();

impl Abilities {
    /// The empty set: a grant that permits nothing.
    pub const NONE: Self = Self(0);

    /// Every ability this version knows.
    pub const ALL: Self = Self(KNOWN_BITS);

    /// This set with `ability` added.
    #[must_use]
    pub const fn with(self, ability: Ability) -> Self {
        Self(self.0 | ability.bit())
    }

    /// Whether `ability` is in this set.
    #[must_use]
    pub const fn contains(self, ability: Ability) -> bool {
        self.0 & ability.bit() != 0
    }

    /// The abilities in both sets.
    #[must_use]
    pub const fn meet(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// Whether every ability here is also in `wider`.
    #[must_use]
    pub const fn is_within(self, wider: Self) -> bool {
        self.0 & wider.0 == self.0
    }

    /// The wire encoding.
    #[must_use]
    pub const fn bits(self) -> u8 {
        self.0
    }

    /// Reads a set from the wire.
    ///
    /// # Errors
    ///
    /// [`UnknownAbility`] when a bit is set that this version does not define.
    /// Refusing is the fail-closed direction: a verifier that ignored unknown
    /// bits would silently approve a delegation it cannot evaluate.
    pub const fn from_bits(bits: u8) -> Result<Self, UnknownAbility> {
        if bits & !KNOWN_BITS != 0 {
            return Err(UnknownAbility { bits });
        }
        Ok(Self(bits))
    }
}

/// A grant carried a bit this version does not understand.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("this grant names an ability this version does not define (bits {bits:#010b})")]
pub struct UnknownAbility {
    /// The byte as it appeared on the wire.
    pub bits: u8,
}

/// What a grant permits, and until when.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Scope {
    abilities: Abilities,
    expires_at: Instant,
}

impl Scope {
    /// A scope with these abilities, expiring at this instant.
    #[must_use]
    pub const fn new(abilities: Abilities, expires_at: Instant) -> Self {
        Self {
            abilities,
            expires_at,
        }
    }

    /// The abilities.
    #[must_use]
    pub const fn abilities(&self) -> Abilities {
        self.abilities
    }

    /// When it stops applying.
    #[must_use]
    pub const fn expires_at(&self) -> Instant {
        self.expires_at
    }

    /// The greatest scope no wider than either of these.
    #[must_use]
    pub fn meet(&self, other: &Self) -> Self {
        Self {
            abilities: self.abilities.meet(other.abilities),
            expires_at: self.expires_at.min(other.expires_at),
        }
    }

    /// Whether this scope grants nothing `wider` does not.
    #[must_use]
    pub fn is_within(&self, wider: &Self) -> bool {
        self.abilities.is_within(wider.abilities) && self.expires_at <= wider.expires_at
    }

    /// Whether this scope still stands at `now`.
    #[must_use]
    pub fn is_live_at(&self, now: Instant) -> bool {
        now < self.expires_at
    }

    /// Whether this scope permits `ability` at `now`.
    #[must_use]
    pub fn permits(&self, ability: Ability, now: Instant) -> bool {
        self.is_live_at(now) && self.abilities.contains(ability)
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
    use super::{Abilities, Ability, Scope, UnknownAbility};
    use kusanagi_kernel::Instant;

    fn at(seconds: u64) -> Instant {
        Instant::from_unix_seconds(seconds)
    }

    #[test]
    fn a_meet_is_never_wider_than_either_side() {
        let left = Scope::new(Abilities::NONE.with(Ability::Send), at(100));
        let right = Scope::new(Abilities::ALL, at(50));
        let met = left.meet(&right);
        assert!(met.is_within(&left));
        assert!(met.is_within(&right));
        assert_eq!(met.abilities(), Abilities::NONE.with(Ability::Send));
        assert_eq!(met.expires_at(), at(50));
    }

    #[test]
    fn asking_for_more_yields_what_you_hold() {
        let held = Scope::new(Abilities::NONE.with(Ability::Read), at(50));
        let asked = Scope::new(Abilities::ALL, at(u64::MAX));
        assert_eq!(held.meet(&asked), held);
    }

    #[test]
    fn meet_is_idempotent_and_commutative() {
        let left = Scope::new(Abilities::ALL, at(100));
        let right = Scope::new(Abilities::NONE.with(Ability::Read), at(200));
        assert_eq!(left.meet(&left), left);
        assert_eq!(left.meet(&right), right.meet(&left));
    }

    #[test]
    fn expiry_is_the_moment_it_stops() {
        let scope = Scope::new(Abilities::ALL, at(100));
        assert!(scope.permits(Ability::Send, at(99)));
        assert!(!scope.permits(Ability::Send, at(100)));
        assert!(!scope.permits(Ability::Send, at(101)));
    }

    #[test]
    fn an_unheld_ability_is_not_permitted() {
        let scope = Scope::new(Abilities::NONE.with(Ability::Read), at(100));
        assert!(scope.permits(Ability::Read, at(0)));
        assert!(!scope.permits(Ability::Send, at(0)));
    }

    #[test]
    fn unknown_bits_are_refused() {
        assert_eq!(Abilities::from_bits(0b11), Ok(Abilities::ALL));
        assert_eq!(
            Abilities::from_bits(0b100),
            Err(UnknownAbility { bits: 0b100 })
        );
    }
}
