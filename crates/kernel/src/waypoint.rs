// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The seam every place-that-stores-bytes implements.
//!
//! A waypoint is not trusted. It sees an opaque address and an opaque byte
//! string, and it is never asked a question whose answer it could usefully lie
//! about: what it returns is verified against a hash by the caller. That is why
//! this trait has three methods and no notion of accounts or sessions.
//!
//! Every one of them takes an [`Object`]: a period, a ward and an address, which
//! is the whole of what a host is told. A reader that named an address alone
//! handed the host the pair of writer and reader on its own access log, and
//! `ARCHITECTURE.md` §9 D-20 replaced that with a read that names a bin. Listing
//! a bin is a *transport* capability, so it lives in [`Listing`] and a place may
//! honestly decline to hold it.
//!
//! `put_if_absent` is not `put`. A [`Drop`](crate::DropAddr) receives exactly one
//! segment in its lifetime, so the storage layer — not this code — is what
//! refuses a second write. An adapter that cannot refuse one must say so through
//! [`WaypointError::OverwriteNotRefused`] rather than pretend.
//!
//! `delete` exists for one reason and it is not tidiness. A channel that has
//! chosen to release keeps no history on the host at all: once the peer says
//! they have read something, the bytes are removed and the only remaining copy
//! is the reader's own site. A host that quietly kept a copy would defeat that,
//! which is why the ratchet burns the key as well — deletion is the honest
//! host's half and the ratchet is the dishonest host's half.

use crate::filing::{Object, Sweep};

/// What happened to a write.
///
/// An address that already holds bytes is a normal outcome, not a failure: a
/// resend after a lost acknowledgement lands here, and the caller should carry on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PutOutcome {
    /// The bytes were written; the address was empty.
    Stored,
    /// The address already held bytes, which were left untouched.
    AlreadyPresent,
}

/// Anything that stores bytes under an opaque key.
pub trait Waypoint {
    /// Writes `bytes` at `at` only if `at` is empty.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] describes what the underlying store refused or failed to
    /// do. An adapter that cannot guarantee write-once semantics must fail with
    /// [`WaypointError::OverwriteNotRefused`] rather than silently overwrite.
    fn put_if_absent(&self, at: &Object, bytes: &[u8]) -> Result<PutOutcome, WaypointError>;

    /// Reads the bytes at `at`, if any.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] describes a transport or storage failure. An empty
    /// address is `Ok(None)`, not an error: nothing has arrived yet is the normal
    /// state of this network.
    fn get(&self, at: &Object) -> Result<Option<Vec<u8>>, WaypointError>;

    /// Removes whatever is at `at`, if anything is.
    ///
    /// **Deleting an empty address is success, not a failure.** A caller that
    /// released the same drop twice, or that is releasing a drop a host already
    /// expired, has got what it asked for: nothing is there. Reporting that as
    /// an error would make an idempotent operation look like a broken one.
    ///
    /// A host is not believed about this any more than about a write. Whoever
    /// needs to know reads the address back.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] when the store failed, and
    /// [`WaypointError::DeletionRefused`] when this kind of place cannot remove
    /// anything at all — which a channel that releases must find out before it
    /// relies on it, rather than after.
    fn delete(&self, at: &Object) -> Result<(), WaypointError>;
}

/// A place that can say what it holds under a key prefix.
///
/// Apart from [`Waypoint`] for the reason `kusanagi_waypoint::Conditional` is:
/// the seam every place implements must stay small enough that a U-stick
/// adapter can implement all of it, and listing is something a place can be
/// unable to do without being broken. What a reader does when a place declines
/// is not this crate's business — it is a verb's, and `kusanagi.unlisted` is the
/// code the door gives it.
pub trait Listing {
    /// Every object the host holds under `sweep`.
    ///
    /// **Order is not part of this contract** and neither is completeness beyond
    /// a cap the adapter documents: a reader takes what it is given, matches it
    /// against addresses only it can derive, and drops the rest. A host that
    /// adds objects to a bin can waste a reader's bandwidth and can never learn
    /// which of them the reader wanted.
    ///
    /// # Errors
    ///
    /// [`WaypointError::ListingRefused`] when this kind of place cannot list at
    /// all, and [`WaypointError`] for a transport failure.
    fn list(&self, sweep: &Sweep) -> Result<Vec<Object>, WaypointError>;
}

/// Why a waypoint could not do what was asked.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum WaypointError {
    /// The underlying store failed.
    #[error("waypoint io failed while {action}: {source}")]
    Io {
        /// What was being attempted, for a caller that must explain itself.
        action: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The store accepted a write that should have been refused.
    #[error("this waypoint does not refuse overwrites; write-once semantics are unavailable")]
    OverwriteNotRefused,
    /// The store will not remove anything, so nothing can be released on it.
    #[error("this waypoint does not delete, so a channel cannot release on it")]
    DeletionRefused,
    /// The store cannot say what it holds, so a read here would have to name an
    /// address.
    ///
    /// A refusal rather than a fallback to naming addresses: falling back would
    /// withdraw the property the bin exists for, quietly, on exactly the hosts
    /// that cannot support it.
    #[error("this waypoint does not list, so a read here would have to name an address")]
    ListingRefused,
    /// The address is not usable as a key in this store.
    #[error("address is not a usable key here: {reason}")]
    UnusableAddress {
        /// Why the store rejected the key.
        reason: String,
    },
    /// The host answered with somewhere else to go, and was not followed.
    ///
    /// Following it would open a connection this endpoint did not choose,
    /// handing a third party the endpoint's address and the drop it asked for.
    /// A host that stores bytes has no reason to redirect, so this is a refusal
    /// rather than a step.
    #[error("waypoint sent {action} somewhere else, to {to}")]
    Redirected {
        /// What was being attempted.
        action: &'static str,
        /// Where the host wanted the request to go instead.
        to: String,
    },
    /// The write did not land, and the host said nothing about why.
    ///
    /// A box answers every write the same way, so a caller learns what happened
    /// by reading the address back. Nothing there means nothing was written: the
    /// host is full, what arrived had already expired, or the place being
    /// written to is not a box at all.
    #[error("waypoint kept nothing while {action}")]
    Unwritten {
        /// What was being attempted.
        action: &'static str,
    },
    /// The host took longer than a one-shot command can wait.
    #[error("waypoint did not answer while {action}, after {after:?}")]
    Unanswered {
        /// What was being attempted.
        action: &'static str,
        /// How long it was given.
        after: std::time::Duration,
    },
}

impl WaypointError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "waypoint.io",
            Self::OverwriteNotRefused => "waypoint.overwrite_not_refused",
            Self::DeletionRefused => "waypoint.deletion_refused",
            Self::ListingRefused => "waypoint.unlisted",
            Self::UnusableAddress { .. } => "waypoint.unusable_address",
            Self::Redirected { .. } => "waypoint.redirected",
            Self::Unwritten { .. } => "waypoint.unwritten",
            Self::Unanswered { .. } => "waypoint.timeout",
        }
    }
}
