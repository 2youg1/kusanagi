// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! A fixed-width opaque identifier and its one textual form.
//!
//! Several things in this network are fixed-width opaque identifiers of different
//! widths: a [`Handle`](crate::Handle), a [`SegmentId`](crate::SegmentId), a
//! [`DropAddr`](crate::DropAddr), and more to come. They share exactly one rule —
//! the text form is lowercase hexadecimal of the full width, and nothing else
//! parses. Keeping that rule in one place is why this type exists; it owns the
//! policy that an identifier has one spelling, not merely the syntax of printing
//! bytes.

use core::fmt;
use core::str::FromStr;

/// A fixed-width opaque identifier, rendered and parsed as lowercase hexadecimal.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest<const N: usize>([u8; N]);

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
        for byte in &self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
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
        let bytes = text.as_bytes();
        if bytes.len() != expected {
            return Err(DigestParseError::Length {
                expected,
                found: bytes.len(),
            });
        }

        let mut out = [0_u8; N];
        for (slot, pair) in out.iter_mut().zip(bytes.chunks_exact(2)) {
            let high = pair.first().and_then(|c| nibble(*c));
            let low = pair.get(1).and_then(|c| nibble(*c));
            *slot = high
                .zip(low)
                .and_then(|(high, low)| high.checked_mul(16)?.checked_add(low))
                .ok_or(DigestParseError::Charset)?;
        }
        Ok(Self(out))
    }
}

/// Decodes one lowercase hexadecimal character.
///
/// Uppercase is rejected rather than folded: an identifier with two spellings is
/// an identifier with two identities, and this network addresses by exact bytes.
fn nibble(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => character.checked_sub(b'0'),
        b'a'..=b'f' => character.checked_sub(b'a')?.checked_add(10),
        _ => None,
    }
}

/// Declares a newtype identifier over a [`Digest`] of the given width.
///
/// Every identifier in this network must render, parse, compare, and hash the
/// same way; three hand-written copies of those four impls is three chances for
/// one of them to drift. Each type still adds its own inherent methods below its
/// declaration — the macro supplies only what is common to all of them.
macro_rules! identifier {
    ($(#[$meta:meta])* $name:ident, $width:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
        pub struct $name($crate::digest::Digest<$width>);

        impl $name {
            /// Wraps the raw bytes.
            #[must_use]
            pub const fn from_bytes(bytes: [u8; $width]) -> Self {
                Self($crate::digest::Digest::from_bytes(bytes))
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
            type Err = $crate::digest::DigestParseError;

            fn from_str(text: &str) -> ::core::result::Result<Self, Self::Err> {
                text.parse().map($name)
            }
        }
    };
}

pub(crate) use identifier;

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
    /// The text contains something other than `0-9` or `a-f`.
    #[error("an identifier is lowercase hexadecimal; uppercase is not folded")]
    Charset,
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
            Self::Charset => "digest.charset",
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
            Err(DigestParseError::Charset)
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
            Err(DigestParseError::Charset)
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
