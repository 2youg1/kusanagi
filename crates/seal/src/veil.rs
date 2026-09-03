// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One size for every drop this network will ever produce.
//!
//! Sealing hides what a segment says. It does not hide how much was said, and a
//! host needs no cryptanalysis to measure that — the length of an object is the
//! one fact a store cannot help learning. Two people whose drops are 61 bytes
//! and 4 003 bytes are two people having a particular conversation, and a
//! transcript's length profile survives being encrypted.
//!
//! So every sealed drop is exactly [`DROP`] bytes:
//!
//! ```text
//! length       4 bytes    big endian, how much of what follows is the segment
//! segment      N bytes    the canonical bytes
//! pad    131052-N bytes   zero
//!             +16 bytes   the authentication tag ChaCha20-Poly1305 appends
//! ```
//!
//! **One size, not a ladder of buckets.** A ladder names a bucket, and every
//! boundary in it is a parameter; two builds holding different parameters are
//! two distinguishable populations. One size has no boundary to sit near and
//! nothing to tune.
//!
//! **The pad is checked.** Unchecked padding is a covert channel: inside the
//! authenticated envelope, exactly as long as the message is short, and never
//! looked at again. A tampered build could carry an identity seed out through it
//! at a few kilobytes a message with every test in this workspace still green.
//! Refusing a non-zero pad costs one comparison.

use kusanagi_kernel::{MAX_SEGMENT, Reader};

use crate::envelope::OpenFailed;

/// The size of every sealed drop, in bytes.
///
/// Not a tunable. A build that changes it cannot exchange drops with a build
/// that has not, and two populations of different sizes are two populations a
/// host can tell apart, which is the leak this constant exists to close.
///
/// **128 KiB is derived rather than picked.** The largest artefact this protocol
/// can produce is an introduction: an eight-hop ML-DSA-87 grant is 58 345 bytes
/// and the newcomer's key another 2 592, under a genesis segment that spends
/// 4 704 on its fixed fields. Everything that can ever have to travel therefore
/// fits in one drop, so no artefact of this design needs chunking in order to
/// exist. 64 KiB fell 125 bytes short of that, which is the kind of margin that
/// becomes an outage rather than a warning.
///
/// The cost is paid deliberately: one drop is one message on the wire whatever
/// the message is, so a conversation that would have been many small objects is
/// a few large ones. Fewer objects is fewer things for a host to count (property
/// 3) and fewer requests for anybody on the path to time (property 4b), and the
/// bandwidth it spends is bandwidth the ruling in `ARCHITECTURE.md` §8 says to
/// spend.
pub const DROP: usize = 131_072;

/// What ChaCha20-Poly1305 appends to every ciphertext.
const TAG: usize = 16;

/// The plaintext the cipher covers: one drop, less the tag it will add.
const VEILED: usize = DROP - TAG;

/// The big-endian `u32` that says how much of a veiled body is the segment.
const LENGTH: usize = 4;

/// How much room a segment actually has.
const ROOM: usize = VEILED - LENGTH;

const _: () = assert!(
    ROOM == MAX_SEGMENT,
    "the drop envelope and the largest segment have drifted apart: a segment \
     that cannot fit in one drop would be refused at seal time, which is far \
     later than the rule that owns the limit"
);

/// Wraps `plain` into the one size every drop has.
///
/// # Errors
///
/// [`OpenFailed::Oversize`] when `plain` does not fit. Unreachable for a segment
/// — the assertion above ties this envelope to the limit `kernel` enforces — and
/// returned rather than asserted away, because an unreachable panic is still a
/// panic.
pub(crate) fn pad(plain: &[u8]) -> Result<Vec<u8>, OpenFailed> {
    let len = u32::try_from(plain.len()).map_err(|_| OpenFailed::Oversize)?;
    if plain.len() > ROOM {
        return Err(OpenFailed::Oversize);
    }
    let mut veiled = Vec::with_capacity(VEILED);
    veiled.extend_from_slice(&len.to_be_bytes());
    veiled.extend_from_slice(plain);
    veiled.resize(VEILED, 0);
    Ok(veiled)
}

/// Recovers what [`pad`] wrapped.
///
/// Three separate ways of being wrong arrive as one answer, for the reason the
/// rest of this crate gives: a body of the wrong size, a length that overruns
/// its own body, and a pad that is not zero are all [`OpenFailed::Rejected`].
///
/// # Errors
///
/// [`OpenFailed::Rejected`] whenever these are not the bytes [`pad`] produced.
pub(crate) fn unpad(veiled: &[u8]) -> Result<Vec<u8>, OpenFailed> {
    if veiled.len() != VEILED {
        return Err(OpenFailed::Rejected);
    }
    let mut reader = Reader::new(veiled);
    let declared = reader.take_u32().map_err(|_| OpenFailed::Rejected)?;
    let wanted = usize::try_from(declared).map_err(|_| OpenFailed::Rejected)?;
    let plain = reader
        .take(wanted)
        .map_err(|_| OpenFailed::Rejected)?
        .to_vec();
    let pad = reader
        .take(reader.remaining())
        .map_err(|_| OpenFailed::Rejected)?;
    if pad.iter().any(|byte| *byte != 0) {
        return Err(OpenFailed::Rejected);
    }
    Ok(plain)
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
    use super::{DROP, OpenFailed, ROOM, VEILED, pad, unpad};

    #[test]
    fn everything_wraps_to_one_size() {
        for len in [0, 1, 2, 63, 64, 1_000, ROOM - 1, ROOM] {
            let veiled = pad(&vec![7_u8; len]).unwrap();
            assert_eq!(veiled.len(), VEILED, "a payload of {len} bytes stood out");
        }
    }

    #[test]
    fn what_was_wrapped_comes_back() {
        for len in [0, 1, 141, ROOM] {
            let plain = vec![9_u8; len];
            assert_eq!(unpad(&pad(&plain).unwrap()).unwrap(), plain);
        }
    }

    #[test]
    fn one_byte_too_much_is_refused_here_rather_than_later() {
        assert_eq!(pad(&vec![0_u8; ROOM + 1]), Err(OpenFailed::Oversize));
    }

    #[test]
    fn a_pad_that_carries_anything_is_refused() {
        // The covert channel this closes: the byte is inside the authenticated
        // envelope, so no signature and no tag notices it, and nothing downstream
        // would ever have looked.
        let mut veiled = pad(b"a short message").unwrap();
        let last = veiled.len() - 1;
        veiled[last] = 1;
        assert_eq!(unpad(&veiled), Err(OpenFailed::Rejected));

        let mut veiled = pad(b"a short message").unwrap();
        veiled[100] = 0xff;
        assert_eq!(unpad(&veiled), Err(OpenFailed::Rejected));
    }

    #[test]
    fn a_length_that_overruns_its_own_body_is_refused() {
        let mut veiled = pad(b"short").unwrap();
        veiled[0..4].copy_from_slice(&u32::MAX.to_be_bytes());
        assert_eq!(unpad(&veiled), Err(OpenFailed::Rejected));
    }

    #[test]
    fn a_body_of_the_wrong_size_is_refused() {
        let veiled = pad(b"short").unwrap();
        assert_eq!(unpad(&veiled[..VEILED - 1]), Err(OpenFailed::Rejected));
        assert_eq!(unpad(&[]), Err(OpenFailed::Rejected));
    }

    #[test]
    fn the_envelope_is_a_whole_number_of_pages() {
        // A filesystem page, an object store part and a TLS record all divide
        // into this, so a drop is a whole number of something everywhere it is
        // handled.
        assert_eq!(DROP % 4_096, 0);
    }
}
