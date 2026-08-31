// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! Deciding whether a sequence of segments is a chain, in constant memory.

use kusanagi_kernel::{ChainHead, Handle, Segment, SegmentId};

/// What the verifier remembers: exactly one author and one head.
#[derive(Clone, Copy, Debug)]
struct Seen {
    author: Handle,
    head: ChainHead,
}

/// Accepts segments in order and reports the first one that breaks the chain.
///
/// The resident state is a single `Option`, so a chain of a million segments
/// costs the same as a chain of one. Nothing is buffered, which also means the
/// caller decides what to do with segments that arrive out of order — reordering
/// is somebody else's job, and pretending otherwise here would cost the memory
/// property that makes this type worth having.
#[derive(Clone, Copy, Debug, Default)]
pub struct Verifier {
    seen: Option<Seen>,
}

impl Verifier {
    /// A verifier that has seen nothing.
    #[must_use]
    pub const fn new() -> Self {
        Self { seen: None }
    }

    /// Accepts the next segment.
    ///
    /// On failure the verifier is left exactly as it was, so a caller may discard
    /// the offending segment and carry on.
    ///
    /// # Errors
    ///
    /// One variant of [`ChainError`] per way a segment can fail to follow its
    /// predecessor.
    pub fn accept(&mut self, segment: &Segment) -> Result<(), ChainError> {
        // The four combinations are exhaustive on purpose: a `_` arm here would
        // silently swallow whatever state is added later.
        match (self.seen, segment.previous()) {
            (None, None) => {}
            (None, Some(_)) => {
                return Err(ChainError::ExpectedGenesis {
                    index: segment.index(),
                });
            }
            (Some(_), None) => return Err(ChainError::UnexpectedGenesis),
            (Some(seen), Some(previous)) => {
                // Author first: if the author is wrong, a height or hash mismatch
                // is a true statement that points at the wrong problem.
                if seen.author != segment.author() {
                    return Err(ChainError::AuthorChanged {
                        expected: seen.author,
                        found: segment.author(),
                    });
                }
                let expected = seen
                    .head
                    .index()
                    .checked_add(1)
                    .ok_or(ChainError::Exhausted)?;
                if segment.index() != expected {
                    return Err(ChainError::IndexGap {
                        expected,
                        found: segment.index(),
                    });
                }
                if previous != seen.head.id() {
                    return Err(ChainError::PreviousMismatch {
                        index: segment.index(),
                        expected: seen.head.id(),
                        found: previous,
                    });
                }
            }
        }

        self.seen = Some(Seen {
            author: segment.author(),
            head: segment.head(),
        });
        Ok(())
    }

    /// The head of everything accepted so far.
    #[must_use]
    pub fn head(&self) -> Option<ChainHead> {
        self.seen.map(|seen| seen.head)
    }

    /// The author of everything accepted so far.
    #[must_use]
    pub fn author(&self) -> Option<Handle> {
        self.seen.map(|seen| seen.author)
    }
}

/// Verifies a whole sequence and returns the verifier that consumed it.
///
/// # Errors
///
/// The first [`ChainError`] the sequence produces; later segments are not read.
pub fn verify<'a, I>(segments: I) -> Result<Verifier, ChainError>
where
    I: IntoIterator<Item = &'a Segment>,
{
    let mut verifier = Verifier::new();
    for segment in segments {
        verifier.accept(segment)?;
    }
    Ok(verifier)
}

/// Why a sequence of segments is not a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ChainError {
    /// The first segment was not a genesis segment.
    #[error("a chain opens with a genesis segment, not one at height {index}")]
    ExpectedGenesis {
        /// The height the first segment claimed.
        index: u64,
    },
    /// A genesis segment appeared after the chain had already opened.
    #[error("a chain has one genesis segment, and it is the first")]
    UnexpectedGenesis,
    /// A height was skipped or repeated.
    #[error("expected the segment at height {expected}, found one at {found}")]
    IndexGap {
        /// The height the chain was waiting for.
        expected: u64,
        /// The height that arrived.
        found: u64,
    },
    /// A segment points at something other than its predecessor.
    #[error("the segment at height {index} points at {found}, not at {expected}")]
    PreviousMismatch {
        /// Where the break was found.
        index: u64,
        /// The predecessor's actual identity.
        expected: SegmentId,
        /// The identity the segment claimed.
        found: SegmentId,
    },
    /// The author changed mid-chain.
    #[error("this chain belongs to {expected}, but a segment by {found} arrived")]
    AuthorChanged {
        /// Whose chain this is.
        expected: Handle,
        /// Who wrote the offending segment.
        found: Handle,
    },
    /// The chain already sits at the last representable height.
    #[error("this chain cannot be extended any further")]
    Exhausted,
}

impl ChainError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::ExpectedGenesis { .. } => "chain.expected_genesis",
            Self::UnexpectedGenesis => "chain.unexpected_genesis",
            Self::IndexGap { .. } => "chain.index_gap",
            Self::PreviousMismatch { .. } => "chain.previous_mismatch",
            Self::AuthorChanged { .. } => "chain.author_changed",
            Self::Exhausted => "chain.exhausted",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]
mod tests {
    use super::{ChainError, Verifier, verify};
    use kusanagi_kernel::{Handle, Segment};

    fn alice() -> Handle {
        Handle::from_name("alice")
    }

    fn chain_of(length: usize) -> Vec<Segment> {
        let mut segments = vec![Segment::genesis(alice(), b"0".to_vec()).unwrap()];
        for step in 1..length {
            let head = segments.last().unwrap().head();
            segments.push(Segment::extend(alice(), step.to_string().into_bytes(), head).unwrap());
        }
        segments
    }

    #[test]
    fn an_empty_sequence_is_a_chain_with_no_head() {
        let verifier = verify(&[]).unwrap();
        assert!(verifier.head().is_none());
        assert!(verifier.author().is_none());
    }

    #[test]
    fn a_ten_segment_chain_verifies() {
        let segments = chain_of(10);
        let verifier = verify(&segments).unwrap();
        assert_eq!(verifier.head(), Some(segments[9].head()));
        assert_eq!(verifier.author(), Some(alice()));
    }

    #[test]
    fn a_chain_must_open_with_genesis() {
        let segments = chain_of(3);
        assert_eq!(
            verify(&segments[1..]).unwrap_err(),
            ChainError::ExpectedGenesis { index: 1 }
        );
    }

    #[test]
    fn a_second_genesis_is_refused() {
        let first = Segment::genesis(alice(), b"a".to_vec()).unwrap();
        let second = Segment::genesis(alice(), b"b".to_vec()).unwrap();
        assert_eq!(
            verify(&[first, second]).unwrap_err(),
            ChainError::UnexpectedGenesis
        );
    }

    #[test]
    fn a_skipped_height_is_named() {
        let segments = chain_of(4);
        let gapped = [
            segments[0].clone(),
            segments[1].clone(),
            segments[3].clone(),
        ];
        assert_eq!(
            verify(&gapped).unwrap_err(),
            ChainError::IndexGap {
                expected: 2,
                found: 3
            }
        );
    }

    #[test]
    fn a_wrong_predecessor_is_named() {
        let genesis = Segment::genesis(alice(), b"a".to_vec()).unwrap();
        let other = Segment::genesis(alice(), b"b".to_vec()).unwrap();
        let forged = Segment::extend(alice(), b"c".to_vec(), other.head()).unwrap();
        assert!(matches!(
            verify(&[genesis, forged]).unwrap_err(),
            ChainError::PreviousMismatch { index: 1, .. }
        ));
    }

    #[test]
    fn a_changed_author_is_named() {
        let genesis = Segment::genesis(alice(), b"a".to_vec()).unwrap();
        let bob = Handle::from_name("bob");
        let intruder = Segment::extend(bob, b"b".to_vec(), genesis.head()).unwrap();
        assert_eq!(
            verify(&[genesis, intruder]).unwrap_err(),
            ChainError::AuthorChanged {
                expected: alice(),
                found: bob
            }
        );
    }

    #[test]
    fn a_rejected_segment_does_not_become_the_head() {
        let segments = chain_of(2);
        let mut verifier = Verifier::new();
        verifier.accept(&segments[0]).unwrap();
        let head_before = verifier.head();

        let stranger = Segment::genesis(alice(), b"stranger".to_vec()).unwrap();
        assert!(verifier.accept(&stranger).is_err());
        assert_eq!(verifier.head(), head_before);

        // and the verifier still accepts the segment it was actually waiting for
        verifier.accept(&segments[1]).unwrap();
        assert_eq!(verifier.head(), Some(segments[1].head()));
    }
}
