// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Places that hold bytes for somebody else.
//!
//! Four adapters ship here, and that is deliberate: one adapter is a
//! hypothetical seam, several make it real. What they share is not an interface
//! document but [`conformance::run`] — a function, not a test — so that an
//! adapter written outside this repository can prove itself against the same
//! contract, and so that `doctor` can run that contract against a live host.
//!
//! None of them is trusted. A waypoint sees an opaque address and an opaque byte
//! string; everything it returns is checked against a hash by the caller. The
//! one thing a host is asked to be honest about is refusing a second write to an
//! occupied address, and [`probe::examine`] measures that rather than assuming it.

mod certificate;
mod conditional;
pub mod conformance;
mod dir;
mod http;
mod memory;
mod place;
pub mod probe;
mod s3;
mod sigv4;

pub use certificate::{Capability, Certificate, Finding, Tier, Verdict};
pub use conditional::{Conditional, Fetched, TtlOutcome, Validator};
pub use dir::DirWaypoint;
pub use http::{HttpWaypoint, TTL_HEADER};
pub use memory::MemoryWaypoint;
pub use place::{Locator, LocatorError, Place};
pub use s3::S3Waypoint;
pub use sigv4::Credentials;
