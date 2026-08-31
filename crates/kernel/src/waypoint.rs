// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! The seam every place-that-stores-bytes implements.
//!
//! A waypoint is not trusted. It sees an opaque address and an opaque byte
//! string, and it is never asked a question whose answer it could usefully lie
//! about: what it returns is verified against a hash by the caller. That is why
//! this trait has two methods and no notion of accounts, sessions, or listing.
//!
//! `put_if_absent` is not `put`. A [`Drop`](crate::DropAddr) receives exactly one
//! segment in its lifetime, so the storage layer — not this code — is what
//! refuses a second write. An adapter that cannot refuse one must say so through
//! [`WaypointError::OverwriteNotRefused`] rather than pretend.

use crate::address::DropAddr;

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
    /// Writes `bytes` at `addr` only if `addr` is empty.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] describes what the underlying store refused or failed to
    /// do. An adapter that cannot guarantee write-once semantics must fail with
    /// [`WaypointError::OverwriteNotRefused`] rather than silently overwrite.
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError>;

    /// Reads the bytes at `addr`, if any.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] describes a transport or storage failure. An empty
    /// address is `Ok(None)`, not an error: nothing has arrived yet is the normal
    /// state of this network.
    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError>;
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
    /// The address is not usable as a key in this store.
    #[error("address is not a usable key here: {reason}")]
    UnusableAddress {
        /// Why the store rejected the key.
        reason: String,
    },
}

impl WaypointError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Io { .. } => "waypoint.io",
            Self::OverwriteNotRefused => "waypoint.overwrite_not_refused",
            Self::UnusableAddress { .. } => "waypoint.unusable_address",
        }
    }
}
