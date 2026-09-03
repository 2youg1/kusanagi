// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How far one stream has been verified, in a form that survives the process.
//!
//! A reader that begins at height zero every time asks the host for every
//! address of a stream, in order, on one connection. The addresses are
//! unlinkable to each other right up to the moment somebody names them in that
//! order — so the walk hands a host with an access log exactly the grouping that
//! `seal` derives addresses to deny it. A cairn is what lets the next read start
//! where the last one stopped, so a poll names one address instead of all of
//! them.
//!
//! It is the verifier's whole resident state and nothing else. There is no
//! second structure describing "where I got to": [`Verifier`] suspends into a
//! cairn and resumes from one, so the thing written to disk and the thing held
//! in memory are the same thing.
//!
//! [`Verifier`]: crate::Verifier

use kusanagi_kernel::{ChainHead, Commitment, Handle, SegmentId};

/// The on-disk shape this build writes and reads.
const VERSION: u8 = 1;

/// Where the fields sit in the encoding.
const HANDLE_WIDTH: usize = 32;
const ID_WIDTH: usize = 32;
const INDEX_WIDTH: usize = 8;
const COMMIT_WIDTH: usize = 32;

/// One author, and the head of everything of theirs that has been verified.
///
/// Holding one is a statement about the past: *this endpoint verified that
/// author's chain up to that head.* Two things follow, and the second is the
/// reason this type is allowed to exist at all.
///
/// - **Resuming from a cairn does not re-check what is below it.** That is not a
///   weakening. It is the [`Tier::AckFirstSeen`] mitigation: a host that lets a
///   drop be rewritten cannot revise history a reader has already passed,
///   because the reader never looks again.
/// - **A corrupted cairn can only cause refusal.** Every use is a comparison
///   against a signed segment, so a wrong head makes the next segment fail to
///   link. It cannot make a forged segment verify.
///
/// [`Tier::AckFirstSeen`]: https://docs.rs/kusanagi-waypoint
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cairn {
    author: Handle,
    head: ChainHead,
}

impl Cairn {
    /// The exact width of the encoding, so a caller can reject a file by size.
    pub const WIDTH: usize = 1 + HANDLE_WIDTH + ID_WIDTH + INDEX_WIDTH + COMMIT_WIDTH;

    /// Records a verified position. Crate-private: the only way to obtain a
    /// cairn from outside is [`crate::Verifier::cairn`], which can only produce
    /// one after the segments below it have been verified.
    pub(crate) const fn new(author: Handle, head: ChainHead) -> Self {
        Self { author, head }
    }

    /// Whose chain this describes.
    #[must_use]
    pub const fn author(&self) -> Handle {
        self.author
    }

    /// The verified head.
    #[must_use]
    pub const fn head(&self) -> ChainHead {
        self.head
    }

    /// The first height above this cairn: where a sender writes next, and where
    /// a reader resumes.
    ///
    /// `None` when the head already sits at the last height a `u64` can express.
    /// That is not a failure but a fact about the chain — nothing can follow it —
    /// and a caller that asks for what comes next is answered rather than
    /// interrupted.
    #[must_use]
    pub const fn next_index(&self) -> Option<u64> {
        self.head.index().checked_add(1)
    }

    /// The bytes to write down.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(Self::WIDTH);
        out.push(VERSION);
        out.extend_from_slice(self.author.as_bytes());
        out.extend_from_slice(self.head.id().as_bytes());
        out.extend_from_slice(&self.head.index().to_be_bytes());
        out.extend_from_slice(self.head.awaited().as_bytes());
        out
    }

    /// Reads back what [`Self::to_bytes`] wrote.
    ///
    /// The head this produces is [`ChainHead::recorded`] rather than a witness:
    /// it is this endpoint's own note, and the asymmetry that makes that safe is
    /// documented there.
    ///
    /// # Errors
    ///
    /// [`CairnError::Version`] for a record this build does not read, and
    /// [`CairnError::Width`] for anything that is not exactly [`Self::WIDTH`]
    /// bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, CairnError> {
        let width = bytes.len();
        let malformed = || CairnError::Width { found: width };

        let (version, rest) = bytes.split_first().ok_or_else(malformed)?;
        if *version != VERSION {
            return Err(CairnError::Version { found: *version });
        }
        let (author, rest) = rest.split_at_checked(HANDLE_WIDTH).ok_or_else(malformed)?;
        let (id, rest) = rest.split_at_checked(ID_WIDTH).ok_or_else(malformed)?;
        let (index, awaited) = rest.split_at_checked(INDEX_WIDTH).ok_or_else(malformed)?;

        let author: [u8; HANDLE_WIDTH] = author.try_into().map_err(|_| malformed())?;
        let id: [u8; ID_WIDTH] = id.try_into().map_err(|_| malformed())?;
        let index: [u8; INDEX_WIDTH] = index.try_into().map_err(|_| malformed())?;
        let awaited: [u8; COMMIT_WIDTH] = awaited.try_into().map_err(|_| malformed())?;

        Ok(Self {
            author: Handle::from_bytes(author),
            head: ChainHead::recorded(
                SegmentId::from_bytes(id),
                u64::from_be_bytes(index),
                Commitment::from_bytes(awaited),
            ),
        })
    }
}

/// Why a cairn could not be read, or could not be carried forward.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum CairnError {
    /// The record was written by a build with a different on-disk shape.
    #[error("this cairn is version {found}, and this build reads {VERSION}")]
    Version {
        /// The version byte that was found.
        found: u8,
    },
    /// The record is not the one width a cairn has.
    #[error("a cairn is {} bytes, and this is {found}", Cairn::WIDTH)]
    Width {
        /// How many bytes were offered.
        found: usize,
    },
}

impl CairnError {
    /// The stable code a caller reports and a script matches on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Version { .. } => "cairn.version",
            Self::Width { .. } => "cairn.width",
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod tests {
    use super::{Cairn, HANDLE_WIDTH, ID_WIDTH, INDEX_WIDTH};
    use kusanagi_kernel::{Segment, Signer, Trail};

    /// A cairn over a chain actually built to `height`, so that the head in it is
    /// a witness rather than an assertion.
    ///
    /// `u8` rather than `u64` because this walks the chain it builds: the type is
    /// what stops a caller asking for a height that would take hours, which is a
    /// mistake this file has already made once.
    fn cairn_at(height: u8) -> Cairn {
        let signer = Signer::from_seed(&[3_u8; 32]);
        let trail = Trail::from_seed([4_u8; 32]);
        let mut segment = Segment::genesis(&signer, &trail, b"genesis".to_vec()).unwrap();
        for _ in 0..height {
            segment =
                Segment::extend(&trail, signer.handle(), b"more".to_vec(), segment.head()).unwrap();
        }
        Cairn::new(signer.handle(), segment.head())
    }

    /// The same cairn with its height rewritten, for the heights a chain cannot
    /// be built to in a test.
    fn cairn_claiming(height: u64) -> Cairn {
        let mut bytes = cairn_at(0).to_bytes();
        // The height sits after the version, the author and the segment id, and
        // before the commitment that closes the record.
        let start = 1 + HANDLE_WIDTH + ID_WIDTH;
        bytes[start..start + INDEX_WIDTH].copy_from_slice(&height.to_be_bytes());
        Cairn::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn a_cairn_survives_the_round_trip() {
        let written = cairn_at(9);
        let read = Cairn::from_bytes(&written.to_bytes()).unwrap();
        assert_eq!(read, written);
        assert_eq!(read.head().index(), 9);
    }

    #[test]
    fn a_cairn_is_exactly_one_width() {
        // The encoding is fixed-width, so the height it carries cannot change the
        // size of the file and a caller may reject a record by its length alone.
        assert_eq!(cairn_at(0).to_bytes().len(), Cairn::WIDTH);
        assert_eq!(cairn_claiming(u64::MAX).to_bytes().len(), Cairn::WIDTH);
    }

    #[test]
    fn a_record_of_another_version_is_refused_rather_than_guessed() {
        let mut bytes = cairn_at(1).to_bytes();
        bytes[0] = 99;
        let refused = Cairn::from_bytes(&bytes).unwrap_err();
        assert_eq!(refused.code(), "cairn.version");
    }

    #[test]
    fn a_truncated_or_padded_record_is_refused() {
        let bytes = cairn_at(1).to_bytes();
        for width in [0, 1, Cairn::WIDTH - 1, Cairn::WIDTH + 1] {
            let mut candidate = bytes.clone();
            candidate.resize(width, 0);
            if width > 0 {
                candidate[0] = super::VERSION;
            }
            let refused = Cairn::from_bytes(&candidate).unwrap_err();
            assert_eq!(refused.code(), "cairn.width", "width {width} was accepted");
        }
    }

    #[test]
    fn the_next_index_is_one_above_the_head() {
        assert_eq!(cairn_at(0).next_index(), Some(1));
        assert_eq!(cairn_at(4).next_index(), Some(5));
    }

    #[test]
    fn nothing_follows_a_chain_at_the_last_height() {
        assert_eq!(cairn_claiming(u64::MAX).next_index(), None);
    }
}
