// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a measurement is said in: a capability, a verdict, a tier, and the
//! certificate that carries them.
//!
//! These names are a published interface — they appear in `doctor` output and in
//! whatever reads it — so they are kept apart from `probe.rs`, which is the
//! writing and reading that finds out. Renaming one of these changes everybody's
//! scripts; changing how a measurement is taken changes nothing but the answer.

use core::fmt;

/// Something a host either does or does not do.
///
/// These names are published identifiers: they appear in `doctor` output and in
/// scripts that read it, so renaming one is a change to a public interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Capability {
    /// A second write to an occupied address is refused.
    WriteOnce,
    /// A reader that already has the bytes can be told so without receiving them.
    ConditionalRead,
    /// The host's name for a version does not change while the bytes do not.
    StableValidator,
    /// The host can be asked to forget an object after a while.
    Expiry,
    /// The host can say what a bin holds, so a read need not name an address.
    ///
    /// Since D-20 this is what makes a host readable at all, which is why
    /// `doctor` reports it beside the others rather than assuming it.
    BinListing,
}

impl Capability {
    /// Every capability, in the order `doctor` reports them.
    pub const ALL: [Self; 5] = [
        Self::WriteOnce,
        Self::ConditionalRead,
        Self::StableValidator,
        Self::Expiry,
        Self::BinListing,
    ];

    /// The published name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::WriteOnce => "write-once",
            Self::ConditionalRead => "conditional-read",
            Self::StableValidator => "stable-validator",
            Self::Expiry => "expiry",
            Self::BinListing => "bin-listing",
        }
    }
}

impl fmt::Display for Capability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// What a host was measured to do about one capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// Measured, and the host does it.
    Held,
    /// The host does not offer this, and says so consistently.
    NotOffered {
        /// Why it is absent, in words a person can act on.
        because: String,
    },
    /// The host claims this and does not hold it.
    Broken {
        /// What was expected and what happened instead.
        detail: String,
    },
}

impl Verdict {
    /// The one-word form used in output and in exit-code decisions.
    #[must_use]
    pub const fn word(&self) -> &'static str {
        match self {
            Self::Held => "held",
            Self::NotOffered { .. } => "not offered",
            Self::Broken { .. } => "BROKEN",
        }
    }
}

/// One measured capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// What was measured.
    pub capability: Capability,
    /// What was found.
    pub verdict: Verdict,
}

/// How far a host can be trusted to hold this network's central invariant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    /// A drop is claimed once and cannot be overwritten. Full semantics.
    WriteOnce,
    /// The host will let a drop be rewritten, so both sides must acknowledge
    /// what they first saw at an address and refuse anything that arrives later.
    AckFirstSeen,
}

impl Tier {
    /// The published name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::WriteOnce => "write-once",
            Self::AckFirstSeen => "ack-first-seen",
        }
    }
}

/// What a host was measured to be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Certificate {
    findings: Vec<Finding>,
}

impl Certificate {
    /// Records a set of findings.
    ///
    /// Crate-private: a certificate that was not produced by measuring a host is
    /// a claim about that host, and this type exists so that there is no such
    /// thing.
    pub(crate) const fn of(findings: Vec<Finding>) -> Self {
        Self { findings }
    }

    /// Every measurement, in [`Capability::ALL`] order.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }

    /// What was found for one capability.
    #[must_use]
    pub fn verdict(&self, capability: Capability) -> Option<&Verdict> {
        self.findings
            .iter()
            .find(|finding| finding.capability == capability)
            .map(|finding| &finding.verdict)
    }

    /// The tier this host qualifies for.
    ///
    /// Only one capability decides it. Conditional reads and expiry change what a
    /// host *costs*; write-once changes what the protocol *is*.
    #[must_use]
    pub fn tier(&self) -> Tier {
        match self.verdict(Capability::WriteOnce) {
            Some(&Verdict::Held) => Tier::WriteOnce,
            _ => Tier::AckFirstSeen,
        }
    }

    /// Whether any capability was claimed and not held.
    #[must_use]
    pub fn any_broken(&self) -> bool {
        self.findings
            .iter()
            .any(|finding| matches!(finding.verdict, Verdict::Broken { .. }))
    }
}
