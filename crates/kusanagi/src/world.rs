// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The two facts this program cannot compute: what time it is, and what nobody
//! can predict.
//!
//! Everything else in kusanagi is a function of its inputs, which is what makes
//! a run reproducible and a failure explicable. These two are not, so they are
//! confined to one small file that a reviewer can read in a minute — and
//! `clippy.toml` denies the clock everywhere so that a second sampling point
//! cannot appear without somebody noticing.

use kusanagi_kernel::{Clock, Instant};

use crate::complaint::Complaint;

/// The clock of the machine this is running on.
#[derive(Debug, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        sample()
    }
}

/// Reads the wall clock. **The only place in this program that does.**
///
/// A clock before the Unix epoch is reported as the epoch rather than as an
/// error. The alternative is a machine whose clock is badly wrong being unable to
/// run any command at all, including the ones that would tell its operator why;
/// every decision this value feeds — grant expiry, object lifetime — fails closed
/// at the epoch, so the wrong answer is the safe one.
#[expect(
    clippy::disallowed_methods,
    reason = "the single sampling point that clippy.toml and AGENTS.md designate"
)]
fn sample() -> Instant {
    let since_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    Instant::from_unix_seconds(since_epoch)
}

/// Thirty-two bytes nobody can predict.
///
/// Straight from the operating system, with no intermediate generator to seed,
/// reseed or fork wrongly. Every secret this program creates begins here.
///
/// # Errors
///
/// [`Complaint::Local`] when the operating system has no entropy to give, which
/// is a refusal to proceed rather than a fallback: a predictable channel secret
/// is worse than no channel.
pub fn fresh_seed() -> Result<[u8; 32], Complaint> {
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|source| Complaint::Local {
        action: "ask the operating system for randomness",
        source: std::io::Error::other(source.to_string()),
    })?;
    Ok(seed)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{SystemClock, fresh_seed};
    use kusanagi_kernel::Clock as _;

    #[test]
    fn the_clock_is_somewhere_this_century() {
        // 2020-01-01 and 2200-01-01: wide enough that a correct machine passes and
        // narrow enough that a clock reading of zero does not.
        let now = SystemClock.now().as_unix_seconds();
        assert!(now > 1_577_836_800, "the clock reads before 2020");
        assert!(now < 7_258_118_400, "the clock reads after 2200");
    }

    #[test]
    fn two_seeds_are_not_the_same_seed() {
        assert_ne!(fresh_seed().unwrap(), fresh_seed().unwrap());
        assert_ne!(fresh_seed().unwrap(), [0_u8; 32]);
    }
}
