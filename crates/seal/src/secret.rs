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

use kusanagi_kernel::{DropAddr, Handle, Signer, Trail};
use zeroize::{Zeroize as _, ZeroizeOnDrop};

use crate::envelope::Key;

/// The one drop an invitation points at, addressed by the secret alone.
///
/// Every other address on a channel is derived through an author's handle,
/// because both ends know both handles by then. **Here neither end knows the
/// other yet** — that is what the drop is for — so the channel secret is all
/// there is to derive from, and it is enough: producing this address requires
/// the secret, and the secret is what the invitation carries.
///
/// One address per channel, so writing a second offer to a live channel finds
/// the first one there. A host still learns nothing from it: it is a drop of the
/// same size as every other, at an address indistinguishable from any other.
#[must_use]
pub fn offer(secret: &Secret) -> (DropAddr, Key) {
    let mut address = [0_u8; 20];
    let mut hasher = blake3::Hasher::new_derive_key(OFFER_CONTEXT);
    hasher.update(secret.as_bytes());
    hasher.finalize_xof().fill(&mut address);

    let mut cipher_key = [0_u8; 32];
    let mut nonce = [0_u8; 12];
    let mut hasher = blake3::Hasher::new_derive_key(KEY_CONTEXT);
    hasher.update(&address);
    let mut output = hasher.finalize_xof();
    output.fill(&mut cipher_key);
    output.fill(&mut nonce);

    let key = Key::new(cipher_key, nonce);
    cipher_key.zeroize();
    nonce.zeroize();
    (DropAddr::from_bytes(address), key)
}

/// The key an archive is sealed under.
///
/// Not derived from a stream, because an archive is not at an address: it is a
/// file somebody keeps, and the only secret behind it is the recovery key its
/// owner wrote down. The nonce is the caller's because it must be fresh for
/// every archive — one recovery key seals many of them over a site's life.
///
/// The context string follows the same convention as the others below: one
/// global, dated purpose per string, so two keys derived from one secret never
/// collide.
#[must_use]
pub fn backup_key(recovery: &[u8; 32], nonce: [u8; 12]) -> Key {
    let mut cipher_key = blake3::derive_key(BACKUP_CONTEXT, recovery);
    let key = Key::new(cipher_key, nonce);
    cipher_key.zeroize();
    key
}

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
const TRAIL_CONTEXT: &str = "kusanagi 2026-01-01 trail seed for one lane";
const BACKUP_CONTEXT: &str = "kusanagi 2026-01-01 backup archive";
const OFFER_CONTEXT: &str = "kusanagi 2026-01-01 offer drop";

/// What an author signs once to obtain the seed of their trail on a lane.
///
/// Signed rather than derived from the channel secret alone, because the peer
/// holds that secret: a trail either end could compute would let either end
/// write the other's stream. `Signer::sign` is deterministic, so the seed is the
/// same on every run of every process — which is what keeps law 1 true, since
/// nothing about a trail is ever written down.
const TRAIL_DOMAIN: &[u8] = b"kusanagi.trail.seed.v1";

/// The root secret two endpoints share.
///
/// Everything either of them can address or read follows from these 32 bytes, so
/// this is the whole of what an invitation hands over and the whole of what a
/// compromised endpoint gives away.
///
/// **Deliberately not comparable.** Two secrets are never compared anywhere in
/// this workspace, and a derived `PartialEq` would compare them in a time that
/// depends on how many leading bytes match — the shape of an oracle. Removing
/// the trait makes the mistake unwritable instead of documenting it.
///
/// It erases itself on the way out, so a secret does not outlive the value that
/// held it in freed memory, a core dump or a swap file.
#[derive(Clone, ZeroizeOnDrop)]
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
        let mut derived = *hasher.finalize().as_bytes();
        let stream = Stream(derived);
        derived.zeroize();
        stream
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
///
/// Not comparable and self-erasing, for the reasons [`Secret`] is not and is.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Stream([u8; 32]);

impl Stream {
    /// The trail this author uses on this lane.
    ///
    /// Two properties, and the design needs both. **Only the author can compute
    /// it**, because it starts from a signature only they can make — the channel
    /// secret is shared, so a seed derived from that alone would let a peer write
    /// segments in the author's name. And **it is the same on every run**,
    /// because `Signer::sign` is deterministic, so an endpoint that was killed
    /// mid-conversation recomputes the identical trail rather than losing the
    /// ability to continue its own stream. That second property is a choice
    /// rather than a gift: FIPS 204 permits a randomised signature, and
    /// `kernel::Signer` takes the standard's deterministic variant precisely so
    /// that this holds.
    #[must_use]
    pub fn trail(&self, author: &Signer) -> Trail {
        let mut message = TRAIL_DOMAIN.to_vec();
        message.extend_from_slice(&self.0);
        let signature = author.sign(&message);
        message.zeroize();

        let mut hasher = blake3::Hasher::new_derive_key(TRAIL_CONTEXT);
        hasher.update(signature.as_bytes());
        let mut seed = *hasher.finalize().as_bytes();
        let trail = Trail::from_seed(seed);
        seed.zeroize();
        trail
    }
}

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

    let key = Key::new(cipher_key, nonce);
    cipher_key.zeroize();
    nonce.zeroize();
    (DropAddr::from_bytes(address), key)
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

    /// A trail is the author's alone, and the same on every run.
    ///
    /// The second half is what makes law 1 survive: nothing about a trail is
    /// written to disk, so a process that is killed must be able to recompute it
    /// exactly, or the endpoint would lose the ability to extend its own stream.
    #[test]
    fn a_trail_belongs_to_one_author_on_one_lane_and_survives_a_kill() {
        let lane = secret().stream(&alice().handle());
        assert_eq!(
            lane.trail(&alice()).reveal(3),
            lane.trail(&alice()).reveal(3)
        );
        assert_ne!(lane.trail(&alice()).reveal(3), lane.trail(&bob()).reveal(3));

        // The peer holds the channel secret and can derive this very lane, so a
        // seed that came from the secret alone would hand them the author's
        // stream. It does not: the signature is the author's.
        let same_lane_other_author = secret().stream(&alice().handle());
        assert_ne!(
            lane.trail(&alice()).reveal(3),
            same_lane_other_author.trail(&bob()).reveal(3)
        );

        // And a different lane of the same channel is a different trail, so a
        // reveal published on one lane authenticates nothing on the other.
        assert_ne!(
            lane.trail(&alice()).reveal(3),
            secret().stream(&bob().handle()).trail(&alice()).reveal(3)
        );
    }

    #[test]
    fn a_secret_never_prints_itself() {
        assert_eq!(format!("{:?}", secret()), "Secret(redacted)");
        assert_eq!(
            format!("{:?}", secret().stream(&alice().handle())),
            "Stream(redacted)"
        );
    }

    /// Compiles only while every type that holds key material erases itself.
    ///
    /// A `Drop` implementation cannot be observed from safe Rust — reading the
    /// bytes after the value is gone is exactly the undefined behaviour this
    /// workspace forbids — so what is asserted is the bound. Losing the derive is
    /// then a compile error rather than a silent change in what a core dump
    /// contains.
    #[test]
    fn every_secret_erases_itself() {
        const fn erases<T: zeroize::ZeroizeOnDrop>() {}
        erases::<Secret>();
        erases::<super::Stream>();
        erases::<crate::Key>();
    }
}
