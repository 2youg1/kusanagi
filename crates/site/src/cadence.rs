// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! When an endpoint writes on a channel, which is a thing an observer can read.
//!
//! Everything else in this design hides *what* is said. The rhythm of saying it
//! is not hidden by any of it: a path observer who cannot decrypt one byte still
//! sees that this endpoint wrote three times in a minute and then went quiet for
//! a day, and `adversary/` measures exactly that — the number of drops and the
//! gaps between requests separate a silent world from a busy one.
//!
//! **A slotted channel makes the rhythm a constant.** Each end has a period and
//! writes exactly one drop per period, carrying whatever was queued or a filler
//! segment when there was nothing. The schedule is public and deterministic on
//! purpose, because `ARCHITECTURE.md` §3 already ruled on that shape: hiding a
//! schedule is a computational defence an unbounded adversary strips away, while
//! filling every slot survives being fully known.
//!
//! **The phase is per endpoint and per channel**, derived from the channel secret
//! and the writer's own handle. Two ends on one period but the same phase would
//! write together, and a host that saw two drops arrive in step every period
//! would have paired them without decrypting anything — which is the one thing
//! derived addresses exist to prevent.
//!
//! What it costs is written down rather than argued away: latency is the period,
//! a channel spends 128 KiB per period per direction whether or not anybody says
//! anything, and **an endpoint that is offline when a slot comes round leaves a
//! gap.** That gap is the residual, it is measurable, and closing it is what a
//! real carrier is for.

use core::num::{NonZeroU32, NonZeroU64};

use kusanagi_kernel::Reader;

use crate::blocks::malformed;
use crate::error::SiteError;

const ON_DEMAND: u8 = 0;
const SLOTTED: u8 = 1;

/// How often this endpoint writes on a channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cadence {
    /// Write when there is something to say, and not otherwise.
    ///
    /// The default, and the only one that keeps a verb free: an on-demand
    /// channel needs no scheduler, no background process and no clock.
    OnDemand,
    /// Write exactly one drop every `period` seconds, whatever there is to say.
    Slotted {
        /// How many seconds one slot lasts.
        ///
        /// `NonZeroU32` because a period of zero is not a fast channel, it is a
        /// division by zero — and a type that cannot hold it is cheaper than a
        /// check every caller has to remember.
        period: NonZeroU32,
    },
}

impl Cadence {
    /// Which slot `now` falls in, for a channel that has slots.
    ///
    /// `phase` shifts this endpoint's slot boundaries away from its peer's; only
    /// its remainder matters, so a caller may pass any derived number.
    #[must_use]
    pub fn slot(self, now: u64, phase: u64) -> Option<u64> {
        let period = NonZeroU64::from(self.seconds()?);
        // The divisor is non-zero in the type, so neither the remainder nor the
        // division has a failing case to handle. The addition saturates rather
        // than wrapping, so that a clock near the end of time cannot fold back
        // to slot zero and rewrite a slot that has already been used.
        Some(now.saturating_add(phase % period) / period)
    }

    /// The period as the type that carries what the arithmetic needs.
    const fn seconds(self) -> Option<NonZeroU32> {
        match self {
            Self::OnDemand => None,
            Self::Slotted { period } => Some(period),
        }
    }

    /// The number of seconds in a period, for reporting.
    #[must_use]
    pub const fn period(self) -> Option<u32> {
        match self.seconds() {
            None => None,
            Some(period) => Some(period.get()),
        }
    }

    /// Writes one byte, and four more only when there is a period to write.
    ///
    /// **An on-demand channel spends no bytes on a number it does not have.**
    /// Padding the tag out to a fixed width would put four bytes in the record
    /// that nothing reads, which is a second spelling of the same record — and
    /// `site`'s decoder tests refuse that, because a field the decoder ignores is
    /// a field somebody can change without being noticed.
    pub(crate) fn write(self, out: &mut Vec<u8>) {
        match self {
            Self::OnDemand => out.push(ON_DEMAND),
            Self::Slotted { period } => {
                out.push(SLOTTED);
                out.extend_from_slice(&period.get().to_be_bytes());
            }
        }
    }

    pub(crate) fn read(reader: &mut Reader<'_>) -> Result<Self, SiteError> {
        match reader.take_byte().map_err(malformed)? {
            ON_DEMAND => Ok(Self::OnDemand),
            SLOTTED => {
                let seconds = u32::from_be_bytes(reader.take_array::<4>().map_err(malformed)?);
                NonZeroU32::new(seconds)
                    .map(|period| Self::Slotted { period })
                    .ok_or_else(|| SiteError::BadRecord {
                        what: "a cadence",
                        reason: "a slotted channel has a period of at least one second".to_owned(),
                    })
            }
            other => Err(SiteError::BadRecord {
                what: "a cadence",
                reason: format!("a cadence is on-demand or slotted, not {other}"),
            }),
        }
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
    use super::Cadence;
    use core::num::NonZeroU32;

    fn every(seconds: u32) -> Cadence {
        Cadence::Slotted {
            period: NonZeroU32::new(seconds).unwrap(),
        }
    }

    #[test]
    fn an_on_demand_channel_has_no_slots() {
        assert_eq!(Cadence::OnDemand.slot(1_000, 0), None);
        assert_eq!(Cadence::OnDemand.period(), None);
    }

    #[test]
    fn a_slot_lasts_exactly_one_period() {
        let cadence = every(60);
        assert_eq!(cadence.slot(0, 0), Some(0));
        assert_eq!(cadence.slot(59, 0), Some(0));
        assert_eq!(cadence.slot(60, 0), Some(1));
        assert_eq!(cadence.period(), Some(60));
    }

    /// The property the host must not be able to use: two ends of one channel
    /// do not change slot at the same instant.
    #[test]
    fn two_phases_put_their_boundaries_in_different_places() {
        let cadence = every(60);
        let mine = (0..120).map(|now| cadence.slot(now, 0));
        let theirs = (0..120).map(|now| cadence.slot(now, 17));
        assert!(
            mine.zip(theirs).any(|(a, b)| a != b),
            "two phases produced the same boundaries"
        );
    }
}
