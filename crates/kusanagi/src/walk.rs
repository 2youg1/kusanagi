// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Reading one author's stream out of a waypoint, checking it as it goes.
//!
//! Four checks happen on the way, in this order, and none of them is optional:
//! the bytes must open under the key that address derives, they must decode to a
//! segment, that segment must be signed by the handle we expected, and it must
//! follow the one before it. A failure at any of them stops the walk — a chain
//! that has been interfered with is not a chain with a gap in it.
//!
//! The walk stops at the first empty address, which costs one request per
//! segment. That is the honest price of an endpoint with no local index: the
//! Bell described in the architecture is what a later version has to beat, and it
//! has to beat this shape rather than replace it.

use kusanagi_chain::Verifier;
use kusanagi_kernel::{ChainHead, DropAddr, Handle, Segment, Waypoint};
use kusanagi_seal::{Stream, derive, open};

use crate::complaint::Complaint;

/// One segment, and the address it was found at.
pub struct Held {
    /// Where it was.
    pub address: DropAddr,
    /// What it was.
    pub segment: Segment,
}

/// A stream as it was found on a waypoint.
pub struct Walked {
    verifier: Verifier,
    held: Vec<Held>,
}

impl Walked {
    /// The verified head, absent when the stream has not started.
    #[must_use]
    pub fn head(&self) -> Option<ChainHead> {
        self.verifier.head()
    }

    /// Every segment found, in order.
    #[must_use]
    pub fn held(&self) -> &[Held] {
        &self.held
    }

    /// The next height this stream will use.
    #[must_use]
    pub fn next_index(&self) -> u64 {
        self.held.len().try_into().unwrap_or(u64::MAX)
    }
}

/// Reads one drop, if anything is there.
///
/// # Errors
///
/// [`Complaint::Waypoint`] when the host fails, [`Complaint::Sealed`] when the
/// bytes do not open under this address's key, and [`Complaint::Segment`] when
/// what comes out is not a segment.
pub fn peek(
    waypoint: &impl Waypoint,
    stream: &Stream,
    index: u64,
) -> Result<Option<Segment>, Complaint> {
    let (address, key) = derive(stream, index);
    let Some(sealed) = waypoint.get(&address)? else {
        return Ok(None);
    };
    let plain = open(&key, &sealed)?;
    Ok(Some(Segment::from_canonical_bytes(&plain)?))
}

/// Walks a stream from height zero until the first empty address.
///
/// # Errors
///
/// Everything [`peek`] reports, plus [`Complaint::NotThePeer`] when a segment is
/// signed by somebody other than `author`, and [`Complaint::Chain`] when the
/// segments do not form a chain.
pub fn walk(
    waypoint: &impl Waypoint,
    stream: &Stream,
    author: &Handle,
    name: &str,
) -> Result<Walked, Complaint> {
    let mut verifier = Verifier::new();
    let mut held = Vec::new();

    for index in 0..u64::MAX {
        let Some(segment) = peek(waypoint, stream, index)? else {
            break;
        };
        if segment.author() != *author {
            return Err(Complaint::NotThePeer {
                name: name.to_owned(),
            });
        }
        verifier.accept(&segment)?;
        held.push(Held {
            address: derive(stream, index).0,
            segment,
        });
    }

    Ok(Walked { verifier, held })
}
