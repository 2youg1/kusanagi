// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where a segment sits in its chain, and the witness that lets the next one say
//! so.
//!
//! Four illegal states are unspellable here rather than validated: a genesis
//! segment cannot carry a predecessor, a following segment cannot sit at index
//! zero, **a genesis segment cannot carry a reveal, and a following segment
//! cannot carry a signature.** The last two are what the Trail turns on: the
//! first segment of a chain is the only one anybody signs, and every segment
//! after it is authenticated by a proof that convinces its reader and nobody
//! else. Holding the authenticator inside the link is what stops a decoder, a
//! constructor or a future caller from producing the other two combinations.
//!
//! [`ChainHead`] has no public constructor, so the only way to hold one is to
//! have held the segment it describes — which is what lets a chain of a million
//! segments be extended for the price of seventy-two bytes.

use core::num::NonZeroU64;

use crate::identity::Signature;
use crate::segment::SegmentId;
use crate::trail::{Commitment, Reveal};

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
    awaited: Commitment,
}

impl ChainHead {
    /// Mints a witness. Crate-private on purpose: outside this crate the only
    /// way to obtain one is [`crate::Segment::head`], which is what makes a
    /// head a witness rather than three fields anybody can assert.
    pub(crate) const fn new(id: SegmentId, index: u64, awaited: Commitment) -> Self {
        Self { id, index, awaited }
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
    pub const fn recorded(id: SegmentId, index: u64, awaited: Commitment) -> Self {
        Self { id, index, awaited }
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

    /// The commitment that segment made about the one above it.
    ///
    /// Carried here rather than looked up because it is the only thing a reader
    /// needs in order to accept the next segment, and a reader that resumes from
    /// a cairn has nothing else left of the segment below.
    #[must_use]
    pub const fn awaited(&self) -> Commitment {
        self.awaited
    }
}

/// Where a segment sits in its chain, and what authenticates it there.
///
/// The two shapes carry different authenticators because they answer different
/// questions. A genesis segment has nothing beneath it to commit to it, so it is
/// signed — which is also what stops a peer who holds the channel secret from
/// racing to height zero with a commitment of their own. Every segment above it
/// shows the proof the segment below promised, and a proof is worth exactly one
/// height to exactly one reader.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[expect(
    clippy::large_enum_variant,
    reason = "the difference is the design: a genesis segment carries an               ML-DSA-87 signature and a following one carries none. Boxing it               would cost `Copy`, which every `const fn` accessor on `Segment`               relies on, to save a copy the ruling in ARCHITECTURE.md §8               explicitly authorises spending"
)]
pub enum Link {
    /// The first segment of a chain.
    Genesis {
        /// What this segment promises about height one.
        commit: Commitment,
        /// The author's signature over the body.
        signature: Signature,
    },
    /// Every later segment.
    Follows {
        /// This segment's height, which is always at least one.
        index: NonZeroU64,
        /// The identity of the segment directly beneath it.
        previous: SegmentId,
        /// The proof the segment beneath it committed to.
        reveal: Reveal,
        /// What this segment promises about the height above it.
        commit: Commitment,
    },
}
