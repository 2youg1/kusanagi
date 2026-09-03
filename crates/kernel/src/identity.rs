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
//! the scheme behind it is Ed25519 or ML-DSA-44, so addresses, cairn filenames
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

use ed25519_dalek::{Signer as _, SigningKey};

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
    /// An Ed25519 signature over some exact byte string.
    Signature, 64
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
    pub const WIDTH: usize = 32;

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
        let key = ed25519_dalek::VerifyingKey::from_bytes(&self.0).map_err(|_| NotAuthentic)?;
        let signature = ed25519_dalek::Signature::from_bytes(signature.as_bytes());
        key.verify_strict(message, &signature)
            .map_err(|_| NotAuthentic)
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
/// Holds a 32-byte seed and nothing else, so an endpoint's whole identity is 32
/// bytes on disk. It deliberately does not implement `Clone`: a signing key that
/// is easy to copy is a signing key that ends up in two places.
pub struct Signer(SigningKey);

impl Signer {
    /// Rebuilds a signer from the seed it was created with.
    #[must_use]
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self(SigningKey::from_bytes(seed))
    }

    /// The public half.
    #[must_use]
    pub fn verifying_key(&self) -> VerifyingKey {
        VerifyingKey::from_bytes(self.0.verifying_key().to_bytes())
    }

    /// The name the public half answers to.
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.verifying_key().handle()
    }

    /// Signs `message`.
    #[must_use]
    pub fn sign(&self, message: &[u8]) -> Signature {
        Signature::from_bytes(self.0.sign(message).to_bytes())
    }
}

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
        let nonsense = VerifyingKey::from_bytes([0xff_u8; 32]);
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
        assert_ne!(key.handle().as_bytes(), key.as_bytes());
        assert_eq!(
            key.handle(),
            VerifyingKey::from_bytes(*key.as_bytes()).handle()
        );
        assert_ne!(key.handle(), VerifyingKey::from_bytes([7_u8; 32]).handle());
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

    #[test]
    fn a_signer_never_prints_its_seed() {
        let signer = alice();
        let printed = format!("{signer:?}");
        assert!(printed.contains(&signer.handle().to_string()));
        assert!(!printed.contains("01010101"));
    }
}
