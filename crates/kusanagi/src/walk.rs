// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! Reading a chain out of a waypoint, verifying it as it goes.

use kusanagi_chain::Verifier;
use kusanagi_kernel::{ChainHead, DropAddr, Handle, Segment, Waypoint, public_v0};

use crate::complaint::Complaint;

/// A chain as it was found on a waypoint.
pub struct Walked {
    verifier: Verifier,
    segments: Vec<(DropAddr, Segment)>,
}

impl Walked {
    /// The verified head, absent when the chain has not started.
    pub fn head(&self) -> Option<ChainHead> {
        self.verifier.head()
    }

    /// Every segment found, in order, with the address it was found at.
    pub fn segments(&self) -> &[(DropAddr, Segment)] {
        &self.segments
    }
}

/// Walks a handle's chain from height zero until the first empty address.
///
/// Probing address by address costs one request per segment, which is the honest
/// price of stage 0 having no index and no local state. Stage 3 replaces the
/// probe with the Bell; the shape of this function is what that stage has to beat.
///
/// # Errors
///
/// [`Complaint`] when the waypoint fails, when bytes at an address are not a
/// segment, or when the segments found do not form a chain.
pub fn chain(waypoint: &impl Waypoint, author: &Handle) -> Result<Walked, Complaint> {
    let mut verifier = Verifier::new();
    let mut segments = Vec::new();

    for index in 0..u64::MAX {
        let address = public_v0(author, index);
        let Some(bytes) = waypoint.get(&address)? else {
            break;
        };
        let segment = Segment::from_canonical_bytes(&bytes)?;
        verifier.accept(&segment)?;
        segments.push((address, segment));
    }

    Ok(Walked { verifier, segments })
}
