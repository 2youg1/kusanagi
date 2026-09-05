// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Every way a segment can fail to be built or read, and the code for each.
//!
//! One variant per failure rather than one per layer, because a caller acts on
//! *which* thing was wrong: a truncated drop is damage, a payload over the limit
//! is a program asking for something the format cannot carry, and a name that is
//! not the expected one is usually a host answering with somebody else's stream.
//!
//! Apart from `segment.rs` because it is a taxonomy rather than a mechanism, and
//! because the two together no longer fit in one reading.

use crate::identity::{Handle, NotAuthentic};
use crate::wire::Incomplete;

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
    /// The purpose byte is neither message nor filler.
    #[error("unknown segment purpose {purpose}")]
    UnknownPurpose {
        /// The purpose byte that was read.
        purpose: u8,
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
    /// The payload exceeds [`MAX_PAYLOAD`](crate::MAX_PAYLOAD).
    #[error("payload of {len} byte(s) exceeds the {limit}-byte limit")]
    PayloadTooLarge {
        /// The payload length that was offered.
        len: usize,
        /// The limit in force.
        limit: u32,
    },
    /// One message needs more segments than the caller allows it.
    ///
    /// Apart from [`Self::PayloadTooLarge`] because the two are different
    /// questions: that one is what a single segment can hold, which the format
    /// fixes, and this one is how much of a shared ward one message may take,
    /// which `kusanagi` decides.
    #[error(
        "{limit} bytes is all one message carries here, and this one is larger; \
         a file and a message are the same thing on this network"
    )]
    MessageTooLarge {
        /// How many bytes were offered.
        len: usize,
        /// The limit in force, in bytes.
        limit: usize,
    },
    /// The declared payload length cannot be held by this platform.
    #[error("declared payload length {len} is not representable here")]
    PayloadUnrepresentable {
        /// The declared length.
        len: u32,
    },
    /// The segment names an author other than the one the caller expected.
    #[error("this segment was written by {found}, and {expected} was expected")]
    NotTheAuthor {
        /// Whose key the caller offered.
        expected: Handle,
        /// Whose name the segment carries.
        found: Handle,
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
            Self::UnknownPurpose { .. } => "segment.purpose",
            Self::GenesisIndexNotZero { .. } => "segment.genesis_index",
            Self::FollowsIndexZero => "segment.follows_index",
            Self::PayloadTooLarge { .. } => "segment.payload_too_large",
            Self::MessageTooLarge { .. } => "segment.message_too_large",
            Self::PayloadUnrepresentable { .. } => "segment.payload_unrepresentable",
            Self::NotTheAuthor { .. } => "segment.not_the_author",
            Self::NotAuthentic(_) => "segment.not_authentic",
            Self::ChainExhausted => "segment.exhausted",
        }
    }
}
