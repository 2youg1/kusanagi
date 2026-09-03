// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How much a segment may carry, and the type that establishes it once.
//!
//! Three numbers and one newtype, kept together because they are one fact seen
//! from three sides: the envelope in `kusanagi_seal` fixes how large a sealed
//! drop is, the fixed fields of a segment take their share, and what is left is
//! what an author may actually say.

use crate::segment::SegmentError;

/// The fixed part of a segment's canonical bytes, in bytes.
///
/// tag 1 + index 8 + previous 32 + author 32 + `payload_len` 4 + signature 64. A
/// genesis segment is 32 bytes shorter because it carries no predecessor, and
/// the envelope in `seal` hides that difference along with every other one.
pub(crate) const OVERHEAD: u32 = 141;

/// The largest canonical byte string a segment can have, in bytes.
///
/// This is the number `seal` builds its envelope around, so it is stated here as
/// a length rather than derived at each use. The two are tied by a compile-time
/// assertion in `kusanagi_seal::veil`: change one without the other and the
/// workspace stops building.
pub const MAX_SEGMENT: usize = 65_516;

/// The largest payload a single segment may carry, in bytes.
///
/// **This number is not chosen; it is what is left over.** Every sealed drop is
/// one fixed size, because a size that varies is a measurement a host can take
/// without any cryptanalysis at all — and a ladder of sizes is still a
/// measurement, only a coarser one. Fixing the drop at 64 KiB and subtracting
/// the authentication tag, the length prefix and [`OVERHEAD`] leaves exactly
/// this much room for what an author actually wants to say.
///
/// A payload larger than this is the job of content-addressed chunking, which
/// does not exist yet; until it does, a larger payload is refused rather than
/// silently split.
pub const MAX_PAYLOAD: u32 = 65_375;

const _: () = assert!(
    MAX_SEGMENT == 65_516 && OVERHEAD + MAX_PAYLOAD == 65_516,
    "MAX_SEGMENT and MAX_PAYLOAD disagree about how large a segment can be"
);

/// A validated payload.
///
/// `len` is the same fact as `bytes.len()`, established once in [`Payload::new`]
/// and thereafter unchangeable because the struct has no mutator and no public
/// field. Caching it is what keeps `Segment::to_canonical_bytes` total: the
/// alternative is a fallible encoder, which would make the identity of a segment
/// a fallible question at every call site.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct Payload {
    bytes: Box<[u8]>,
    len: u32,
}

impl Payload {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self, SegmentError> {
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

    /// The opaque bytes themselves.
    pub(crate) const fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// How many of them, as the four bytes that go on the wire.
    pub(crate) const fn declared_len(&self) -> [u8; 4] {
        self.len.to_be_bytes()
    }
}
