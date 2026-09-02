// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where a segment sits in its chain, and the witness that lets the next one say
//! so.
//!
//! Two illegal states are unspellable here rather than validated: a genesis
//! segment cannot carry a predecessor, and a following segment cannot sit at
//! index zero. [`ChainHead`] has no public constructor, so the only way to hold
//! one is to have held the segment it describes — which is what lets a chain of
//! a million segments be extended for the price of forty bytes.

use core::num::NonZeroU64;

use crate::segment::SegmentId;

/// A witness that a particular segment exists, and where it sits.
///
/// There is no public constructor: the only way to hold one is to have held the
/// segment it describes. That is what lets [`Segment::extend`] build a link whose
/// height and predecessor cannot disagree, while carrying forty bytes instead of
/// a whole segment — extending a chain of a million segments costs the same as
/// extending a chain of one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ChainHead {
    id: SegmentId,
    index: u64,
}

impl ChainHead {
    /// Mints a witness. Crate-private on purpose: outside this crate the only
    /// way to obtain one is [`crate::Segment::head`], which is what makes a
    /// head a witness rather than a pair of numbers anybody can assert.
    pub(crate) const fn new(id: SegmentId, index: u64) -> Self {
        Self { id, index }
    }

    /// Rebuilds a head from a note this endpoint wrote about a segment it held.
    ///
    /// A head obtained through [`crate::Segment::head`] is a witness: whoever
    /// holds it held the segment. A head obtained here is one step weaker — it is
    /// this endpoint's own record of having held it, read back from its own disk.
    /// The two are the same forty bytes and the same type, so this is the one
    /// place where that difference is visible, and it is stated rather than
    /// hidden.
    ///
    /// **A false head cannot cause anything to be accepted.** Every use of a head
    /// is a comparison: a chain that does not link to it is refused, and a segment
    /// extended from it is signed by this endpoint and refused by every reader
    /// whose own chain disagrees. So the damage a corrupted record can do is to
    /// make this endpoint reject a chain it should have accepted — never the
    /// reverse. That asymmetry is the whole argument for this constructor, and if
    /// it ever stops holding, this constructor has to go.
    ///
    /// The disk it is read back from already holds this endpoint's identity seed
    /// and every channel secret, so it adds no attacker who was not already able
    /// to read the traffic outright.
    #[must_use]
    pub const fn recorded(id: SegmentId, index: u64) -> Self {
        Self { id, index }
    }

    /// The segment this head witnesses.
    #[must_use]
    pub const fn id(&self) -> SegmentId {
        self.id
    }

    /// How high that segment sits.
    #[must_use]
    pub const fn index(&self) -> u64 {
        self.index
    }
}

/// Where a segment sits in its chain.
///
/// Two illegal states are unspellable here rather than validated: a genesis
/// segment cannot carry a predecessor, and a following segment cannot sit at
/// index zero.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Link {
    /// The first segment of a chain.
    Genesis,
    /// Every later segment.
    Follows {
        /// This segment's height, which is always at least one.
        index: NonZeroU64,
        /// The identity of the segment directly beneath it.
        previous: SegmentId,
    },
}
