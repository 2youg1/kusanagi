// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

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
//! author      32 bytes  the author's handle
//! payload_len  4 bytes  big endian
//! payload      payload_len bytes
//! signature   64 bytes  by the author, over everything above
//! ```
//!
//! Everything above the signature is the *body*. A [`Segment`] value can only be
//! built by signing a body or by decoding one whose signature checks out, so
//! holding a segment is already proof that its author wrote it. There is no
//! "unverified segment" state for a later caller to forget about.

use core::num::NonZeroU64;

use crate::identifier;
use crate::identity::{Handle, NotAuthentic, Signature, Signer};
use crate::link::{ChainHead, Link};
use crate::wire::{Incomplete, Reader};

/// Domain separation for segment identity.
///
/// The version is part of the prefix so that a future layout change produces
/// different identifiers for the same bytes. Two encodings sharing one address
/// space is the failure this prevents.
const SEGMENT_DOMAIN: &[u8] = b"kusanagi.segment.v2";

/// Domain separation for what the author signs.
///
/// Distinct from [`SEGMENT_DOMAIN`] so that a segment identifier can never be
/// mistaken for something an author agreed to, in either direction.
const SIGNING_DOMAIN: &[u8] = b"kusanagi.segment.v2.sign";

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
    signature: Signature,
}

impl Segment {
    /// Starts a chain.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the payload exceeds [`MAX_PAYLOAD`].
    pub fn genesis(signer: &Signer, payload: Vec<u8>) -> Result<Self, SegmentError> {
        Self::sign(signer, Link::Genesis, payload)
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
    pub fn extend(
        signer: &Signer,
        payload: Vec<u8>,
        head: ChainHead,
    ) -> Result<Self, SegmentError> {
        let height = head
            .index()
            .checked_add(1)
            .ok_or(SegmentError::ChainExhausted)?;
        // `height` is `head.index + 1`, hence at least one; the zero branch is
        // modelled rather than asserted away.
        let index = NonZeroU64::new(height).ok_or(SegmentError::ChainExhausted)?;
        Self::sign(
            signer,
            Link::Follows {
                index,
                previous: head.id(),
            },
            payload,
        )
    }

    fn sign(signer: &Signer, link: Link, payload: Vec<u8>) -> Result<Self, SegmentError> {
        let author = signer.handle();
        let payload = Payload::new(payload)?;
        let signature = signer.sign(&signed_bytes(&body(link, &author, &payload)));
        Ok(Self {
            link,
            author,
            payload,
            signature,
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
        ChainHead::new(self.id(), self.index())
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

    /// Who wrote it, proven by the signature this value carries.
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
        let mut out = body(self.link, &self.author, &self.payload);
        out.extend_from_slice(self.signature.as_bytes());
        out
    }

    /// Decodes a segment from its canonical byte string, checking the signature.
    ///
    /// Re-encoding the body before checking the signature makes canonicity part
    /// of authenticity: bytes that decode to this segment but are not the bytes
    /// this segment encodes to produce a different signed message, and are
    /// therefore refused.
    ///
    /// # Errors
    ///
    /// Every malformed input has its own variant of [`SegmentError`]; nothing here
    /// panics, and trailing bytes are refused so that one segment keeps exactly
    /// one spelling.
    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut reader = Reader::new(bytes);
        let tag = reader.take_byte()?;
        let height = reader.take_u64()?;

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
        let declared = reader.take_u32()?;
        let wanted = usize::try_from(declared)
            .map_err(|_| SegmentError::PayloadUnrepresentable { len: declared })?;
        let payload = Payload::new(reader.take(wanted)?.to_vec())?;
        let signature = Signature::from_bytes(reader.take_array::<64>()?);

        if reader.remaining() != 0 {
            return Err(SegmentError::TrailingBytes {
                count: reader.remaining(),
            });
        }

        author.verify(&signed_bytes(&body(link, &author, &payload)), &signature)?;
        Ok(Self {
            link,
            author,
            payload,
            signature,
        })
    }
}

/// Everything a segment is, except the signature over it.
fn body(link: Link, author: &Handle, payload: &Payload) -> Vec<u8> {
    let mut out = Vec::new();
    match link {
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
    out.extend_from_slice(author.as_bytes());
    out.extend_from_slice(&payload.len.to_be_bytes());
    out.extend_from_slice(&payload.bytes);
    out
}

/// The exact message an author signs.
fn signed_bytes(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIGNING_DOMAIN.len().saturating_add(body.len()));
    out.extend_from_slice(SIGNING_DOMAIN);
    out.extend_from_slice(body);
    out
}

/// Why a segment could not be built or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SegmentError {
    /// The input ended in the middle of a field.
    #[error("segment bytes end inside a field: {0}")]
    Truncated(#[from] Incomplete),
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
    /// The signature does not cover these bytes under this author.
    #[error("this segment is not signed by the handle it names")]
    NotAuthentic(#[from] NotAuthentic),
    /// The predecessor already sits at the last representable height.
    #[error("this chain cannot be extended any further")]
    ChainExhausted,
}

impl SegmentError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Truncated(_) => "segment.truncated",
            Self::TrailingBytes { .. } => "segment.trailing",
            Self::UnknownTag { .. } => "segment.tag",
            Self::GenesisIndexNotZero { .. } => "segment.genesis_index",
            Self::FollowsIndexZero => "segment.follows_index",
            Self::PayloadTooLarge { .. } => "segment.payload_too_large",
            Self::PayloadUnrepresentable { .. } => "segment.payload_unrepresentable",
            Self::NotAuthentic(_) => "segment.not_authentic",
            Self::ChainExhausted => "segment.exhausted",
        }
    }
}
