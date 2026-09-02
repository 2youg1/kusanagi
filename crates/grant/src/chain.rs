// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A grant: a chain of signed steps that only ever narrows.

use kusanagi_kernel::{Handle, Instant, Reader, Signer};

use crate::error::GrantError;
use crate::revocation::Revocations;
use crate::scope::{Ability, Scope};
use crate::step::{STEP_BYTES, Step};

/// How many hops a delegation may take.
///
/// Eight is a bound on verification work and on the size of an invitation, not a
/// statement about organisations. A chain that needs more hops than this is
/// describing a structure that should have been a second grant from the root.
pub const MAX_STEPS: usize = 8;

/// An offline-verifiable authorisation that can only be narrowed.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grant {
    steps: Vec<Step>,
}

impl Grant {
    /// Issues a fresh grant from a root authority.
    ///
    /// This is the only way a scope enters the system without being met against
    /// something narrower, which is why it takes a `Signer`: the root's key is the
    /// only authority for what the root is willing to permit.
    #[must_use]
    pub fn issue(root: &Signer, subject: &Handle, scope: Scope) -> Self {
        Self {
            steps: vec![Step::sign(root, subject, scope, None)],
        }
    }

    /// Hands this grant onward, no wider than it already is.
    ///
    /// The delegated scope is `held.meet(request)`. A caller asking for more than
    /// it holds is not an error to report but a request to clamp — there is no
    /// path through this function that produces a scope outside the one above it,
    /// so widening is not refused, it is unrepresentable.
    ///
    /// # Errors
    ///
    /// [`GrantError::NotYours`] when `holder` is not the handle this grant was
    /// last issued to, and [`GrantError::TooLong`] at the hop limit.
    pub fn attenuate(
        &self,
        holder: &Signer,
        subject: &Handle,
        request: Scope,
    ) -> Result<Self, GrantError> {
        let last = self.last()?;
        if holder.handle() != last.subject() {
            return Err(GrantError::NotYours);
        }
        let next = self.steps.len().saturating_add(1);
        if next > MAX_STEPS {
            return Err(GrantError::TooLong {
                count: next,
                limit: MAX_STEPS,
            });
        }

        let narrowed = last.scope().meet(&request);
        let mut steps = self.steps.clone();
        steps.push(Step::sign(holder, subject, narrowed, Some(last.id())));
        Ok(Self { steps })
    }

    /// The handle that signed the first step.
    ///
    /// # Errors
    ///
    /// [`GrantError::Empty`] when there are no steps.
    pub fn root(&self) -> Result<Handle, GrantError> {
        Ok(self.steps.first().ok_or(GrantError::Empty)?.issuer())
    }

    /// The handle the last step was issued to.
    ///
    /// # Errors
    ///
    /// [`GrantError::Empty`] when there are no steps.
    pub fn holder(&self) -> Result<Handle, GrantError> {
        Ok(self.last()?.subject())
    }

    /// How many hops this grant has taken.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.steps.len()
    }

    /// Every step, root first.
    #[must_use]
    pub fn steps(&self) -> &[Step] {
        &self.steps
    }

    fn last(&self) -> Result<&Step, GrantError> {
        self.steps.last().ok_or(GrantError::Empty)
    }

    /// Checks the whole chain and reports what it conveys.
    ///
    /// The order of the checks is the order in which a failure is worth
    /// reporting: structure before cryptography before policy, so the message a
    /// caller sees names the earliest thing that is actually wrong.
    ///
    /// # Errors
    ///
    /// One variant of [`GrantError`] per way a chain can fail, each naming where.
    pub fn verify(
        &self,
        root: &Handle,
        now: Instant,
        revoked: &Revocations,
    ) -> Result<Scope, GrantError> {
        if self.steps.is_empty() {
            return Err(GrantError::Empty);
        }
        if self.steps.len() > MAX_STEPS {
            return Err(GrantError::TooLong {
                count: self.steps.len(),
                limit: MAX_STEPS,
            });
        }

        let mut above: Option<&Step> = None;
        for (at, step) in self.steps.iter().enumerate() {
            match (above, step.parent()) {
                (None, None) => {
                    if step.issuer() != *root {
                        return Err(GrantError::WrongRoot {
                            expected: *root,
                            found: step.issuer(),
                        });
                    }
                }
                (None, Some(_)) | (Some(_), None) => return Err(GrantError::Detached { at }),
                (Some(parent), Some(claimed)) => {
                    if claimed != parent.id() {
                        return Err(GrantError::Detached { at });
                    }
                    if step.issuer() != parent.subject() {
                        return Err(GrantError::IssuerMismatch { at });
                    }
                    if !step.scope().is_within(&parent.scope()) {
                        return Err(GrantError::Widened { at });
                    }
                }
            }

            step.check_signature()?;
            let id = step.id();
            if revoked.contains(&id) {
                return Err(GrantError::Revoked { step: id });
            }
            above = Some(step);
        }

        let scope = self.last()?.scope();
        if !scope.is_live_at(now) {
            return Err(GrantError::Expired {
                expired_at: scope.expires_at(),
                now,
            });
        }
        Ok(scope)
    }

    /// Checks that `presenter` may do `ability` under this grant right now.
    ///
    /// This is the question every caller actually has, so it is one call rather
    /// than four that a caller could get out of order or forget.
    ///
    /// # Errors
    ///
    /// Everything [`Grant::verify`] reports, plus
    /// [`GrantError::NotTheHolder`] and [`GrantError::Forbidden`].
    pub fn permits(
        &self,
        root: &Handle,
        presenter: &Handle,
        ability: Ability,
        now: Instant,
        revoked: &Revocations,
    ) -> Result<(), GrantError> {
        let scope = self.verify(root, now, revoked)?;
        let holder = self.holder()?;
        if holder != *presenter {
            return Err(GrantError::NotTheHolder {
                holder,
                presenter: *presenter,
            });
        }
        if !scope.abilities().contains(ability) {
            return Err(GrantError::Forbidden { ability });
        }
        Ok(())
    }

    /// The wire form: a step count, then that many fixed-width steps.
    #[must_use]
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out =
            Vec::with_capacity(1_usize.saturating_add(self.steps.len().saturating_mul(STEP_BYTES)));
        // The count cannot exceed MAX_STEPS, which is far below 255; a chain that
        // somehow held more would encode a count it could never decode, so the
        // saturation makes that impossible instead of wrapping into a lie.
        out.push(u8::try_from(self.steps.len()).unwrap_or(u8::MAX));
        for step in &self.steps {
            out.extend_from_slice(&step.to_bytes());
        }
        out
    }

    /// Reads the wire form.
    ///
    /// Nothing is verified here beyond shape: a decoded grant is a claim, and
    /// [`Grant::verify`] is what turns a claim into an authorisation.
    ///
    /// # Errors
    ///
    /// [`GrantError::Truncated`], [`GrantError::TrailingBytes`],
    /// [`GrantError::TooLong`], or a malformed field.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, GrantError> {
        let mut reader = Reader::new(bytes);
        let count = usize::from(reader.take_byte()?);
        if count == 0 {
            return Err(GrantError::Empty);
        }
        if count > MAX_STEPS {
            return Err(GrantError::TooLong {
                count,
                limit: MAX_STEPS,
            });
        }
        let mut steps = Vec::with_capacity(count);
        for _ in 0..count {
            steps.push(Step::read(&mut reader)?);
        }
        if reader.remaining() != 0 {
            return Err(GrantError::TrailingBytes {
                count: reader.remaining(),
            });
        }
        Ok(Self { steps })
    }
}

/// The proof `ARCHITECTURE.md` asks for, against real MIR rather than samples.
///
/// `kani` is not installed on this machine, so this harness does not run in
/// `just check`; `cargo kani --harness attenuation_never_widens` is what runs it,
/// and `crates/grant/tests/attenuation.rs` is what samples the same property
/// today. The harness is committed rather than deferred because a property worth
/// proving is worth writing down in the form the prover accepts.
#[cfg(kani)]
mod proof {
    use super::Grant;
    use crate::scope::{Abilities, Scope};
    use kusanagi_kernel::{Instant, Signer};

    #[kani::proof]
    #[kani::unwind(4)]
    fn attenuation_never_widens() {
        let held_bits: u8 = kani::any();
        let asked_bits: u8 = kani::any();
        kani::assume(held_bits <= 0b11 && asked_bits <= 0b11);
        let held_until: u64 = kani::any();
        let asked_until: u64 = kani::any();

        let (Ok(held_abilities), Ok(asked_abilities)) = (
            Abilities::from_bits(held_bits),
            Abilities::from_bits(asked_bits),
        ) else {
            return;
        };
        let held = Scope::new(held_abilities, Instant::from_unix_seconds(held_until));
        let asked = Scope::new(asked_abilities, Instant::from_unix_seconds(asked_until));

        let root = Signer::from_seed(&[0; 32]);
        let first = Signer::from_seed(&[1; 32]);
        let grant = Grant::issue(&root, &first.handle(), held);
        if let Ok(delegated) = grant.attenuate(&first, &Signer::from_seed(&[2; 32]).handle(), asked)
        {
            let Some(step) = delegated.steps().last() else {
                return;
            };
            assert!(step.scope().is_within(&held));
        }
    }
}
