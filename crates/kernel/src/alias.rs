// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What an identity calls itself, signed so that the claim travels with the key.
//!
//! A handle is a hash and reads like one. An [`Alias`] is the word a person
//! chose, and a [`Declaration`] is that word signed by the key it belongs to, so
//! the same key reporting a name on five channels reports one name that every
//! reader can check — which is what lets a merged group thread say who spoke.
//!
//! **An alias never enters a payload.** It is metadata about the author,
//! exchanged once at introduction and rendered outside the fence that marks the
//! peer's own bytes; a name spliced into the text would be a label any peer
//! could forge in the peer's half of the answer.
//!
//! **An alias is one printable line.** No control character, no bidirectional
//! override, at most [`Alias::MOST`] bytes: it is shown beside program output
//! and must not be able to rewrite the line it is shown on.

use crate::identity::{Handle, Signature, Signer, VerifyingKey};
use crate::wire::Reader;

/// Domain separation for the signature over a declaration.
const NAME_DOMAIN: &[u8] = b"kusanagi/name/1";

/// How wide an ML-DSA-87 signature is on the wire.
const SIGNATURE: usize = 4_627;

/// A self-chosen name, fit to print beside program output.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Alias(String);

impl Alias {
    /// The most bytes an alias may have. One byte of length prefix, and room
    /// for a readable name in any script.
    pub const MOST: usize = 32;

    /// Accepts `text` as an alias when it is one printable line of at most
    /// [`Alias::MOST`] bytes.
    ///
    /// # Errors
    ///
    /// [`AliasError::Unfit`] with the reason, when it is not.
    pub fn new(text: &str) -> Result<Self, AliasError> {
        let unfit = |reason: &'static str| AliasError::Unfit { reason };
        if text.is_empty() {
            return Err(unfit("a name has at least one character"));
        }
        if text.len() > Self::MOST {
            return Err(unfit("a name is at most 32 bytes"));
        }
        if text.chars().any(|character| {
            character.is_control()
                || matches!(character, '\u{202A}'..='\u{202E}' | '\u{2066}'..='\u{2069}')
        }) {
            return Err(unfit(
                "a name is one printable line: no control character or bidirectional override",
            ));
        }
        if text.trim() != text {
            return Err(unfit("a name does not begin or end with whitespace"));
        }
        Ok(Self(text.to_owned()))
    }

    /// The name itself.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An alias signed by the key it is claimed for.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Declaration {
    alias: Alias,
    signature: Signature,
}

/// The bytes a declaration signs: domain, name, and the handle it is about.
fn claimed(alias: &Alias, handle: &Handle) -> Vec<u8> {
    let mut out = NAME_DOMAIN.to_vec();
    out.extend_from_slice(alias.as_str().as_bytes());
    out.extend_from_slice(handle.as_bytes());
    out
}

impl Declaration {
    /// Signs `alias` as the name of `signer`'s identity.
    #[must_use]
    pub fn sign(signer: &Signer, alias: Alias) -> Self {
        let signature = signer.sign(&claimed(&alias, &signer.handle()));
        Self { alias, signature }
    }

    /// The name, as claimed; [`Declaration::verify`] is what makes it believed.
    #[must_use]
    pub const fn alias(&self) -> &Alias {
        &self.alias
    }

    /// The wire form: `len u8 ‖ alias ‖ signature`.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let name = self.alias.as_str().as_bytes();
        // `Alias::new` bounds the length at 32, so the prefix always fits.
        let len = u8::try_from(name.len()).unwrap_or(u8::MAX);
        let mut out = vec![len];
        out.extend_from_slice(name);
        out.extend_from_slice(self.signature.as_bytes());
        out
    }

    /// Reads what [`Declaration::to_bytes`] wrote, without believing it.
    ///
    /// # Errors
    ///
    /// [`AliasError::Malformed`] when the bytes are not a declaration, and
    /// [`AliasError::Unfit`] when the name inside is not one an alias may be.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, AliasError> {
        let mut reader = Reader::new(bytes);
        let malformed = |_| AliasError::Malformed;
        let len = usize::from(reader.take_byte().map_err(malformed)?);
        let name = reader.take(len).map_err(malformed)?;
        let name = core::str::from_utf8(name).map_err(|_| AliasError::Malformed)?;
        let alias = Alias::new(name)?;
        let signature = Signature::from_bytes(reader.take_array::<SIGNATURE>().map_err(malformed)?);
        if reader.remaining() != 0 {
            return Err(AliasError::Malformed);
        }
        Ok(Self { alias, signature })
    }

    /// The name, once `key` is shown to have signed it for its own handle.
    ///
    /// # Errors
    ///
    /// [`AliasError::Forged`] when the signature is not `key`'s over this name
    /// and `key`'s handle — which is also what a declaration moved from one
    /// identity to another fails with.
    pub fn verify(&self, key: &VerifyingKey) -> Result<&Alias, AliasError> {
        key.verify(&claimed(&self.alias, &key.handle()), &self.signature)
            .map_err(|_| AliasError::Forged)?;
        Ok(&self.alias)
    }
}

/// Why a name was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum AliasError {
    /// The text is not one an alias may be.
    #[error("{reason}")]
    Unfit {
        /// Which rule it broke.
        reason: &'static str,
    },
    /// The bytes are not a declaration.
    #[error("these bytes are not a name declaration")]
    Malformed,
    /// The signature is not the named key's over this name.
    #[error("this name was not signed by the key it is claimed for")]
    Forged,
}
