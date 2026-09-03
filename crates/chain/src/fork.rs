// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Two segments that cannot both be true.

use core::fmt;

use kusanagi_kernel::{Handle, Segment, SegmentId};

/// Evidence that one author wrote two different segments at one height.
///
/// This is the cheapest intrusion signal the design has: an endpoint that forks
/// its own chain is visible to any two peers who compare heads, and no key is
/// needed to see it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Fork {
    author: Handle,
    index: u64,
    left: SegmentId,
    right: SegmentId,
}

impl Fork {
    /// Whose chain forked.
    #[must_use]
    pub const fn author(&self) -> Handle {
        self.author
    }

    /// The height at which it forked.
    #[must_use]
    pub const fn index(&self) -> u64 {
        self.index
    }

    /// One of the two segments.
    #[must_use]
    pub const fn left(&self) -> SegmentId {
        self.left
    }

    /// The other.
    #[must_use]
    pub const fn right(&self) -> SegmentId {
        self.right
    }
}

impl fmt::Display for Fork {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} wrote both {} and {} at height {}",
            self.author, self.left, self.right, self.index
        )
    }
}

/// Reports a fork, if these two segments are one.
///
/// Returns `None` when the two are the same segment: **a redelivery is not a
/// fork**, and treating it as one would make every retry look like an attack.
#[must_use]
pub fn fork(left: &Segment, right: &Segment) -> Option<Fork> {
    let (left_id, right_id) = (left.id(), right.id());
    let forked =
        left.author() == right.author() && left.index() == right.index() && left_id != right_id;
    forked.then(|| Fork {
        author: left.author(),
        index: left.index(),
        left: left_id,
        right: right_id,
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::fork;
    use kusanagi_kernel::{Segment, Signer, Trail};

    fn trail() -> Trail {
        Trail::from_seed([4_u8; 32])
    }

    fn alice() -> Signer {
        Signer::from_seed(&[1_u8; 32])
    }

    #[test]
    fn one_author_two_segments_one_height_is_a_fork() {
        let left = Segment::genesis(&alice(), &trail(), b"a".to_vec()).unwrap();
        let right = Segment::genesis(&alice(), &trail(), b"b".to_vec()).unwrap();
        let found = fork(&left, &right).unwrap();
        assert_eq!(found.author(), alice().handle());
        assert_eq!(found.index(), 0);
        assert_eq!(found.left(), left.id());
        assert_eq!(found.right(), right.id());
    }

    #[test]
    fn a_redelivery_is_not_a_fork() {
        let segment = Segment::genesis(&alice(), &trail(), b"a".to_vec()).unwrap();
        assert!(fork(&segment, &segment.clone()).is_none());
    }

    #[test]
    fn two_authors_are_not_a_fork() {
        let left = Segment::genesis(&alice(), &trail(), b"a".to_vec()).unwrap();
        let right =
            Segment::genesis(&Signer::from_seed(&[2_u8; 32]), &trail(), b"b".to_vec()).unwrap();
        assert!(fork(&left, &right).is_none());
    }

    #[test]
    fn two_heights_are_not_a_fork() {
        let first = Segment::genesis(&alice(), &trail(), b"a".to_vec()).unwrap();
        let second =
            Segment::extend(&trail(), alice().handle(), b"b".to_vec(), first.head()).unwrap();
        assert!(fork(&first, &second).is_none());
    }
}
