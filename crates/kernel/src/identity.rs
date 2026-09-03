// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who wrote something, who can prove it, and what the proof looks like.
//!
//! Four types, one concept, therefore one module. A [`Handle`] is a **name**; a
//! [`VerifyingKey`] is what checks a signature made under that name; a
//! [`Signer`] is the private half; a [`Signature`] is what the third produces
//! and the second checks. Splitting them across files would only create a cycle
//! between the files.
//!
//! **A handle is the hash of a key, not the key.** That is what keeps the width
//! of a signature scheme out of the wire format: a handle is 32 bytes whether
//! the scheme behind it is Ed25519 or ML-DSA-87, so addresses, cairn filenames
//! and the author field of a segment do not move when the scheme does. The price
//! is that a name cannot check anything — whoever verifies a signature must hold
//! the key, and where each caller gets one is stated where it gets it: a
//! `kusanagi_grant::Grant` carries the issuer's key in every step, because a
//! credential has to convince a stranger, and a channel record on disk carries
//! the peer's key, because a stream only has to convince the one endpoint that
//! was introduced to its author.
//!
//! A handle carries **no authority by itself**. It names a writer; it does not
//! prove one. The proof is a signature over the bytes in question, and every
//! [`Segment`](crate::Segment) in this network carries one.

use core::fmt;

use fips204::ml_dsa_87;
use fips204::traits::{KeyGen as _, SerDes as _, Signer as _, Verifier as _};
use zeroize::ZeroizeOnDrop;

use crate::identifier;

/// Domain separation for the one mapping from a key to a name.
///
/// A plain hash rather than BLAKE3's `derive_key` mode: a handle is a public
/// identifier that goes into filenames and reports, not key material, and the
/// contexts reserved for derivation are the ones in `kusanagi_seal`.
const HANDLE_DOMAIN: &[u8] = b"kusanagi.handle.v1";

identifier! {
    /// The name an identity answers to: `BLAKE3("kusanagi.handle.v1" ‖ key)`.
    ///
    /// Any 32 bytes can be spelled as a handle, and one that names nobody simply
    /// never matches a key. That is deliberate — parsing a handle is a text
    /// operation and stays infallible.
    ///
    /// **There is no `verify` here, and its absence is the point.** A name cannot
    /// check a signature, so the question "signed by whom?" cannot be answered
    /// without naming whose key is expected, and a caller that never says who it
    /// expects no longer compiles.
    Handle, 32
}

identifier! {
    /// An ML-DSA-87 signature over some exact byte string.
    Signature, 4627
}

/// The public half of an identity: the only thing that can check a signature.
///
/// This is the one type in the workspace whose width follows the signature
/// scheme, which is why so little carries it. It travels where a signature must
/// be checked and nowhere else; everywhere else an identity appears as its
/// [`Handle`].
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct VerifyingKey([u8; VerifyingKey::WIDTH]);

impl VerifyingKey {
    /// How many bytes one key occupies on the wire and on disk.
    ///
    /// Public because the formats that embed a key — a grant step, a channel
    /// record, a greeting — are fixed-width and must agree with this number
    /// rather than repeat it.
    pub const WIDTH: usize = ml_dsa_87::PK_LEN;

    /// Wraps the raw bytes.
    ///
    /// Infallible, and bytes that are not a point on the curve are accepted here
    /// and refused in [`VerifyingKey::verify`]. Validity is decided at the one
    /// moment it matters rather than at every place a key is read off a wire.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; Self::WIDTH]) -> Self {
        Self(bytes)
    }

    /// Borrows the raw bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::WIDTH] {
        &self.0
    }

    /// The name this key answers to.
    #[must_use]
    pub fn handle(&self) -> Handle {
        let mut hasher = blake3::Hasher::new();
        hasher.update(HANDLE_DOMAIN);
        hasher.update(&self.0);
        Handle::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Checks that `signature` covers `message` and was produced by this key.
    ///
    /// # Errors
    ///
    /// [`NotAuthentic`] for every way this can fail — a wrong signature, a
    /// tampered message, or bytes that are not a key at all. The three are one
    /// answer on purpose: telling an attacker *which* part of a forgery failed is
    /// a service to the attacker.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), NotAuthentic> {
        let key = ml_dsa_87::PublicKey::try_from_bytes(self.0).map_err(|_| NotAuthentic)?;
        if key.verify(message, signature.as_bytes(), EMPTY_CONTEXT) {
            Ok(())
        } else {
            Err(NotAuthentic)
        }
    }
}

impl fmt::Debug for VerifyingKey {
    /// Prints the handle. A key is public, but a reader comparing two of them
    /// wants the name they share with every other mention in a report.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "VerifyingKey({})", self.handle())
    }
}

/// The private half of an identity.
///
/// Holds the 32-byte seed an ML-DSA-44 key pair expands from, so an endpoint's
/// whole identity is still 32 bytes on disk however large the expanded key is.
/// It deliberately does not implement `Clone`: a signing key that is easy to
/// copy is a signing key that ends up in two places.
#[derive(ZeroizeOnDrop)]
pub struct Signer {
    seed: [u8; 32],
    /// Boxed because ML-DSA-87 expands a 32-byte seed into 4 896 bytes, and a
    /// value that large moves every time a signer is returned, passed or
    /// rebound. The heap holds it once and the moves cost a pointer.
    #[zeroize(skip)]
    key: Box<ml_dsa_87::PrivateKey>,
    /// Cached for the same reason and one more: deriving it means running key
    /// generation again, which is the most expensive thing this type does.
    #[zeroize(skip)]
    public: Box<VerifyingKey>,
}

impl Signer {
    /// Rebuilds a signer from the seed it was created with.
    ///
    /// Expansion is deterministic, which is what lets an identity file hold 32
    /// bytes rather than the 4 896 the scheme actually signs with.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        let (public, key) = ml_dsa_87::KG::keygen_from_seed(seed);
        Self {
            seed: *seed,
            key: Box::new(key),
            public: Box::new(VerifyingKey::from_bytes(public.into_bytes())),
        }
    }

    /// A 32-byte secret this identity alone can compute, bound to `context`.
    ///
    /// The one way anything derives from the identity seed without being handed
    /// it. `kusanagi_seal::Stream::trail` is the caller: a trail must be the
    /// author's alone, because the peer holds the channel secret, and identical
    /// on every run, because nothing about a trail is written down. Deriving it
    /// from a signature would tie that determinism to whether the signature
    /// scheme happens to be deterministic, which ML-DSA-87 is only when asked.
    #[must_use]
    pub fn derive(&self, context: &str, bound_to: &[u8]) -> [u8; 32] {
        let mut hasher = blake3::Hasher::new_derive_key(context);
        hasher.update(&self.seed);
        hasher.update(bound_to);
        *hasher.finalize().as_bytes()
    }

    /// The public half.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        *self.public
    }

    /// The name the public half answers to.
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.verifying_key().handle()
    }

    /// Signs `message`.
    ///
    /// Deterministic: the hedging randomness FIPS 204 allows is fixed at zero,
    /// which is the standard's own deterministic variant. Two builds signing one
    /// message produce one signature, so a segment has one spelling and the
    /// canonical-bytes rule survives a signature scheme that would otherwise be
    /// free to vary.
    ///
    /// # Panics
    ///
    /// Never. The only failure `try_sign_with_seed` reports is a malformed
    /// private key, which cannot arise from a key this type expanded itself, and
    /// the branch returns an all-zero signature rather than panicking — a
    /// signature that verifies under nothing.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> Signature {
        let signed = self
            .key
            .try_sign_with_seed(&DETERMINISTIC, message, EMPTY_CONTEXT)
            .unwrap_or([0_u8; ml_dsa_87::SIG_LEN]);
        Signature::from_bytes(signed)
    }
}

/// FIPS 204 allows a signature to be randomised or deterministic. This is the
/// deterministic choice, and it is the one the canonical-bytes rule needs.
const DETERMINISTIC: [u8; 32] = [0_u8; 32];

/// This network signs domain-separated messages of its own, so the scheme's
/// application context stays empty rather than becoming a second place where
/// domain separation could disagree with itself.
const EMPTY_CONTEXT: &[u8] = &[];

impl fmt::Debug for Signer {
    /// Prints the public half only. A `Debug` that printed the seed would put a
    /// private key into every log line that formats a struct containing one.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Signer({})", self.handle())
    }
}

/// A signature did not check out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the signature was not produced by this handle over these bytes")]
pub struct NotAuthentic;

impl NotAuthentic {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "identity.not_authentic"
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
    use super::{Handle, NotAuthentic, Signer, VerifyingKey};
    use core::str::FromStr;

    fn alice() -> Signer {
        Signer::from_seed(&[1_u8; 32])
    }

    #[test]
    fn a_seed_determines_the_identity() {
        assert_eq!(alice().handle(), Signer::from_seed(&[1_u8; 32]).handle());
        assert_ne!(alice().handle(), Signer::from_seed(&[2_u8; 32]).handle());
    }

    #[test]
    fn a_signature_verifies_under_its_own_key() {
        let signer = alice();
        let signature = signer.sign(b"the message");
        assert_eq!(
            signer.verifying_key().verify(b"the message", &signature),
            Ok(())
        );
    }

    #[test]
    fn a_tampered_message_does_not_verify() {
        let signer = alice();
        let signature = signer.sign(b"the message");
        assert_eq!(
            signer.verifying_key().verify(b"the messagf", &signature),
            Err(NotAuthentic)
        );
    }

    #[test]
    fn another_key_does_not_verify() {
        let signature = alice().sign(b"the message");
        let bob = Signer::from_seed(&[2_u8; 32]);
        assert_eq!(
            bob.verifying_key().verify(b"the message", &signature),
            Err(NotAuthentic)
        );
    }

    #[test]
    fn bytes_that_are_not_a_key_verify_nothing() {
        let nonsense = VerifyingKey::from_bytes([0xff_u8; VerifyingKey::WIDTH]);
        let signature = alice().sign(b"the message");
        assert_eq!(
            nonsense.verify(b"the message", &signature),
            Err(NotAuthentic)
        );
    }

    /// The load-bearing one: a handle must not be the key wearing a new name.
    ///
    /// If it were, every address, cairn filename and segment in the network
    /// would widen the day the signature scheme changes — which is the cost this
    /// split exists to avoid, and the kind of cost that is only noticed once it
    /// has already been paid everywhere.
    #[test]
    fn a_handle_is_the_hash_of_a_key_and_not_the_key() {
        let key = alice().verifying_key();
        assert_ne!(
            key.handle().as_bytes().as_slice(),
            key.as_bytes().as_slice()
        );
        assert_eq!(
            key.handle(),
            VerifyingKey::from_bytes(*key.as_bytes()).handle()
        );
        assert_ne!(
            key.handle(),
            VerifyingKey::from_bytes([7_u8; VerifyingKey::WIDTH]).handle()
        );
    }

    #[test]
    fn a_key_prints_its_handle_rather_than_itself() {
        let key = alice().verifying_key();
        let printed = format!("{key:?}");
        assert!(printed.contains(&key.handle().to_string()));
        assert!(!printed.contains(&crate::Hex(key.as_bytes()).to_string()));
    }

    #[test]
    fn a_handle_survives_a_text_round_trip() {
        let handle = alice().handle();
        assert_eq!(Handle::from_str(&handle.to_string()).unwrap(), handle);
        assert_eq!(handle.to_string().len(), 64);
    }

    /// The seed on disk stays 32 bytes even though the scheme signs with 2 560.
    #[test]
    fn an_identity_is_still_thirty_two_bytes_on_disk() {
        assert_eq!(alice().verifying_key().as_bytes().len(), 2592);
        assert_eq!(alice().sign(b"x").as_bytes().len(), 4627);
        assert_eq!(alice().handle().as_bytes().len(), 32);
    }

    /// Two runs sign one message identically, which the canonical bytes need.
    #[test]
    fn signing_is_deterministic() {
        assert_eq!(alice().sign(b"the message"), alice().sign(b"the message"));
        assert_ne!(alice().sign(b"the message"), alice().sign(b"another"));
    }

    #[test]
    fn a_derived_secret_is_this_identity_alone_and_the_same_every_run() {
        let mine = alice().derive("kusanagi test context", b"lane");
        assert_eq!(mine, alice().derive("kusanagi test context", b"lane"));
        assert_ne!(mine, alice().derive("kusanagi test context", b"other lane"));
        assert_ne!(
            mine,
            Signer::from_seed(&[2_u8; 32]).derive("kusanagi test context", b"lane")
        );
    }

    #[test]
    fn a_signer_never_prints_its_seed() {
        let signer = alice();
        let printed = format!("{signer:?}");
        assert!(printed.contains(&signer.handle().to_string()));
        assert!(!printed.contains("01010101"));
    }
}
