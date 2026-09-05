// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How long one bin of a ward lasts, and which one a clock reading falls in.
//!
//! A [`Period`] is public. Every host has always seen when a drop arrived, so
//! putting the period in the key gives away nothing the host did not have, and
//! it buys three things a private bin could not:
//!
//! - a bin small enough that a reader takes all of it in one request;
//! - one lifetime for every object in it, so a host expiring a bin cannot tell a
//!   channel that releases from one that keeps;
//! - agreement without coordination — a writer and a reader that share a clock
//!   share a bin, and nothing has to be negotiated.
//!
//! The length is here, beside address derivation, because it is a **protocol
//! constant**: two endpoints that disagree about it write and read different
//! bins and never meet. It is unrelated to `Cadence`, which is how often *one*
//! endpoint writes: a slow channel's segments simply land in the bins of the
//! moments they were written.

use kusanagi_kernel::{Bin, Period, Ward};

use crate::secret::Secret;

/// How many seconds one period lasts.
///
/// Ten minutes, which is the trade this constant *is*. Shorter periods mean more
/// bins to sweep when catching up and fewer objects in each; longer periods mean
/// one cheap sweep and a larger crowd, since everything written in the same ten
/// minutes shares a bin. Ten minutes keeps a reader that polls every few minutes
/// at one or two bins per poll while leaving a bin big enough for the writers of
/// one ward to be indistinguishable within it.
///
/// **Changing this is a protocol break.** Two endpoints on different numbers
/// write and read different keys, and neither of them sees an error: the
/// symptom is silence. It therefore moves only with the suite byte in an
/// invitation.
pub const PERIOD_SECONDS: u64 = 600;

/// Which period a clock reading falls in.
///
/// Integer division, so every endpoint on the same second agrees, and the
/// boundary is the one place a segment written at 09:59:59 and one written at
/// 10:00:00 land apart. That costs a reader one extra bin at the edge and costs
/// nobody a message, because a reader sweeps the bins it has not yet swept
/// rather than only the current one.
#[must_use]
pub const fn period(now_seconds: u64) -> Period {
    Period::from_count(now_seconds / PERIOD_SECONDS)
}

/// The context string that separates a rendezvous ward from every other use.
const RENDEZVOUS_CONTEXT: &str = "kusanagi 2026 rendezvous ward v1";

/// The bin the two one-time drops of a channel sit in, before either end knows
/// the other.
///
/// **The one bin that is not a reader's ward**, and the reason is the order the
/// world happens in: an offer is written before anybody has accepted it, and a
/// greeting is written by somebody the inviter has not met, so neither drop can
/// be filed where a reader sweeps. Both ends can compute this one from the
/// channel secret, which the invitation carries and nobody else has.
///
/// Period zero, because a newcomer does not know when the invitation was
/// written and a sweep never looks here. What a host sees is two objects in a
/// ward that never receives another byte — the shape of an invitation, which it
/// could already see from the two requests that fetch them.
#[must_use]
pub fn rendezvous(secret: &Secret) -> Bin {
    let mut two = [0_u8; 2];
    let mut hasher = blake3::Hasher::new_derive_key(RENDEZVOUS_CONTEXT);
    hasher.update(secret.as_bytes());
    hasher.finalize_xof().fill(&mut two);
    Bin::new(
        Period::from_count(0),
        Ward::from_bits(u16::from_be_bytes(two)),
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{PERIOD_SECONDS, period, rendezvous};
    use crate::secret::Secret;

    #[test]
    fn a_period_holds_every_instant_of_its_own_span() {
        assert_eq!(period(0), period(PERIOD_SECONDS - 1));
        assert_ne!(period(PERIOD_SECONDS - 1), period(PERIOD_SECONDS));
        assert_eq!(
            period(PERIOD_SECONDS).count(),
            period(0).count().saturating_add(1)
        );
    }

    #[test]
    fn periods_count_upwards_from_the_epoch() {
        // A number a reader can check against a clock without running anything:
        // 2026-01-01T00:00:00Z is 1 767 225 600 seconds, and ten-minute periods
        // divide it exactly.
        assert_eq!(period(1_767_225_600).count(), 2_945_376);
    }

    #[test]
    fn a_rendezvous_is_the_same_bin_at_both_ends_and_differs_between_channels() {
        let one = Secret::from_bytes([0x5a; 32]);
        let two = Secret::from_bytes([0x5b; 32]);
        assert_eq!(
            rendezvous(&one),
            rendezvous(&Secret::from_bytes([0x5a; 32]))
        );
        assert_ne!(rendezvous(&one).ward(), rendezvous(&two).ward());
        assert_eq!(rendezvous(&one).period().count(), 0);
    }
}
