// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One hop of a delegation: who handed what to whom.
//!
//! A step is fixed-width, so a grant needs no length prefixes and a decoder needs
//! no arithmetic on attacker-supplied sizes.
//!
//! ```text
//! issuer      32 bytes  the verifying key that signed this step
//! subject     32 bytes  the handle it was signed over to
//! abilities    1 byte   the ability bitset
//! expires_at   8 bytes  big endian, seconds since the Unix epoch
//! has_parent   1 byte   0 for the root step, 1 otherwise
//! parent      32 bytes  the identifier of the step above; zeroes at the root
//! signature   64 bytes  by the issuer, over everything above
//! ```
//!
//! **A step carries the issuer's key and the subject's name, and the asymmetry
//! is the rule rather than an accident.** A grant has to convince somebody who
//! has never met either party — that is what an offline-verifiable credential
//! is — so each hop must carry enough to check its own signature. A subject
//! proves nothing here and is therefore only named; whoever presents the grant
//! presents their key alongside it, and
//! [`Grant::permits`](crate::Grant::permits) is where the two are made to agree.

use kusanagi_kernel::{Handle, Instant, Reader, Signature, Signer, VerifyingKey, identifier};

use crate::error::GrantError;
use crate::scope::{Abilities, Scope};

/// Domain separation for step identity.
const STEP_DOMAIN: &[u8] = b"kusanagi.grant.v1";

/// Domain separation for what an issuer signs.
const SIGNING_DOMAIN: &[u8] = b"kusanagi.grant.v1.sign";

/// The size of one encoded step, signature included.
pub(crate) const STEP_BYTES: usize = 170;

/// Everything a step is, except the signature over it.
const BODY_BYTES: usize = 106;

identifier! {
    /// The identity of one step, and the thing a revocation names.
    StepId, 32
}

/// One signed hop of a delegation.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Step {
    issuer: VerifyingKey,
    subject: Handle,
    scope: Scope,
    parent: Option<StepId>,
    signature: Signature,
}

impl Step {
    /// Signs a new step.
    pub(crate) fn sign(
        issuer: &Signer,
        subject: &Handle,
        scope: Scope,
        parent: Option<StepId>,
    ) -> Self {
        let body = body(&issuer.verifying_key(), subject, &scope, parent.as_ref());
        Self {
            issuer: issuer.verifying_key(),
            subject: *subject,
            scope,
            parent,
            signature: issuer.sign(&signed_bytes(&body)),
        }
    }

    /// Who signed this step.
    #[must_use]
    pub fn issuer(&self) -> Handle {
        self.issuer.handle()
    }

    /// The key that checks this step's signature.
    #[must_use]
    pub const fn issuer_key(&self) -> &VerifyingKey {
        &self.issuer
    }

    /// Who received it.
    #[must_use]
    pub const fn subject(&self) -> Handle {
        self.subject
    }

    /// What it conveys.
    #[must_use]
    pub const fn scope(&self) -> Scope {
        self.scope
    }

    /// The step directly above, absent only at the root.
    #[must_use]
    pub const fn parent(&self) -> Option<StepId> {
        self.parent
    }

    /// This step's identity, which is what a revocation names.
    #[must_use]
    pub fn id(&self) -> StepId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(STEP_DOMAIN);
        hasher.update(&self.to_bytes());
        StepId::from_bytes(*hasher.finalize().as_bytes())
    }

    /// Checks that the issuer really signed this step.
    pub(crate) fn check_signature(&self) -> Result<(), GrantError> {
        let body = body(
            &self.issuer,
            &self.subject,
            &self.scope,
            self.parent.as_ref(),
        );
        self.issuer
            .verify(&signed_bytes(&body), &self.signature)
            .map_err(|_| GrantError::NotAuthentic { step: self.id() })
    }

    /// The wire form.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = body(
            &self.issuer,
            &self.subject,
            &self.scope,
            self.parent.as_ref(),
        );
        out.extend_from_slice(self.signature.as_bytes());
        out
    }

    /// Reads one step off the front of `reader`.
    ///
    /// The signature is **not** checked here. Checking it needs no context, but
    /// reporting it does: a bad signature is only meaningful once the caller
    /// knows which position in which chain it sat at, so the check belongs to
    /// `Grant::verify` where that position is known.
    pub(crate) fn read(reader: &mut Reader<'_>) -> Result<Self, GrantError> {
        let issuer = VerifyingKey::from_bytes(reader.take_array::<32>()?);
        let subject = Handle::from_bytes(reader.take_array::<32>()?);
        let abilities = Abilities::from_bits(reader.take_byte()?)?;
        let expires_at = Instant::from_unix_seconds(reader.take_u64()?);
        let has_parent = reader.take_byte()?;
        let parent_bytes = reader.take_array::<32>()?;
        let parent = match has_parent {
            0 => None,
            1 => Some(StepId::from_bytes(parent_bytes)),
            other => return Err(GrantError::UnknownParentTag { tag: other }),
        };
        // A root step must not smuggle bytes in the parent field: they would
        // change the identifier without changing the meaning.
        if parent.is_none() && parent_bytes != [0_u8; 32] {
            return Err(GrantError::UnknownParentTag { tag: 0 });
        }
        let signature = Signature::from_bytes(reader.take_array::<64>()?);
        Ok(Self {
            issuer,
            subject,
            scope: Scope::new(abilities, expires_at),
            parent,
            signature,
        })
    }
}

fn body(
    issuer: &VerifyingKey,
    subject: &Handle,
    scope: &Scope,
    parent: Option<&StepId>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(BODY_BYTES);
    out.extend_from_slice(issuer.as_bytes());
    out.extend_from_slice(subject.as_bytes());
    out.push(scope.abilities().bits());
    out.extend_from_slice(&scope.expires_at().as_unix_seconds().to_be_bytes());
    match parent {
        None => {
            out.push(0);
            out.extend_from_slice(&[0_u8; 32]);
        }
        Some(parent) => {
            out.push(1);
            out.extend_from_slice(parent.as_bytes());
        }
    }
    out
}

fn signed_bytes(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIGNING_DOMAIN.len().saturating_add(body.len()));
    out.extend_from_slice(SIGNING_DOMAIN);
    out.extend_from_slice(body);
    out
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{BODY_BYTES, STEP_BYTES, Step};
    use crate::scope::{Abilities, Scope};
    use kusanagi_kernel::{Instant, Reader, Signer};

    #[test]
    fn the_wire_form_is_the_declared_width() {
        let issuer = Signer::from_seed(&[1; 32]);
        let step = Step::sign(
            &issuer,
            &Signer::from_seed(&[2; 32]).handle(),
            Scope::new(Abilities::ALL, Instant::from_unix_seconds(10)),
            None,
        );
        assert_eq!(step.to_bytes().len(), STEP_BYTES);
        assert_eq!(BODY_BYTES.saturating_add(64), STEP_BYTES);
    }

    #[test]
    fn a_step_round_trips() {
        let issuer = Signer::from_seed(&[1; 32]);
        let step = Step::sign(
            &issuer,
            &Signer::from_seed(&[2; 32]).handle(),
            Scope::new(Abilities::ALL, Instant::from_unix_seconds(10)),
            None,
        );
        let bytes = step.to_bytes();
        let decoded = Step::read(&mut Reader::new(&bytes)).unwrap();
        assert_eq!(decoded, step);
        assert_eq!(decoded.id(), step.id());
        assert_eq!(decoded.check_signature(), Ok(()));
    }
}
