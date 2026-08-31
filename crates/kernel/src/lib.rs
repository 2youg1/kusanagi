// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! The noun layer of kusanagi: identifiers, segments, canonical bytes, and the
//! [`Waypoint`] seam.
//!
//! This crate performs no I/O and holds no policy. It defines what a thing *is*
//! so that the outer layers can decide what to *do*. Every type here is a value:
//! no interior mutability, no handles to the outside world, no clock.
//!
//! The one rule that governs the whole crate: **the bytes of a [`Segment`] are
//! canonical**. Encoding the same segment twice yields identical bytes, because
//! the identity of a segment is the hash of those bytes.

mod address;
mod digest;
mod handle;
mod segment;
mod waypoint;

pub use address::{DropAddr, public_v0};
pub use digest::{Digest, DigestParseError};
pub use handle::Handle;
pub use segment::{ChainHead, Link, MAX_PAYLOAD, Segment, SegmentError, SegmentId};
pub use waypoint::{PutOutcome, Waypoint, WaypointError};
