// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Permission, in the only form this network has it.
//!
//! There is no access control list anywhere in kusanagi and no server that
//! decides who may do what. There is one thing: a [`Grant`], which is a chain of
//! signed steps that starts at a root authority and can only ever narrow.
//! Verifying one requires no network, no clock but the one you pass in, and no
//! agreement with anybody.
//!
//! Three properties are worth stating because the rest of the design leans on
//! them:
//!
//! - **Attenuation cannot widen.** A delegated scope is produced by
//!   [`Scope::meet`], the greatest lower bound, and by nothing else.
//! - **Revocation is immediate and transitive.** Verification walks from the
//!   root, so a revoked step voids everything beneath it with no cascade to
//!   propagate and nothing to re-sign.
//! - **Nothing here is a policy.** This crate answers "does this chain permit
//!   this"; who is asking and what they get to do about the answer belongs to
//!   the caller.

mod chain;
mod error;
mod revocation;
mod scope;
mod step;

pub use chain::{Grant, MAX_STEPS};
pub use error::GrantError;
pub use revocation::Revocations;
pub use scope::{Abilities, Ability, Scope, UnknownAbility};
pub use step::{Step, StepId};
