// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The noun layer of kusanagi: identifiers, segments, canonical bytes, and the
//! [`Waypoint`] and [`Clock`] seams.
//!
//! This crate performs no I/O and holds no policy. It defines what a thing *is*
//! so that the outer layers can decide what to *do*. Every type here is a value:
//! no interior mutability, no handles to the outside world, no clock reading
//! itself.
//!
//! Two rules govern the whole crate. **The bytes of a [`Segment`] are canonical**,
//! because the identity of a segment is the hash of those bytes. And **a segment
//! that exists is a segment that was signed**, because both constructors sign and
//! the decoder verifies — so no caller downstream has to remember to check.

mod address;
mod clock;
mod digest;
mod identity;
mod link;
mod payload;
mod segment;
mod trail;
mod waypoint;
mod wire;

pub use address::DropAddr;
pub use clock::{Clock, FixedClock, Instant};
pub use digest::{Digest, DigestParseError};
pub use identity::{Handle, NotAuthentic, Signature, Signer, VerifyingKey};
pub use link::{ChainHead, Link};
pub use payload::{MAX_PAYLOAD, MAX_SEGMENT};
pub use segment::{Segment, SegmentError, SegmentId};
pub use trail::{Commitment, Reveal, Trail};
pub use waypoint::{PutOutcome, Waypoint, WaypointError};
pub use wire::{Hex, HexError, Incomplete, Reader, unhex};
