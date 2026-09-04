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

    /// The bytes themselves, so that a test can say two keys differ.
    ///
    /// Test-only on purpose: nothing in this workspace has a reason to look at
    /// a key, and an accessor that existed in a release build would be the one
    /// way to get one out of this type.
    #[cfg(test)]
    pub(crate) const fn as_parts(&self) -> (&[u8; 32], &[u8; 12]) {
        (&self.bytes, &self.nonce)
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

/// How much room a sealed body takes.
///
/// **An enum rather than a flag, because the two are different promises.** A
/// veiled body is the same length for every message on the network, which is
/// what stops a host from reading the size of what was said; an exact one is as
/// long as what went in, which is only safe where nothing untrusted sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Padded to exactly [`DROP`]. Everything that reaches a host.
    Veil,
    /// Exactly as long as what went in. **Never sent anywhere.**
    ///
    /// A backup archive is the one thing sealed here that no host ever holds: it
    /// goes to a file the owner chose, and padding it to a multiple of a drop
    /// would make a site of three channels indistinguishable from one of six only
    /// to the person who already owns both.
    Exact,
}

/// Seals `plain` under `key`.
///
/// What comes back is [`DROP`] bytes under [`Fit::Veil`], and the length of
/// `plain` plus the cipher's tag under [`Fit::Exact`].
///
/// # Errors
///
/// [`OpenFailed::Unusable`] when the cipher refuses the key, and
/// [`OpenFailed::Oversize`] when `plain` does not fit one drop. Neither is
/// reachable for a segment, whose size `kernel` caps to exactly what fits — they
/// are returned rather than asserted away because an unreachable panic is still
/// a panic.
pub fn seal(key: &Key, fit: Fit, plain: &[u8]) -> Result<Vec<u8>, OpenFailed> {
    let mut body = match fit {
        Fit::Veil => pad(plain)?,
        Fit::Exact => plain.to_vec(),
    };
    let sealed = key
        .cipher()?
        .encrypt(&Nonce::from(key.nonce), body.as_slice())
        .map_err(|_| OpenFailed::Rejected)?;
    // The body held a copy of what was sealed. Nothing downstream needs it, and
    // the allocation it sits in will be handed to somebody else.
    body.zeroize();
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
pub fn open(key: &Key, fit: Fit, sealed: &[u8]) -> Result<Vec<u8>, OpenFailed> {
    if fit == Fit::Veil && sealed.len() != DROP {
        return Err(OpenFailed::Rejected);
    }
    let mut veiled = key
        .cipher()?
        .decrypt(&Nonce::from(key.nonce), sealed)
        .map_err(|_| OpenFailed::Rejected)?;
    let plain = match fit {
        Fit::Veil => unpad(&veiled),
        Fit::Exact => Ok(veiled.clone()),
    };
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
    use super::{Fit, OpenFailed, open, seal};
    use crate::{Secret, derive};
    use kusanagi_kernel::Signer;

    fn keys(index: u64) -> super::Key {
        let secret = Secret::from_bytes([9_u8; 32]);
        derive(&secret.stream(&Signer::from_seed(&[1; 32]).handle()), index).1
    }

    #[test]
    fn a_sealed_message_opens_again() {
        let sealed = seal(&keys(0), Fit::Veil, b"the message").unwrap();
        assert_eq!(open(&keys(0), Fit::Veil, &sealed).unwrap(), b"the message");
    }

    #[test]
    fn the_sealed_form_does_not_contain_the_plain_form() {
        let sealed = seal(&keys(0), Fit::Veil, b"the message").unwrap();
        assert!(!sealed.windows(11).any(|w| w == b"the message"));
    }

    #[test]
    fn every_drop_is_the_same_size_whatever_it_carries() {
        // The assertion the whole envelope exists for. A host that can measure
        // an object learns nothing from measuring this one.
        for len in [0_usize, 1, 11, 512, 4_076, 65_536] {
            let sealed = seal(&keys(0), Fit::Veil, &vec![3_u8; len]).unwrap();
            assert_eq!(
                sealed.len(),
                crate::DROP,
                "a payload of {len} bytes produced a drop of its own size"
            );
        }
    }

    #[test]
    fn bytes_that_are_not_one_drop_long_never_open() {
        let sealed = seal(&keys(0), Fit::Veil, b"the message").unwrap();
        let mut longer = sealed.clone();
        longer.push(0);
        assert_eq!(
            open(&keys(0), Fit::Veil, &longer),
            Err(OpenFailed::Rejected)
        );
        assert_eq!(
            open(&keys(0), Fit::Veil, &sealed[..sealed.len() - 1]),
            Err(OpenFailed::Rejected)
        );
    }

    #[test]
    fn an_empty_message_survives() {
        let sealed = seal(&keys(0), Fit::Veil, b"").unwrap();
        assert_eq!(open(&keys(0), Fit::Veil, &sealed).unwrap(), b"");
    }

    #[test]
    fn another_drops_key_does_not_open_it() {
        let sealed = seal(&keys(0), Fit::Veil, b"the message").unwrap();
        assert_eq!(
            open(&keys(1), Fit::Veil, &sealed),
            Err(OpenFailed::Rejected)
        );
    }

    /// A flip anywhere in a sealed drop is refused — sampled, not exhaustive.
    ///
    /// Every rejection costs one ChaCha20-Poly1305 pass over the whole drop, so
    /// visiting all `DROP` positions costs `DROP` squared and did not finish in
    /// half an hour of a debug build. The property is uniform over the
    /// ciphertext, because Poly1305 does not distinguish one offset from the
    /// next, so this walks the structural boundaries — the first bytes, the seam
    /// where the 16-byte tag begins, the last byte — and strides the rest with a
    /// prime step that aligns to no block. An AEAD that failed only at an
    /// unsampled offset does not exist; a test nobody runs does.
    #[test]
    fn every_flipped_byte_is_refused() {
        const TAG: usize = 16;
        let sealed = seal(&keys(0), Fit::Veil, b"the message").unwrap();
        let seam = sealed.len() - TAG;
        let sampled: Vec<usize> = (0..4)
            .chain((seam - 2)..sealed.len())
            .chain((0..sealed.len()).step_by(1_021))
            .collect();
        for at in sampled {
            let mut tampered = sealed.clone();
            tampered[at] ^= 0x01;
            assert_eq!(
                open(&keys(0), Fit::Veil, &tampered),
                Err(OpenFailed::Rejected),
                "a flip at byte {at} was accepted"
            );
        }
    }

    #[test]
    fn truncation_is_refused() {
        let sealed = seal(&keys(0), Fit::Veil, b"the message").unwrap();
        assert_eq!(
            open(&keys(0), Fit::Veil, &sealed[..sealed.len() - 1]),
            Err(OpenFailed::Rejected)
        );
        assert_eq!(open(&keys(0), Fit::Veil, &[]), Err(OpenFailed::Rejected));
    }

    #[test]
    fn bytes_that_are_the_right_length_and_nothing_else_never_open() {
        // A host that knows how large every drop is can manufacture one. What
        // it cannot do is make it open, and the answer must be the same refusal
        // a flipped bit gets — anything else is an oracle it can query.
        let mut invented = vec![0_u8; crate::DROP];
        for (at, byte) in invented.iter_mut().enumerate() {
            *byte = u8::try_from(at % 251).unwrap_or(0);
        }
        assert_eq!(
            open(&keys(0), Fit::Veil, &invented),
            Err(OpenFailed::Rejected)
        );
        assert_eq!(
            open(&keys(0), Fit::Veil, &vec![0_u8; crate::DROP]),
            Err(OpenFailed::Rejected)
        );
        assert_eq!(
            open(&keys(0), Fit::Veil, &vec![0xff_u8; crate::DROP]),
            Err(OpenFailed::Rejected)
        );
    }

    #[test]
    fn the_same_text_at_two_heights_looks_different() {
        assert_ne!(
            seal(&keys(0), Fit::Veil, b"identical").unwrap(),
            seal(&keys(1), Fit::Veil, b"identical").unwrap()
        );
    }
}
