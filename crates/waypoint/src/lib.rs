// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! Places that hold bytes for somebody else.
//!
//! Two adapters ship here, and that is deliberate: one adapter is a hypothetical
//! seam, two make it real. What they share is not an interface document but
//! [`conformance::run`] — a function, not a test — so that an adapter written
//! outside this repository can prove itself against the same contract.

pub mod conformance;
mod dir;
mod memory;

pub use dir::DirWaypoint;
pub use memory::MemoryWaypoint;
