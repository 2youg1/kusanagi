// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! The only thing that travels, and the bytes that define it.
//!
//! A segment's identity is the hash of its canonical bytes, so the encoding must
//! be *canonical* in the strict sense: one segment, one byte string, forever. The
//! layout is fixed-width and big-endian, and decoding rejects trailing bytes —
//! without that rejection a segment would have infinitely many spellings and
//! content addressing would stop meaning anything.
//!
//! ```text
//! tag          1 byte   0 = Genesis, 1 = Follows
//! index        8 bytes  big endian; always 0 when tag = 0
//! previous    32 bytes  present only when tag = 1
//! author      32 bytes
//! payload_len  4 bytes  big endian
//! payload      payload_len bytes
//! ```

use core::num::NonZeroU64;

use crate::digest::identifier;
use crate::handle::Handle;

/// Domain separation for segment identity.
///
/// The version is part of the prefix so that a future layout change produces
/// different identifiers for the same bytes. Two encodings sharing one address
/// space is the failure this prevents.
const SEGMENT_DOMAIN: &[u8] = b"kusanagi.segment.v1";

const TAG_GENESIS: u8 = 0;
const TAG_FOLLOWS: u8 = 1;

/// The largest payload a single segment may carry, in bytes.
///
/// 64 KiB is the bulk bucket of the transport design. Anything larger is the job
/// of content-addressed chunking, which does not exist yet; until it does, a
/// larger payload is refused rather than silently split.
pub const MAX_PAYLOAD: u32 = 65_536;

identifier! {
    /// The content address of a segment.
    SegmentId, 32
}

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

/// A validated payload.
///
/// `len` is the same fact as `bytes.len()`, established once in [`Payload::new`]
/// and thereafter unchangeable because the struct has no mutator and no public
/// field. Caching it is what keeps [`Segment::to_canonical_bytes`] total: the
/// alternative is a fallible encoder, which would make the identity of a segment
/// a fallible question at every call site.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Payload {
    bytes: Box<[u8]>,
    len: u32,
}

impl Payload {
    fn new(bytes: Vec<u8>) -> Result<Self, SegmentError> {
        let len = u32::try_from(bytes.len()).map_err(|_| SegmentError::PayloadTooLarge {
            len: bytes.len(),
            limit: MAX_PAYLOAD,
        })?;
        if len > MAX_PAYLOAD {
            return Err(SegmentError::PayloadTooLarge {
                len: bytes.len(),
                limit: MAX_PAYLOAD,
            });
        }
        Ok(Self {
            bytes: bytes.into_boxed_slice(),
            len,
        })
    }
}

/// The only thing that crosses a boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Segment {
    link: Link,
    author: Handle,
    payload: Payload,
}

impl Segment {
    /// Starts a chain.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the payload exceeds [`MAX_PAYLOAD`].
    pub fn genesis(author: Handle, payload: Vec<u8>) -> Result<Self, SegmentError> {
        Ok(Self {
            link: Link::Genesis,
            author,
            payload: Payload::new(payload)?,
        })
    }

    /// Extends a chain by one.
    ///
    /// Takes a [`ChainHead`] rather than the predecessor itself, so extending a
    /// long chain does not require holding it.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the payload exceeds [`MAX_PAYLOAD`];
    /// [`SegmentError::ChainExhausted`] when the predecessor already sits at the
    /// last representable height.
    pub fn extend(author: Handle, payload: Vec<u8>, head: ChainHead) -> Result<Self, SegmentError> {
        let height = head
            .index
            .checked_add(1)
            .ok_or(SegmentError::ChainExhausted)?;
        // `height` is `head.index + 1`, hence at least one; the zero branch is
        // modelled rather than asserted away.
        let index = NonZeroU64::new(height).ok_or(SegmentError::ChainExhausted)?;
        Ok(Self {
            link: Link::Follows {
                index,
                previous: head.id,
            },
            author,
            payload: Payload::new(payload)?,
        })
    }

    /// This segment's content address.
    #[must_use]
    pub fn id(&self) -> SegmentId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SEGMENT_DOMAIN);
        hasher.update(&self.to_canonical_bytes());
        SegmentId::from_bytes(*hasher.finalize().as_bytes())
    }

    /// A witness of this segment, for whoever extends the chain next.
    #[must_use]
    pub fn head(&self) -> ChainHead {
        ChainHead {
            id: self.id(),
            index: self.index(),
        }
    }

    /// This segment's height in its chain; zero for a genesis segment.
    #[must_use]
    pub const fn index(&self) -> u64 {
        match self.link {
            Link::Genesis => 0,
            Link::Follows { index, .. } => index.get(),
        }
    }

    /// The segment directly beneath this one, absent only at genesis.
    #[must_use]
    pub const fn previous(&self) -> Option<SegmentId> {
        match self.link {
            Link::Genesis => None,
            Link::Follows { previous, .. } => Some(previous),
        }
    }

    /// Who wrote it.
    #[must_use]
    pub const fn author(&self) -> Handle {
        self.author
    }

    /// The opaque bytes this segment carries.
    #[must_use]
    pub const fn payload(&self) -> &[u8] {
        &self.payload.bytes
    }

    /// Encodes this segment into its one canonical byte string.
    #[must_use]
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        match self.link {
            Link::Genesis => {
                out.push(TAG_GENESIS);
                out.extend_from_slice(&0_u64.to_be_bytes());
            }
            Link::Follows { index, previous } => {
                out.push(TAG_FOLLOWS);
                out.extend_from_slice(&index.get().to_be_bytes());
                out.extend_from_slice(previous.as_bytes());
            }
        }
        out.extend_from_slice(self.author.as_bytes());
        out.extend_from_slice(&self.payload.len.to_be_bytes());
        out.extend_from_slice(&self.payload.bytes);
        out
    }

    /// Decodes a segment from its canonical byte string.
    ///
    /// # Errors
    ///
    /// Every malformed input has its own variant of [`SegmentError`]; nothing here
    /// panics, and trailing bytes are refused so that one segment keeps exactly
    /// one spelling.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut reader = Reader::new(bytes);
        let tag = reader.take_array::<1>()?;
        let tag = tag.first().copied().ok_or(SegmentError::Truncated)?;
        let height = u64::from_be_bytes(reader.take_array::<8>()?);

        let link = match tag {
            TAG_GENESIS => {
                if height != 0 {
                    return Err(SegmentError::GenesisIndexNotZero { index: height });
                }
                Link::Genesis
            }
            TAG_FOLLOWS => {
                let previous = SegmentId::from_bytes(reader.take_array::<32>()?);
                let index = NonZeroU64::new(height).ok_or(SegmentError::FollowsIndexZero)?;
                Link::Follows { index, previous }
            }
            other => return Err(SegmentError::UnknownTag { tag: other }),
        };

        let author = Handle::from_bytes(reader.take_array::<32>()?);
        let declared = u32::from_be_bytes(reader.take_array::<4>()?);
        let wanted = usize::try_from(declared)
            .map_err(|_| SegmentError::PayloadUnrepresentable { len: declared })?;
        let payload = reader.take(wanted)?.to_vec();

        if reader.remaining() != 0 {
            return Err(SegmentError::TrailingBytes {
                count: reader.remaining(),
            });
        }

        Ok(Self {
            link,
            author,
            payload: Payload::new(payload)?,
        })
    }
}

/// A cursor that cannot walk off the end of its input.
struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], SegmentError> {
        let end = self.at.checked_add(count).ok_or(SegmentError::Truncated)?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(SegmentError::Truncated)?;
        self.at = end;
        Ok(slice)
    }

    fn take_array<const N: usize>(&mut self) -> Result<[u8; N], SegmentError> {
        let slice = self.take(N)?;
        <[u8; N]>::try_from(slice).map_err(|_| SegmentError::Truncated)
    }

    fn remaining(&self) -> usize {
        self.bytes.len().saturating_sub(self.at)
    }
}

/// Why a segment could not be built or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentError {
    /// The input ended in the middle of a field.
    #[error("segment bytes end inside a field")]
    Truncated,
    /// Bytes remain after a complete segment.
    #[error("{count} byte(s) follow a complete segment; a segment has one spelling")]
    TrailingBytes {
        /// How many bytes were left over.
        count: usize,
    },
    /// The leading tag is neither genesis nor follows.
    #[error("unknown segment tag {tag}")]
    UnknownTag {
        /// The tag byte that was read.
        tag: u8,
    },
    /// A genesis segment declared a non-zero height.
    #[error("a genesis segment sits at height 0, not {index}")]
    GenesisIndexNotZero {
        /// The height that was declared.
        index: u64,
    },
    /// A following segment declared height zero.
    #[error("a following segment sits above height 0")]
    FollowsIndexZero,
    /// The payload exceeds [`MAX_PAYLOAD`].
    #[error("payload of {len} byte(s) exceeds the {limit}-byte limit")]
    PayloadTooLarge {
        /// The payload length that was offered.
        len: usize,
        /// The limit in force.
        limit: u32,
    },
    /// The declared payload length cannot be held by this platform.
    #[error("declared payload length {len} is not representable here")]
    PayloadUnrepresentable {
        /// The declared length.
        len: u32,
    },
    /// The predecessor already sits at the last representable height.
    #[error("this chain cannot be extended any further")]
    ChainExhausted,
}

impl SegmentError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Truncated => "segment.truncated",
            Self::TrailingBytes { .. } => "segment.trailing",
            Self::UnknownTag { .. } => "segment.tag",
            Self::GenesisIndexNotZero { .. } => "segment.genesis_index",
            Self::FollowsIndexZero => "segment.follows_index",
            Self::PayloadTooLarge { .. } => "segment.payload_too_large",
            Self::PayloadUnrepresentable { .. } => "segment.payload_unrepresentable",
            Self::ChainExhausted => "segment.exhausted",
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
    use super::{MAX_PAYLOAD, Segment, SegmentError};
    use crate::handle::Handle;

    fn alice() -> Handle {
        Handle::from_name("alice")
    }

    fn genesis() -> Segment {
        Segment::genesis(alice(), b"first".to_vec()).unwrap()
    }

    #[test]
    fn canonical_bytes_are_stable() {
        let segment = genesis();
        assert_eq!(segment.to_canonical_bytes(), segment.to_canonical_bytes());
    }

    #[test]
    fn genesis_round_trips() {
        let segment = genesis();
        let decoded = Segment::from_canonical_bytes(&segment.to_canonical_bytes()).unwrap();
        assert_eq!(decoded, segment);
        assert_eq!(decoded.id(), segment.id());
    }

    #[test]
    fn extend_round_trips_and_links() {
        let first = genesis();
        let second = Segment::extend(alice(), b"second".to_vec(), first.head()).unwrap();
        assert_eq!(second.index(), 1);
        assert_eq!(second.previous(), Some(first.id()));

        let decoded = Segment::from_canonical_bytes(&second.to_canonical_bytes()).unwrap();
        assert_eq!(decoded, second);
    }

    #[test]
    fn identity_follows_every_field() {
        let base = genesis();
        let other_author = Segment::genesis(Handle::from_name("bob"), b"first".to_vec()).unwrap();
        let other_payload = Segment::genesis(alice(), b"second".to_vec()).unwrap();
        let higher = Segment::extend(alice(), b"first".to_vec(), base.head()).unwrap();

        assert_ne!(base.id(), other_author.id());
        assert_ne!(base.id(), other_payload.id());
        assert_ne!(base.id(), higher.id());
    }

    #[test]
    fn empty_input_is_truncated() {
        assert_eq!(
            Segment::from_canonical_bytes(&[]),
            Err(SegmentError::Truncated)
        );
    }

    #[test]
    fn tag_only_is_truncated() {
        assert_eq!(
            Segment::from_canonical_bytes(&[0]),
            Err(SegmentError::Truncated)
        );
    }

    #[test]
    fn unknown_tag_is_named() {
        let mut bytes = genesis().to_canonical_bytes();
        bytes[0] = 2;
        assert_eq!(
            Segment::from_canonical_bytes(&bytes),
            Err(SegmentError::UnknownTag { tag: 2 })
        );
    }

    #[test]
    fn genesis_with_a_height_is_refused() {
        let mut bytes = genesis().to_canonical_bytes();
        bytes[8] = 7;
        assert_eq!(
            Segment::from_canonical_bytes(&bytes),
            Err(SegmentError::GenesisIndexNotZero { index: 7 })
        );
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = genesis().to_canonical_bytes();
        bytes.push(0);
        assert_eq!(
            Segment::from_canonical_bytes(&bytes),
            Err(SegmentError::TrailingBytes { count: 1 })
        );
    }

    #[test]
    fn a_lying_length_is_truncated_not_panicking() {
        let mut bytes = genesis().to_canonical_bytes();
        let length_at = bytes.len() - 5 - 4;
        bytes[length_at..length_at + 4].copy_from_slice(&1000_u32.to_be_bytes());
        assert_eq!(
            Segment::from_canonical_bytes(&bytes),
            Err(SegmentError::Truncated)
        );
    }

    #[test]
    fn an_oversized_payload_is_refused() {
        let payload = vec![0_u8; usize::try_from(MAX_PAYLOAD).unwrap() + 1];
        assert!(matches!(
            Segment::genesis(alice(), payload),
            Err(SegmentError::PayloadTooLarge { .. })
        ));
    }
}
