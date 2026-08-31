// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How bytes become text, and how bytes are read back without stepping off the end.
//!
//! Three things in this network are encoded by hand — identifiers, segments, and
//! grants — and every one of them is hashed or signed, so each needs exactly one
//! spelling and a decoder that cannot panic on hostile input. Both rules live
//! here rather than in three copies: a second hex parser is a second answer to
//! "is this the same identifier", and a second cursor is a second chance to index
//! past the end of a buffer somebody else supplied.

use core::fmt;

/// Bytes rendered as lowercase hexadecimal.
///
/// A view rather than a `String`, so a caller that is already writing into a
/// formatter does not allocate to append one field.
pub struct Hex<'a>(pub &'a [u8]);

impl fmt::Display for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Parses lowercase hexadecimal back into bytes.
///
/// # Errors
///
/// [`HexError::OddLength`] when the text cannot be split into byte pairs, and
/// [`HexError::Charset`] for anything outside `0-9a-f`. Uppercase is refused
/// rather than folded: a value with two spellings is a value with two identities,
/// and everything encoded this way is addressed by exact bytes.
pub fn unhex(text: &str) -> Result<Vec<u8>, HexError> {
    let bytes = text.as_bytes();
    let pairs = bytes.chunks_exact(2);
    if !pairs.remainder().is_empty() {
        return Err(HexError::OddLength { found: bytes.len() });
    }

    let mut out = Vec::with_capacity(pairs.len());
    for pair in pairs {
        let high = pair.first().and_then(|c| nibble(*c));
        let low = pair.get(1).and_then(|c| nibble(*c));
        let byte = high
            .zip(low)
            .and_then(|(high, low)| high.checked_mul(16)?.checked_add(low))
            .ok_or(HexError::Charset)?;
        out.push(byte);
    }
    Ok(out)
}

/// Decodes one lowercase hexadecimal character.
fn nibble(character: u8) -> Option<u8> {
    match character {
        b'0'..=b'9' => character.checked_sub(b'0'),
        b'a'..=b'f' => character.checked_sub(b'a')?.checked_add(10),
        _ => None,
    }
}

/// Why a piece of text is not hexadecimal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum HexError {
    /// The text does not divide into pairs of characters.
    #[error("hexadecimal comes in pairs; {found} character(s) do not divide")]
    OddLength {
        /// How many characters the text had.
        found: usize,
    },
    /// The text contains something other than `0-9` or `a-f`.
    #[error("hexadecimal here is lowercase; uppercase is not folded")]
    Charset,
}

impl HexError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::OddLength { .. } => "hex.odd_length",
            Self::Charset => "hex.charset",
        }
    }
}

/// A cursor that cannot walk off the end of its input.
///
/// Every decoder in this workspace reads bytes that arrived from somewhere
/// untrusted, so the cursor — not the decoder — owns the rule that a read past
/// the end is a named failure rather than a panic.
pub struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    /// Starts at the beginning of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    /// Takes the next `count` bytes.
    ///
    /// # Errors
    ///
    /// [`Incomplete`] when fewer than `count` bytes remain.
    pub fn take(&mut self, count: usize) -> Result<&'a [u8], Incomplete> {
        let end = self.at.checked_add(count).ok_or(Incomplete {
            wanted: count,
            had: self.remaining(),
        })?;
        let slice = self.bytes.get(self.at..end).ok_or(Incomplete {
            wanted: count,
            had: self.remaining(),
        })?;
        self.at = end;
        Ok(slice)
    }

    /// Takes the next `N` bytes as a fixed-width array.
    ///
    /// # Errors
    ///
    /// [`Incomplete`] when fewer than `N` bytes remain.
    pub fn take_array<const N: usize>(&mut self) -> Result<[u8; N], Incomplete> {
        let slice = self.take(N)?;
        <[u8; N]>::try_from(slice).map_err(|_| Incomplete {
            wanted: N,
            had: slice.len(),
        })
    }

    /// Takes the next byte.
    ///
    /// # Errors
    ///
    /// [`Incomplete`] when the input is exhausted.
    pub fn take_byte(&mut self) -> Result<u8, Incomplete> {
        let byte = self.take_array::<1>()?;
        byte.first()
            .copied()
            .ok_or(Incomplete { wanted: 1, had: 0 })
    }

    /// Takes the next eight bytes as a big-endian `u64`.
    ///
    /// # Errors
    ///
    /// [`Incomplete`] when fewer than eight bytes remain.
    pub fn take_u64(&mut self) -> Result<u64, Incomplete> {
        Ok(u64::from_be_bytes(self.take_array::<8>()?))
    }

    /// Takes the next four bytes as a big-endian `u32`.
    ///
    /// # Errors
    ///
    /// [`Incomplete`] when fewer than four bytes remain.
    pub fn take_u32(&mut self) -> Result<u32, Incomplete> {
        Ok(u32::from_be_bytes(self.take_array::<4>()?))
    }

    /// How many bytes are left.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }
}

/// The input ended in the middle of a field.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("needed {wanted} more byte(s) but only {had} remained")]
pub struct Incomplete {
    /// How many bytes the field required.
    pub wanted: usize,
    /// How many bytes were actually left.
    pub had: usize,
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{Hex, HexError, Reader, unhex};

    #[test]
    fn renders_and_parses_back() {
        let bytes = [0x00, 0x0f, 0xa0, 0xff];
        assert_eq!(Hex(&bytes).to_string(), "000fa0ff");
        assert_eq!(unhex("000fa0ff").unwrap(), bytes);
    }

    #[test]
    fn every_byte_value_survives_a_round_trip() {
        let all: Vec<u8> = (0_u8..=255).collect();
        assert_eq!(unhex(&Hex(&all).to_string()).unwrap(), all);
    }

    #[test]
    fn uppercase_is_refused() {
        assert_eq!(unhex("00FF"), Err(HexError::Charset));
    }

    #[test]
    fn an_odd_length_is_refused() {
        assert_eq!(unhex("000"), Err(HexError::OddLength { found: 3 }));
    }

    #[test]
    fn a_reader_stops_at_the_end() {
        let mut reader = Reader::new(&[1, 2, 3]);
        assert_eq!(reader.take_byte().unwrap(), 1);
        assert_eq!(reader.remaining(), 2);
        assert!(reader.take_array::<8>().is_err());
        // the failed read consumed nothing
        assert_eq!(reader.remaining(), 2);
    }
}
