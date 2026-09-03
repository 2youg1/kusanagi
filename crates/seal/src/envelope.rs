// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Making a segment into bytes a host can hold but not read.
//!
//! What travels to a waypoint is the sealed form of a segment's canonical bytes —
//! **not** a segment with a sealed payload. The difference decides the whole
//! privacy claim: a segment carries its author's handle in the clear, so leaving
//! the envelope open would let a host group every drop by author and the
//! unlinkable addressing above it would buy nothing.
//!
//! What the host still learns is stated honestly: it sees an address, the time
//! of the request, and a length that is the same for every drop on the network.
//! Timing is the one left, and it is not this module's to close.

use chacha20poly1305::aead::{Aead as _, KeyInit as _};
use chacha20poly1305::{ChaCha20Poly1305, Nonce};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::veil::{DROP, pad, unpad};

/// The key and nonce for exactly one drop.
///
/// A key is derived per address and used for exactly one message, which is why a
/// derived nonce is sound here: the pair never repeats, so the catastrophic
/// nonce-reuse failure of this cipher is unreachable rather than avoided by
/// discipline. Anything that made a key cover two messages would break that, so
/// this type is deliberately not clonable and not constructible from outside.
///
/// It erases itself when it goes out of scope. A key that stays in freed memory
/// is a key in the next allocation, in a core dump, and in a swap file, and this
/// program hands 32-byte buffers around often enough that "it will be
/// overwritten soon" is not a claim anybody can check.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Key {
    bytes: [u8; 32],
    nonce: [u8; 12],
}

impl Key {
    /// Builds the key for one drop. Only `derive` calls this.
    pub(crate) const fn new(bytes: [u8; 32], nonce: [u8; 12]) -> Self {
        Self { bytes, nonce }
    }

    fn cipher(&self) -> Result<ChaCha20Poly1305, OpenFailed> {
        ChaCha20Poly1305::new_from_slice(&self.bytes).map_err(|_| OpenFailed::Unusable)
    }
}

impl core::fmt::Debug for Key {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Key(redacted)")
    }
}

/// Seals `plain` under `key`.
///
/// What comes back is always [`DROP`] bytes, whatever went in.
///
/// # Errors
///
/// [`OpenFailed::Unusable`] when the cipher refuses the key, and
/// [`OpenFailed::Oversize`] when `plain` does not fit one drop. Neither is
/// reachable for a segment, whose size `kernel` caps to exactly what fits — they
/// are returned rather than asserted away because an unreachable panic is still
/// a panic.
pub fn seal(key: &Key, plain: &[u8]) -> Result<Vec<u8>, OpenFailed> {
    let mut veiled = pad(plain)?;
    let sealed = key
        .cipher()?
        .encrypt(&Nonce::from(key.nonce), veiled.as_slice())
        .map_err(|_| OpenFailed::Rejected)?;
    // The padded body held a copy of the segment. Nothing downstream needs it,
    // and the allocation it sits in will be handed to somebody else.
    veiled.zeroize();
    Ok(sealed)
}

/// Opens what [`seal`] produced.
///
/// # Errors
///
/// [`OpenFailed::Rejected`] whenever the bytes were not sealed under this exact
/// key, whatever the reason — a wrong key, a flipped bit, a truncated body, or a
/// blob moved here from another address. They are one answer deliberately: an
/// attacker who learns *why* a forgery failed has been handed a way to test
/// guesses one at a time.
pub fn open(key: &Key, sealed: &[u8]) -> Result<Vec<u8>, OpenFailed> {
    if sealed.len() != DROP {
        return Err(OpenFailed::Rejected);
    }
    let mut veiled = key
        .cipher()?
        .decrypt(&Nonce::from(key.nonce), sealed)
        .map_err(|_| OpenFailed::Rejected)?;
    let plain = unpad(&veiled);
    veiled.zeroize();
    plain
}

/// Why sealed bytes did not open.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum OpenFailed {
    /// The bytes are not authentic under this key.
    #[error("these bytes were not sealed under this key")]
    Rejected,
    /// The cipher could not be built from this key.
    #[error("the key is not usable with this cipher suite")]
    Unusable,
    /// The bytes are larger than one drop can carry.
    ///
    /// Separate from [`Self::Rejected`] because it is not a forgery and telling
    /// the two apart hands an attacker nothing: it is this endpoint's own caller
    /// exceeding a limit its own `kernel` already enforces, so nobody on the far
    /// side can reach it.
    #[error("these bytes are larger than one drop carries")]
    Oversize,
}

impl OpenFailed {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Rejected => "seal.rejected",
            Self::Unusable => "seal.unusable",
            Self::Oversize => "seal.oversize",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]
mod tests {
    use super::{OpenFailed, open, seal};
    use crate::{Secret, derive};
    use kusanagi_kernel::Signer;

    fn keys(index: u64) -> super::Key {
        let secret = Secret::from_bytes([9_u8; 32]);
        derive(&secret.stream(&Signer::from_seed(&[1; 32]).handle()), index).1
    }

    #[test]
    fn a_sealed_message_opens_again() {
        let sealed = seal(&keys(0), b"the message").unwrap();
        assert_eq!(open(&keys(0), &sealed).unwrap(), b"the message");
    }

    #[test]
    fn the_sealed_form_does_not_contain_the_plain_form() {
        let sealed = seal(&keys(0), b"the message").unwrap();
        assert!(!sealed.windows(11).any(|w| w == b"the message"));
    }

    #[test]
    fn every_drop_is_the_same_size_whatever_it_carries() {
        // The assertion the whole envelope exists for. A host that can measure
        // an object learns nothing from measuring this one.
        for len in [0_usize, 1, 11, 512, 4_076] {
            let sealed = seal(&keys(0), &vec![3_u8; len]).unwrap();
            assert_eq!(
                sealed.len(),
                crate::DROP,
                "a payload of {len} bytes produced a drop of its own size"
            );
        }
    }

    #[test]
    fn bytes_that_are_not_one_drop_long_never_open() {
        let sealed = seal(&keys(0), b"the message").unwrap();
        let mut longer = sealed.clone();
        longer.push(0);
        assert_eq!(open(&keys(0), &longer), Err(OpenFailed::Rejected));
        assert_eq!(
            open(&keys(0), &sealed[..sealed.len() - 1]),
            Err(OpenFailed::Rejected)
        );
    }

    #[test]
    fn an_empty_message_survives() {
        let sealed = seal(&keys(0), b"").unwrap();
        assert_eq!(open(&keys(0), &sealed).unwrap(), b"");
    }

    #[test]
    fn another_drops_key_does_not_open_it() {
        let sealed = seal(&keys(0), b"the message").unwrap();
        assert_eq!(open(&keys(1), &sealed), Err(OpenFailed::Rejected));
    }

    #[test]
    fn every_flipped_byte_is_refused() {
        let sealed = seal(&keys(0), b"the message").unwrap();
        for at in 0..sealed.len() {
            let mut tampered = sealed.clone();
            tampered[at] ^= 0x01;
            assert_eq!(
                open(&keys(0), &tampered),
                Err(OpenFailed::Rejected),
                "a flip at byte {at} was accepted"
            );
        }
    }

    #[test]
    fn truncation_is_refused() {
        let sealed = seal(&keys(0), b"the message").unwrap();
        assert_eq!(
            open(&keys(0), &sealed[..sealed.len() - 1]),
            Err(OpenFailed::Rejected)
        );
        assert_eq!(open(&keys(0), &[]), Err(OpenFailed::Rejected));
    }

    #[test]
    fn bytes_that_are_the_right_length_and_nothing_else_never_open() {
        // A host that knows every drop is 4 096 bytes can manufacture one. What
        // it cannot do is make it open, and the answer must be the same refusal
        // a flipped bit gets — anything else is an oracle it can query.
        let mut invented = vec![0_u8; crate::DROP];
        for (at, byte) in invented.iter_mut().enumerate() {
            *byte = u8::try_from(at % 251).unwrap_or(0);
        }
        assert_eq!(open(&keys(0), &invented), Err(OpenFailed::Rejected));
        assert_eq!(
            open(&keys(0), &vec![0_u8; crate::DROP]),
            Err(OpenFailed::Rejected)
        );
        assert_eq!(
            open(&keys(0), &vec![0xff_u8; crate::DROP]),
            Err(OpenFailed::Rejected)
        );
    }

    #[test]
    fn the_same_text_at_two_heights_looks_different() {
        assert_ne!(
            seal(&keys(0), b"identical").unwrap(),
            seal(&keys(1), b"identical").unwrap()
        );
    }
}
