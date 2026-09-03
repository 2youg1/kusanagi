// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who wrote something, who can prove it, and what the proof looks like.
//!
//! These three types are one concept and therefore one module. A [`Handle`] is a
//! public key; a [`Signer`] is the private half; a [`Signature`] is what the
//! second produces and the first checks. Splitting them across files would only
//! create a cycle between the files.
//!
//! A handle carries **no authority by itself**. It names a writer; it does not
//! prove one. The proof is a signature over the bytes in question, and every
//! [`Segment`](crate::Segment) in this network carries one.

use core::fmt;

use ed25519_dalek::{Signer as _, SigningKey, VerifyingKey};

use crate::identifier;

identifier! {
    /// The identity that authored something: an Ed25519 verifying key.
    ///
    /// Any 32 bytes can be spelled as a handle, and bytes that are not a valid
    /// key simply never verify anything. That is deliberate — parsing a handle is
    /// a text operation and stays infallible, while authenticity is decided at
    /// the one moment it matters, in [`Handle::verify`].
    Handle, 32
}

identifier! {
    /// An Ed25519 signature over some exact byte string.
    Signature, 64
}

impl Handle {
    /// Checks that `signature` covers `message` and was produced by this handle.
    ///
    /// # Errors
    ///
    /// [`NotAuthentic`] for every way this can fail — a wrong signature, a
    /// tampered message, or bytes that are not a key at all. The three are one
    /// answer on purpose: telling an attacker *which* part of a forgery failed is
    /// a service to the attacker.
    pub fn verify(&self, message: &[u8], signature: &Signature) -> Result<(), NotAuthentic> {
        let key = VerifyingKey::from_bytes(self.as_bytes()).map_err(|_| NotAuthentic)?;
        let signature = ed25519_dalek::Signature::from_bytes(signature.as_bytes());
        key.verify_strict(message, &signature)
            .map_err(|_| NotAuthentic)
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
    pub fn handle(&self) -> Handle {
        Handle::from_bytes(self.0.verifying_key().to_bytes())
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
    use super::{Handle, NotAuthentic, Signer};
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
    fn a_signature_verifies_under_its_own_handle() {
        let signer = alice();
        let signature = signer.sign(b"the message");
        assert_eq!(signer.handle().verify(b"the message", &signature), Ok(()));
    }

    #[test]
    fn a_tampered_message_does_not_verify() {
        let signer = alice();
        let signature = signer.sign(b"the message");
        assert_eq!(
            signer.handle().verify(b"the messagf", &signature),
            Err(NotAuthentic)
        );
    }

    #[test]
    fn another_handle_does_not_verify() {
        let signature = alice().sign(b"the message");
        let bob = Signer::from_seed(&[2_u8; 32]);
        assert_eq!(
            bob.handle().verify(b"the message", &signature),
            Err(NotAuthentic)
        );
    }

    #[test]
    fn bytes_that_are_not_a_key_verify_nothing() {
        let nonsense = Handle::from_bytes([0xff_u8; 32]);
        let signature = alice().sign(b"the message");
        assert_eq!(
            nonsense.verify(b"the message", &signature),
            Err(NotAuthentic)
        );
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
