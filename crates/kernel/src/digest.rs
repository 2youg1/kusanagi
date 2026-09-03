// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A fixed-width opaque identifier and its one textual form.
//!
//! Several things in this network are fixed-width opaque identifiers of different
//! widths: a [`Handle`](crate::Handle), a [`SegmentId`](crate::SegmentId), a
//! [`DropAddr`](crate::DropAddr), a [`Signature`](crate::Signature). They share
//! exactly one rule — the text form is lowercase hexadecimal of the full width,
//! and nothing else parses. Keeping that rule in one place is why this type
//! exists; it owns the policy that an identifier has one spelling, not merely the
//! syntax of printing bytes.

use core::fmt;
use core::str::FromStr;

use subtle::ConstantTimeEq as _;

use crate::wire::{self, Hex};

/// A fixed-width opaque identifier, rendered and parsed as lowercase hexadecimal.
///
/// Equality is constant-time. Every identifier in this workspace is a `Digest`,
/// so the rule is held in one place rather than five, and an authenticator that
/// becomes one is compared correctly by construction.
#[derive(Clone, Copy, Eq, PartialOrd, Ord)]
pub struct Digest<const N: usize>([u8; N]);

impl<const N: usize> PartialEq for Digest<N> {
    /// Compares in a time that does not depend on where two identifiers differ.
    fn eq(&self, other: &Self) -> bool {
        self.0.as_slice().ct_eq(other.0.as_slice()).into()
    }
}

impl<const N: usize> core::hash::Hash for Digest<N> {
    /// Hand-written because `PartialEq` is, and the two have to agree.
    ///
    /// Ordering stays derived and stays variable-time. That is deliberate: a
    /// comparison that answers "which is larger" cannot be made to leak less
    /// than it already tells its caller, and nothing here orders a secret.
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}

impl<const N: usize> Digest<N> {
    /// Wraps `N` raw bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; N]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; N] {
        &self.0
    }
}

impl<const N: usize> fmt::Display for Digest<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&Hex(&self.0), f)
    }
}

impl<const N: usize> fmt::Debug for Digest<N> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl<const N: usize> FromStr for Digest<N> {
    type Err = DigestParseError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let expected = N
            .checked_mul(2)
            .ok_or(DigestParseError::WidthUnrepresentable)?;
        if text.len() != expected {
            return Err(DigestParseError::Length {
                expected,
                found: text.len(),
            });
        }
        let bytes = wire::unhex(text)?;
        let sized =
            <[u8; N]>::try_from(bytes.as_slice()).map_err(|_| DigestParseError::Length {
                expected,
                found: text.len(),
            })?;
        Ok(Self(sized))
    }
}

/// Declares a newtype identifier over a [`Digest`] of the given width.
///
/// Every identifier in this network must render, parse, compare, and hash the
/// same way; three hand-written copies of those four impls is three chances for
/// one of them to drift. Each type still adds its own inherent methods below its
/// declaration — the macro supplies only what is common to all of them.
#[macro_export]
macro_rules! identifier {
    ($(#[$meta:meta])* $name:ident, $width:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $name($crate::Digest<$width>);

        impl $name {
            /// Wraps the raw bytes.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; $width]) -> Self {
                Self($crate::Digest::from_bytes(bytes))
            }

            /// Borrows the raw bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; $width] {
                self.0.as_bytes()
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                ::core::fmt::Display::fmt(&self.0, f)
            }
        }

        impl ::core::str::FromStr for $name {
            type Err = $crate::DigestParseError;

            fn from_str(text: &str) -> ::core::result::Result<Self, Self::Err> {
                text.parse().map($name)
            }
        }
    };
}

/// Why a piece of text is not an identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum DigestParseError {
    /// The text is not exactly twice the identifier's width.
    #[error("expected {expected} hexadecimal characters, found {found}")]
    Length {
        /// How many characters the identifier's width requires.
        expected: usize,
        /// How many characters the text actually had.
        found: usize,
    },
    /// The text is the right length but is not hexadecimal.
    #[error(transparent)]
    Hex(#[from] wire::HexError),
    /// The identifier's width cannot be expressed as a character count.
    #[error("identifier width is too large to render as text")]
    WidthUnrepresentable,
}

impl DigestParseError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Length { .. } => "digest.length",
            Self::Hex(error) => error.code(),
            Self::WidthUnrepresentable => "digest.width",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod tests {
    use super::{Digest, DigestParseError};
    use crate::wire::HexError;
    use core::str::FromStr;

    #[test]
    fn renders_and_parses_back() {
        let digest = Digest::<4>::from_bytes([0x00, 0x0f, 0xa0, 0xff]);
        assert_eq!(digest.to_string(), "000fa0ff");
        assert_eq!(Digest::<4>::from_str("000fa0ff").unwrap(), digest);
    }

    #[test]
    fn rejects_uppercase() {
        assert_eq!(
            Digest::<4>::from_str("000FA0FF"),
            Err(DigestParseError::Hex(HexError::Charset))
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            Digest::<4>::from_str("000fa0"),
            Err(DigestParseError::Length {
                expected: 8,
                found: 6
            })
        );
    }

    #[test]
    fn rejects_non_hexadecimal() {
        assert_eq!(
            Digest::<4>::from_str("000fa0fz"),
            Err(DigestParseError::Hex(HexError::Charset))
        );
    }

    #[test]
    fn every_byte_value_survives_a_round_trip() {
        for value in 0_u8..=255 {
            let digest = Digest::<1>::from_bytes([value]);
            let parsed = Digest::<1>::from_str(&digest.to_string()).unwrap();
            assert_eq!(parsed, digest, "byte {value} did not survive");
        }
    }
}
