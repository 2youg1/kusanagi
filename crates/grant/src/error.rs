// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Why a grant does not authorise what was attempted.
//!
//! Every variant names the step it failed at, because a delegation chain that is
//! refused without saying where is a chain nobody can repair.

use kusanagi_kernel::{Handle, Incomplete, Instant};

use crate::scope::{Ability, UnknownAbility};
use crate::step::StepId;

/// Why a grant is not valid, or does not reach far enough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum GrantError {
    /// A grant with no steps authorises nothing.
    #[error("a grant with no steps authorises nothing")]
    Empty,
    /// The chain is longer than this version verifies.
    #[error("a delegation chain of {count} steps exceeds the limit of {limit}")]
    TooLong {
        /// How many steps arrived.
        count: usize,
        /// The limit in force.
        limit: usize,
    },
    /// The chain does not start where the verifier expected.
    #[error("this chain descends from {found}, not from {expected}")]
    WrongRoot {
        /// The root the verifier trusts.
        expected: Handle,
        /// The handle that actually signed the first step.
        found: Handle,
    },
    /// A step after the first claimed to be a root, or the first did not.
    #[error("the step at position {at} does not attach to the one above it")]
    Detached {
        /// Where in the chain the break is.
        at: usize,
    },
    /// A step was signed by somebody who did not hold the step above.
    #[error("the step at position {at} was issued by a handle that did not hold the one above")]
    IssuerMismatch {
        /// Where in the chain the break is.
        at: usize,
    },
    /// A step's signature does not check out.
    #[error("the step {step} is not signed by the handle it names as issuer")]
    NotAuthentic {
        /// Which step failed.
        step: StepId,
    },
    /// A step conveys more than the step above it held.
    #[error("the step at position {at} conveys more than the step above it holds")]
    Widened {
        /// Where in the chain the break is.
        at: usize,
    },
    /// A step in the chain has been revoked.
    #[error("the step {step} has been revoked; everything below it is void")]
    Revoked {
        /// Which step was revoked.
        step: StepId,
    },
    /// The grant has expired.
    #[error("this grant expired at {}, and it is now {}", expired_at.as_unix_seconds(), now.as_unix_seconds())]
    Expired {
        /// When it stopped applying.
        expired_at: Instant,
        /// The time it was checked against.
        now: Instant,
    },
    /// The grant is held by somebody else.
    #[error("this grant was issued to {holder}, not to {presenter}")]
    NotTheHolder {
        /// Who the last step names.
        holder: Handle,
        /// Who presented it.
        presenter: Handle,
    },
    /// The grant is valid but does not carry the ability that was needed.
    #[error("this grant does not carry `{}`", ability.name())]
    Forbidden {
        /// What was attempted.
        ability: Ability,
    },
    /// Only the current holder may delegate onward.
    #[error("only the handle a grant was issued to may delegate it onward")]
    NotYours,
    /// The bytes ran out mid-step.
    #[error("grant bytes end inside a step: {0}")]
    Truncated(#[from] Incomplete),
    /// Bytes remain after a complete grant.
    #[error("{count} byte(s) follow a complete grant")]
    TrailingBytes {
        /// How many bytes were left over.
        count: usize,
    },
    /// A step named an ability this version does not define.
    #[error(transparent)]
    UnknownAbility(#[from] UnknownAbility),
    /// A step's parent field is neither absent nor a well-formed identifier.
    #[error("a step's parent field carries {tag}, which is neither absent nor present")]
    UnknownParentTag {
        /// The byte that was read.
        tag: u8,
    },
}

impl GrantError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "grant.empty",
            Self::TooLong { .. } => "grant.too_long",
            Self::WrongRoot { .. } => "grant.wrong_root",
            Self::Detached { .. } => "grant.detached",
            Self::IssuerMismatch { .. } => "grant.issuer_mismatch",
            Self::NotAuthentic { .. } => "grant.not_authentic",
            Self::Widened { .. } => "grant.widened",
            Self::Revoked { .. } => "grant.revoked",
            Self::Expired { .. } => "grant.expired",
            Self::NotTheHolder { .. } => "grant.not_the_holder",
            Self::Forbidden { .. } => "grant.forbidden",
            Self::NotYours => "grant.not_yours",
            Self::Truncated(_) => "grant.truncated",
            Self::TrailingBytes { .. } => "grant.trailing",
            Self::UnknownAbility(_) => "grant.unknown_ability",
            Self::UnknownParentTag { .. } => "grant.unknown_parent_tag",
        }
    }
}
