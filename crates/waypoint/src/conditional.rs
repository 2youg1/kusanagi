// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The two things a network host can do that a directory cannot.
//!
//! [`Waypoint`](kusanagi_kernel::Waypoint) has three methods and will keep them,
//! because it is the seam every place implements and every extra method is a
//! method somebody's U-stick adapter has to fake. But polling costs money, and
//! the difference between "fetch it again" and "tell me it has not changed" is
//! the difference between paying for a byte and paying for a round trip. That
//! difference is a *transport* capability, so it lives in its own trait, which a
//! place may decline to hold.
//!
//! This trait reports **mechanism**, never health. Whether a host's answers add
//! up to a usable host is one judgement made in one place, `probe::examine`.

use kusanagi_kernel::{Object, WaypointError};

/// A host's own name for a version of some bytes — an HTTP `ETag`.
///
/// Opaque on purpose: its only defined operation is being handed back to the
/// host that produced it. Two validators are comparable, and nothing else about
/// one means anything.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Validator(String);

impl Validator {
    /// Wraps a host's validator exactly as the host spelled it.
    #[must_use]
    pub fn new(value: &str) -> Self {
        Self(value.to_owned())
    }

    /// The validator as the host spelled it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a conditional read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fetched {
    /// Nothing is at this address.
    Absent,
    /// The host confirmed the caller's copy is current, and sent no body.
    Unchanged,
    /// Bytes, and the validator to present next time if the host offered one.
    Fresh {
        /// What was stored.
        bytes: Vec<u8>,
        /// The host's name for this version, when it named one.
        validator: Option<Validator>,
    },
}

/// What a host did with a requested lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtlOutcome {
    /// The host took the request. Whether it *honours* it is a separate question,
    /// answered by reading the address afterwards rather than by believing this.
    Accepted,
    /// The host has no per-object lifetime and said so.
    NotOffered,
}

/// A place that can answer conditionally.
pub trait Conditional {
    /// Reads, telling the host what the caller already has.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] for a transport failure. An absent address and an
    /// unchanged one are answers, not failures.
    fn get_if_changed(
        &self,
        at: &Object,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError>;

    /// Writes with a requested lifetime in seconds.
    ///
    /// A lifetime of zero means "already expired", which is how a probe tests
    /// expiry without waiting: a host that honours lifetimes answers the
    /// following read with nothing, and one that ignores them hands the bytes
    /// straight back.
    ///
    /// # Errors
    ///
    /// [`WaypointError`] for a transport failure.
    fn put_with_ttl(
        &self,
        at: &Object,
        bytes: &[u8],
        seconds: u64,
    ) -> Result<TtlOutcome, WaypointError>;
}
