// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What time it is, as a value somebody hands you.
//!
//! Nothing in this workspace reads the wall clock except one designated function
//! in the binary; everything else receives an [`Instant`]. That is not
//! fastidiousness about testing. A grant that expires and a drop that is swept
//! are decisions about the future, and a decision about the future that samples
//! its own clock cannot be replayed, cannot be tested at a boundary, and cannot
//! be audited after it went wrong.

/// A point in time: whole seconds since the Unix epoch, UTC.
///
/// Seconds rather than milliseconds because everything this network dates —
/// grant expiry, object retention — is measured in minutes at the finest, and a
/// unit finer than the decision invites false precision across machines whose
/// clocks disagree by more than the unit.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Instant(u64);

impl Instant {
    /// The Unix epoch itself.
    pub const EPOCH: Self = Self(0);

    /// The furthest representable point; used as "does not expire".
    pub const NEVER: Self = Self(u64::MAX);

    /// Wraps a count of seconds since the Unix epoch.
    #[must_use]
    pub const fn from_unix_seconds(seconds: u64) -> Self {
        Self(seconds)
    }

    /// Seconds since the Unix epoch.
    #[must_use]
    pub const fn as_unix_seconds(&self) -> u64 {
        self.0
    }

    /// This instant moved forward by `seconds`, saturating at [`Instant::NEVER`].
    ///
    /// Saturating is the safe direction here only because the saturation point is
    /// also the "never expires" value, and an expiry pushed beyond the end of
    /// time is one that has not been reached.
    #[must_use]
    pub const fn plus_seconds(&self, seconds: u64) -> Self {
        Self(self.0.saturating_add(seconds))
    }
}

/// Anything that can say what time it is.
///
/// Two implementations exist so that this is a seam rather than a description of
/// the operating system: the production clock lives in the binary's assembly
/// module, and [`FixedClock`] is the one that does not move.
pub trait Clock {
    /// The current time.
    fn now(&self) -> Instant;
}

/// A clock that always reports the same instant.
///
/// The second implementation of the [`Clock`] seam, and the one every test uses:
/// an expiry test whose result depends on how long the test took is a test that
/// will fail on somebody else's slower machine.
#[derive(Clone, Copy, Debug)]
pub struct FixedClock {
    at: Instant,
}

impl FixedClock {
    /// A clock stopped at `at`.
    #[must_use]
    pub const fn at(at: Instant) -> Self {
        Self { at }
    }

    /// Moves the stopped clock forward by `seconds`.
    #[must_use]
    pub const fn advanced(&self, seconds: u64) -> Self {
        Self {
            at: self.at.plus_seconds(seconds),
        }
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Instant {
        self.at
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
    use super::{Clock, FixedClock, Instant};

    #[test]
    fn a_fixed_clock_does_not_move() {
        let clock = FixedClock::at(Instant::from_unix_seconds(1_000));
        assert_eq!(clock.now(), clock.now());
        assert_eq!(clock.now().as_unix_seconds(), 1_000);
    }

    #[test]
    fn advancing_moves_only_the_copy() {
        let clock = FixedClock::at(Instant::from_unix_seconds(1_000));
        assert_eq!(clock.advanced(60).now().as_unix_seconds(), 1_060);
        assert_eq!(clock.now().as_unix_seconds(), 1_000);
    }

    #[test]
    fn time_saturates_at_never() {
        assert_eq!(Instant::NEVER.plus_seconds(1), Instant::NEVER);
        assert!(Instant::EPOCH < Instant::NEVER);
    }
}
