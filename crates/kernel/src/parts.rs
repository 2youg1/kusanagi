// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How something larger than one segment is said in several, and read back as
//! one.
//!
//! A drop is one fixed size, so a message larger than [`MAX_PAYLOAD`] has to
//! become a run of segments on the author's own lane. The run is ordinary in
//! every respect a host can see: each part is sealed, chained and filed exactly
//! like a sentence, and the host counts objects rather than messages.
//!
//! **The order is the chain's, and the header is the check.** Four bytes say
//! which part this is and how many there are, which the chain already implies —
//! carried anyway so that a run interrupted by a killed writer is *detected*
//! rather than silently joined to whatever the author says next.
//!
//! **A run that does not complete is not a message.** A reader cannot tell a
//! writer that died halfway from a writer that is still going, so neither is
//! reported and neither blocks the stream: the segments after it are read as
//! usual, and the author who wants those bytes read sends them again.
//!
//! How large a run a *sender* may write is not decided here. It is a question
//! about the ward the drops land in and who pays to download it, and it is
//! answered in `kusanagi`, which knows whose ward that is.

use crate::payload::MAX_PAYLOAD;
use crate::segment::{Freight, SegmentError};

/// How many bytes at the front of a part say where it sits in its run.
const HEADER: u32 = 4;

/// The same number for the places that count in `usize`.
const HEADER_WIDE: usize = 4;

/// How much of one part is the message rather than the header.
///
/// Four bytes less than a whole segment carries, which is why a message that
/// fits in one segment is never divided: it would lose those four bytes for
/// nothing.
pub const PART_ROOM: u32 = MAX_PAYLOAD.saturating_sub(HEADER);

/// One part of a run, as it was read off a segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Part<'a> {
    /// Where it sits, counting from zero.
    pub index: u16,
    /// How many parts the whole message has, always at least two.
    pub total: u16,
    /// The bytes of the message this part carries.
    pub bytes: &'a [u8],
}

impl<'a> Part<'a> {
    /// Reads a part's header off the payload of a [`Purpose::Part`] segment.
    ///
    /// [`None`] for a payload that does not carry a coherent header, which is a
    /// run to abandon rather than a stream to refuse: the bytes came from a peer
    /// and a peer is allowed to be broken without stopping everything after it.
    ///
    /// [`Purpose::Part`]: crate::Purpose::Part
    #[must_use]
    pub fn of(payload: &'a [u8]) -> Option<Self> {
        let (header, bytes) = payload.split_at_checked(HEADER_WIDE)?;
        let (index, total) = header.split_at_checked(2)?;
        let index = u16::from_be_bytes(<[u8; 2]>::try_from(index).ok()?);
        let total = u16::from_be_bytes(<[u8; 2]>::try_from(total).ok()?);
        // A run of one is a message that was not divided, and a part that claims
        // to sit at or past the end of its own run is nothing a reader can place.
        if total < 2 || index >= total {
            return None;
        }
        Some(Self {
            index,
            total,
            bytes,
        })
    }
}

impl Freight {
    /// One part of a run, carrying `bytes` as part `index` of `total`.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when `bytes` exceeds [`PART_ROOM`].
    pub fn part(index: u16, total: u16, bytes: &[u8]) -> Result<Self, SegmentError> {
        let mut payload = Vec::with_capacity(bytes.len().saturating_add(HEADER_WIDE));
        payload.extend_from_slice(&index.to_be_bytes());
        payload.extend_from_slice(&total.to_be_bytes());
        payload.extend_from_slice(bytes);
        Self::parted(payload)
    }
}

/// The segments one message becomes: itself, or the run that carries it.
///
/// `most` is how many segments the caller's ward may take for one message. The
/// number is the caller's to decide and is refused here rather than there so
/// that the refusal happens before anything is written, before a host is
/// opened, and with the limit in it.
///
/// # Errors
///
/// [`SegmentError::MessageTooLarge`] when `payload` needs more than `most`
/// parts, which is the failure a person sees when they send a file.
pub fn divide(payload: &[u8], most: u16) -> Result<Vec<Freight>, SegmentError> {
    let room = usize::try_from(PART_ROOM).unwrap_or(usize::MAX);
    let whole = usize::try_from(MAX_PAYLOAD).unwrap_or(usize::MAX);
    let limit = room.saturating_mul(usize::from(most));
    if payload.len() <= whole {
        return Ok(vec![Freight::message(payload.to_vec())?]);
    }
    if payload.len() > limit {
        return Err(SegmentError::MessageTooLarge {
            len: payload.len(),
            limit,
        });
    }
    // Bounded by `most` on the line above, so this conversion cannot narrow.
    let total =
        u16::try_from(payload.len().div_ceil(room)).map_err(|_| SegmentError::MessageTooLarge {
            len: payload.len(),
            limit,
        })?;
    payload
        .chunks(room)
        .enumerate()
        .map(|(at, chunk)| {
            let index = u16::try_from(at).map_err(|_| SegmentError::MessageTooLarge {
                len: payload.len(),
                limit,
            })?;
            Freight::part(index, total, chunk)
        })
        .collect()
}
