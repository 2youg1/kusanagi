// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One lane's progress through the bins of a sweep, a bin at a time.
//!
//! Four checks happen on the way, in this order, and none of them is optional:
//! the bytes must open under the key that height derives, they must decode to a
//! segment, that segment must be signed by the handle we expected, and it must
//! follow the one before it. A failure at any of them stops the walk — a chain
//! that has been interfered with is not a chain with a gap in it.
//!
//! Apart from `walk.rs` because it holds no policy: where a lane starts and
//! what is written down afterwards are decided there. This is the part that
//! stays the same whether one lane is walked or thirty-two share a sweep.

use std::collections::HashMap;

use kusanagi_chain::{Cairn, Verifier};
use kusanagi_kernel::{DropAddr, Period, Segment, SegmentError};
use kusanagi_seal::{Fit, open};

use crate::lane::Lane;
use kusanagi_door::Complaint;

/// One segment, and the address it was found at.
pub struct Held {
    /// Where it was.
    pub address: DropAddr,
    /// The period of the bin it was found in: when its author filed it, as
    /// the host already sees it. Streams carry no clock, and this is the one
    /// coarse order that holds across authors.
    pub filed: Period,
    /// What it was.
    pub segment: Segment,
}

/// One lane being walked: what it has verified, what it fetched, and the
/// height it wants next.
pub(crate) struct Stepping<'a> {
    lane: &'a Lane,
    verifier: Verifier,
    held: Vec<Held>,
    /// The next height wanted; none once the last height a `u64` counts is
    /// verified, because nothing can stand above it.
    next: Option<u64>,
}

impl<'a> Stepping<'a> {
    /// A walk of `lane` carrying on from `from`, or from genesis when there is
    /// nothing to carry on from.
    ///
    /// `from` is where verification carries on from, not merely where fetching
    /// starts: the first segment found must link to that cairn's head, so
    /// resuming checks the join rather than assuming it.
    pub(crate) fn from(lane: &'a Lane, from: Option<Cairn>) -> Self {
        let (verifier, next) = match from {
            Some(cairn) => (Verifier::resume(cairn), cairn.next_index()),
            None => (Verifier::new(), Some(0)),
        };
        Self {
            lane,
            verifier,
            held: Vec::new(),
            next,
        }
    }

    /// Claims every consecutive height this bin holds, stopping at the first it
    /// does not: a height above a missing one could only be reached by skipping
    /// it, and the next bin may hold the missing one.
    ///
    /// # Errors
    ///
    /// Everything [`decode`] reports, plus [`Complaint::Chain`] when a segment
    /// does not follow the one before it — which, on a resumed walk, is also
    /// what a host that revised a drop this endpoint already read comes out as.
    pub(crate) fn advance(
        &mut self,
        filed: Period,
        bin: &mut HashMap<DropAddr, Vec<u8>>,
        name: &str,
    ) -> Result<(), Complaint> {
        while let Some(index) = self.next {
            let address = self.lane.keys.address(index);
            let Some(sealed) = bin.remove(&address) else {
                return Ok(());
            };
            let segment = decode(self.lane, name, index, &sealed)?;
            self.verifier.accept(&segment)?;
            self.held.push(Held {
                address,
                filed,
                segment,
            });
            self.next = index.checked_add(1);
        }
        Ok(())
    }

    /// What the walk holds once every bin has been offered to it.
    pub(crate) fn finish(self) -> (Verifier, Vec<Held>) {
        (self.verifier, self.held)
    }
}

/// Opens and decodes what was found at `index`, as a segment by this lane's
/// author.
///
/// # Errors
///
/// [`Complaint::Sealed`] when the bytes do not open under this height's key,
/// [`Complaint::NotThePeer`] when what comes out is a genuine segment by
/// somebody else — the host answering with a drop from a stream nobody asked
/// for, reported as what it is rather than as a malformed segment — and
/// [`Complaint::Segment`] for anything else that is not a segment.
pub(crate) fn decode(
    lane: &Lane,
    name: &str,
    index: u64,
    sealed: &[u8],
) -> Result<Segment, Complaint> {
    let plain = open(&lane.keys.key(index)?, Fit::Veil, sealed)?;
    match Segment::from_canonical_bytes(&plain, &lane.author) {
        Err(SegmentError::NotTheAuthor { .. }) => Err(Complaint::NotThePeer {
            name: name.to_owned(),
        }),
        other => Ok(other?),
    }
}
