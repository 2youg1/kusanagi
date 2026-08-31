// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Turning one shared secret into an unbounded supply of unrelated addresses.
//!
//! This is where the network's central privacy claim is actually made, so the
//! construction is stated plainly rather than hidden behind a helper:
//!
//! ```text
//! stream  = KDF("…stream…",  secret ‖ author)
//! address = KDF("…address…", stream ‖ index)   -> 20 bytes, public
//! key     = KDF("…key…",     stream ‖ index)   -> 32 + 12 bytes, private
//! ```
//!
//! Three consequences follow, and each is a requirement somewhere else:
//!
//! - **The host cannot link two drops.** Addresses are outputs of a keyed hash
//!   over a secret it does not hold, so two addresses of one conversation are
//!   two uniformly random 160-bit strings.
//! - **Nobody can write to you unintroduced.** Producing your address requires
//!   the secret, so unsolicited delivery is not filtered, it is uncomputable.
//! - **Two people sharing one secret do not collide.** Each author's lane is
//!   derived through their own handle, so both halves of a conversation can write
//!   at every height without ever contending for an address.
//!
//! The key derivation is BLAKE3's own `derive_key` mode. There is exactly one
//! hash primitive in this workspace; adding HKDF would add a second construction
//! to audit and buy nothing.

use kusanagi_kernel::{DropAddr, Handle};

use crate::envelope::Key;

/// Context strings for BLAKE3's key derivation mode.
///
/// The convention these follow is BLAKE3's own: a hard-coded, globally unique,
/// application-specific string that includes the date it was fixed. They are part
/// of the wire format — changing one changes every address and key derived
/// afterwards, and two endpoints that disagree on them cannot find each other at
/// all.
const STREAM_CONTEXT: &str = "kusanagi 2026-01-01 stream: one author's lane in a channel";
const ADDRESS_CONTEXT: &str = "kusanagi 2026-01-01 drop address";
const KEY_CONTEXT: &str = "kusanagi 2026-01-01 drop key and nonce";

/// The root secret two endpoints share.
///
/// Everything either of them can address or read follows from these 32 bytes, so
/// this is the whole of what an invitation hands over and the whole of what a
/// compromised endpoint gives away.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret([u8; 32]);

impl Secret {
    /// Wraps 32 bytes of shared secret.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The raw bytes, for the callers that must persist or transmit them.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The lane `author` writes on inside this channel.
    #[must_use]
    pub fn stream(&self, author: &Handle) -> Stream {
        let mut hasher = blake3::Hasher::new_derive_key(STREAM_CONTEXT);
        hasher.update(&self.0);
        hasher.update(author.as_bytes());
        Stream(*hasher.finalize().as_bytes())
    }
}

impl core::fmt::Debug for Secret {
    /// Never prints the bytes. A secret that appears in a log has stopped being
    /// one, and `{:?}` on an enclosing struct is how that usually happens.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Secret(redacted)")
    }
}

/// One author's sequence of drops inside a channel.
///
/// Derived rather than agreed: both endpoints compute both lanes from the shared
/// secret and the two public handles, so no negotiation is needed to know where
/// the other side writes.
#[derive(Clone, PartialEq, Eq)]
pub struct Stream([u8; 32]);

impl core::fmt::Debug for Stream {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Stream(redacted)")
    }
}

/// Derives the address and the key for one height of one stream.
///
/// The address and the key come from separate derivations rather than separate
/// halves of one output. Nothing is wrong with splitting one extendable output,
/// but the address is published to an untrusted host while the key must never be:
/// two contexts make that separation an argument about domain separation rather
/// than an argument about a hash's internals.
#[must_use]
pub fn derive(stream: &Stream, index: u64) -> (DropAddr, Key) {
    let mut address = [0_u8; 20];
    let mut hasher = blake3::Hasher::new_derive_key(ADDRESS_CONTEXT);
    hasher.update(&stream.0);
    hasher.update(&index.to_be_bytes());
    hasher.finalize_xof().fill(&mut address);

    let mut cipher_key = [0_u8; 32];
    let mut nonce = [0_u8; 12];
    let mut hasher = blake3::Hasher::new_derive_key(KEY_CONTEXT);
    hasher.update(&stream.0);
    hasher.update(&index.to_be_bytes());
    let mut output = hasher.finalize_xof();
    output.fill(&mut cipher_key);
    output.fill(&mut nonce);

    (DropAddr::from_bytes(address), Key::new(cipher_key, nonce))
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
    use super::{Secret, derive};
    use kusanagi_kernel::{DropAddr, Signer};
    use std::collections::BTreeSet;

    fn secret() -> Secret {
        Secret::from_bytes([9_u8; 32])
    }

    fn alice() -> Signer {
        Signer::from_seed(&[1_u8; 32])
    }

    fn bob() -> Signer {
        Signer::from_seed(&[2_u8; 32])
    }

    #[test]
    fn both_endpoints_compute_the_same_address() {
        let stream = secret().stream(&alice().handle());
        let (mine, _) = derive(&stream, 7);
        let (theirs, _) = derive(&secret().stream(&alice().handle()), 7);
        assert_eq!(mine, theirs);
    }

    #[test]
    fn each_height_gets_its_own_address() {
        let stream = secret().stream(&alice().handle());
        assert_ne!(derive(&stream, 0).0, derive(&stream, 1).0);
    }

    #[test]
    fn the_two_lanes_of_one_channel_never_collide() {
        let alice_lane = secret().stream(&alice().handle());
        let bob_lane = secret().stream(&bob().handle());
        for index in 0..64 {
            assert_ne!(derive(&alice_lane, index).0, derive(&bob_lane, index).0);
        }
    }

    #[test]
    fn a_different_secret_is_a_different_place_entirely() {
        let ours = secret().stream(&alice().handle());
        let theirs = Secret::from_bytes([8_u8; 32]).stream(&alice().handle());
        assert_ne!(derive(&ours, 0).0, derive(&theirs, 0).0);
    }

    #[test]
    fn a_thousand_addresses_are_a_thousand_addresses() {
        let stream = secret().stream(&alice().handle());
        let addresses: BTreeSet<DropAddr> = (0..1_000).map(|i| derive(&stream, i).0).collect();
        assert_eq!(addresses.len(), 1_000, "two heights shared an address");
    }

    #[test]
    fn a_secret_never_prints_itself() {
        assert_eq!(format!("{:?}", secret()), "Secret(redacted)");
        assert_eq!(
            format!("{:?}", secret().stream(&alice().handle())),
            "Stream(redacted)"
        );
    }
}
