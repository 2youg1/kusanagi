// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The set of bins one request asks for.
//!
//! Apart from the nouns in `filing` because it is the one knob a reader turns
//! alone: how many leading hex digits of its ward a request names. Fewer digits
//! is a larger crowd to hide in and more bytes to download, and no writer, host
//! or peer is consulted about the choice.

use crate::filing::{Bin, Object, Period, Ward};

/// The set of bins a reader asks for at once.
///
/// `digits` is how many of the ward's four hex digits the request names, so four
/// is one ward and zero is every ward on the host. Each digit dropped multiplies
/// the anonymity set by sixteen and the bandwidth with it — the reader chooses
/// where on that line to stand, alone, and neither the writers nor the host is
/// told which choice was made beyond what the request itself says.
///
/// Hex digits rather than bits because a request carries a **string** prefix on
/// every adapter that lists; a prefix of three and a half digits would have to
/// become several requests, and several requests are a pattern a host can read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sweep {
    bin: Bin,
    digits: u8,
}

impl Sweep {
    /// Names `digits` leading hex digits of `bin`'s ward, saturating at four.
    ///
    /// Saturating rather than refusing: asking for more digits than a ward has is
    /// asking for one ward, which is what four means, and there is no caller for
    /// whom that is the wrong answer.
    #[must_use]
    pub const fn of(bin: Bin, digits: u8) -> Self {
        Self {
            bin,
            digits: if digits > Ward::DIGITS {
                Ward::DIGITS
            } else {
                digits
            },
        }
    }

    /// The period every bin in this sweep shares.
    #[must_use]
    pub const fn period(&self) -> Period {
        self.bin.period()
    }

    /// How many hex digits of the ward the request names.
    #[must_use]
    pub const fn digits(&self) -> u8 {
        self.digits
    }

    /// The key prefix this sweep asks for, ending at a component boundary when
    /// it names a whole ward.
    ///
    /// A whole ward ends in `/` so that ward `00ab` cannot answer with the keys
    /// of ward `00abc…` — which cannot exist today, and would be a silent
    /// widening of the request on the day a ward grows a digit.
    #[must_use]
    pub fn prefix(&self) -> String {
        let ward = self.bin.ward().to_string();
        let kept: String = ward.chars().take(usize::from(self.digits)).collect();
        if self.digits == Ward::DIGITS {
            format!("{}/{kept}/", self.bin.period())
        } else {
            format!("{}/{kept}", self.bin.period())
        }
    }

    /// The sweep that asked for `prefix`, if a sweep could have.
    ///
    /// The inverse of [`Sweep::prefix`] and beside it, because a host parsing
    /// what a reader wrote is the one place the two spellings could drift apart.
    /// A ward shorter than four digits names more wards, and the digits it does
    /// not give are zero — which is why the parsed ward is padded rather than
    /// refused.
    #[must_use]
    pub fn from_prefix(prefix: &str) -> Option<Self> {
        let mut parts = prefix.trim_end_matches('/').split('/');
        let (Some(period), ward, None) = (parts.next(), parts.next(), parts.next()) else {
            return None;
        };
        let ward = ward.unwrap_or_default();
        if period.len() != 16 || ward.len() > usize::from(Ward::DIGITS) {
            return None;
        }
        let digits = u8::try_from(ward.len()).ok()?;
        let padded = format!("{ward:0<width$}", width = usize::from(Ward::DIGITS));
        Some(Self::of(
            Bin::new(
                Period::from_count(u64::from_str_radix(period, 16).ok()?),
                Ward::from_bits(u16::from_str_radix(&padded, 16).ok()?),
            ),
            digits,
        ))
    }

    /// Whether `bin` is one this sweep asked for.
    ///
    /// The one authority for that question, so that an adapter which lists too
    /// much — a directory of wards, a bucket that ignores a prefix — is narrowed
    /// here rather than trusted there. On the bits rather than on the text of
    /// [`Sweep::prefix`]: the same rule read twice is two rules, and this is the
    /// reading that runs once per object.
    #[must_use]
    pub fn covers(&self, bin: Bin) -> bool {
        let dropped = u32::from(Ward::DIGITS.saturating_sub(self.digits)).saturating_mul(4);
        // Shifting a `u16` by sixteen has no answer, and the answer it has no
        // room for is zero: a sweep of no digits holds every ward.
        let named = u16::MAX.checked_shl(dropped).unwrap_or(0);
        bin.period() == self.period() && bin.ward().bits() & named == self.bin.ward().bits() & named
    }

    /// Whether `object` is one this sweep asked for.
    #[must_use]
    pub fn holds(&self, object: &Object) -> bool {
        self.covers(object.bin())
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
    use super::Sweep;
    use crate::address::DropAddr;
    use crate::filing::{Bin, Object, Period, Ward};

    fn object(period: u64, ward: u16, byte: u8) -> Object {
        Object::new(
            Bin::new(Period::from_count(period), Ward::from_bits(ward)),
            DropAddr::from_bytes([byte; 20]),
        )
    }

    #[test]
    fn a_whole_ward_is_asked_for_up_to_the_separator() {
        let sweep = Sweep::of(object(1, 0x00ab, 0).bin(), 4);
        assert_eq!(sweep.prefix(), "0000000000000001/00ab/");
        assert_eq!(
            Sweep::of(object(1, 0x00ab, 0).bin(), 2).prefix(),
            "0000000000000001/00"
        );
    }

    #[test]
    fn a_prefix_reads_back_as_the_sweep_that_wrote_it() {
        for digits in 0..=Ward::DIGITS {
            let sweep = Sweep::of(object(0x2b, 0x3c5a, 0).bin(), digits);
            let read = Sweep::from_prefix(&sweep.prefix()).unwrap();
            assert_eq!(read.digits(), digits);
            assert_eq!(read.prefix(), sweep.prefix());
        }
        for bad in ["", "zzzz", "0000000000000001/00abc", "1/00", "a/b/c"] {
            assert!(Sweep::from_prefix(bad).is_none(), "{bad} parsed");
        }
    }

    #[test]
    fn asking_for_more_digits_than_a_ward_has_asks_for_one_ward() {
        assert_eq!(Sweep::of(object(1, 7, 0).bin(), 9).digits(), Ward::DIGITS);
    }

    #[test]
    fn what_a_sweep_asks_for_in_text_is_what_it_keeps_in_bits() {
        // The two readings of one rule, held together for every ward and every
        // width, because a listing is filtered by the second after being asked
        // for by the first.
        for digits in 0..=Ward::DIGITS {
            let sweep = Sweep::of(object(9, 0x3c5a, 0).bin(), digits);
            for ward in 0..=u16::MAX {
                let at = object(9, ward, 0x11);
                assert_eq!(
                    at.to_string().starts_with(&sweep.prefix()),
                    sweep.holds(&at),
                    "ward {ward:04x} at {digits} digits"
                );
            }
        }
    }

    #[test]
    fn a_shorter_prefix_holds_more_wards_and_never_another_period() {
        let sweep = Sweep::of(object(4, 0x00ab, 0).bin(), 2);
        assert!(sweep.holds(&object(4, 0x00ab, 1)));
        assert!(sweep.holds(&object(4, 0x00ff, 1)), "same two digits");
        assert!(
            !sweep.holds(&object(4, 0x01ab, 1)),
            "a different second digit"
        );
        assert!(!sweep.holds(&object(5, 0x00ab, 1)), "a different period");
    }

    #[test]
    fn zero_digits_is_every_ward_of_one_period() {
        let sweep = Sweep::of(object(4, 0x00ab, 0).bin(), 0);
        assert_eq!(sweep.prefix(), "0000000000000004/");
        assert!(sweep.holds(&object(4, 0xffff, 1)));
        assert!(!sweep.holds(&object(3, 0x00ab, 1)));
    }
}
