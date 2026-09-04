// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a segment carries, apart from the chain that holds it in place.
//!
//! Three facts travel together because a caller decides all three at once and a
//! constructor that took them apart would let two of them disagree: the bytes,
//! whether those bytes are anything anybody meant to say, and how far the author
//! had verified the other side when they wrote.
//!
//! **The acknowledgement is a count, not a height.** "I have verified three of
//! your segments" needs no sentinel for *none*, while "I have verified up to
//! height zero" and "I have verified nothing" are two different facts that a
//! `u64` height cannot tell apart. The count is what a release deletes against,
//! so the encoding that has no ambiguous case is the one that gets used.

use crate::payload::Payload;
use crate::segment::SegmentError;

/// Why a segment exists.
///
/// An enum rather than a flag on a payload because a reader has to act
/// differently: a message is reported and a filler is counted and dropped. A
/// boolean at the call site three functions away would say neither.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Purpose {
    /// The author meant to say this.
    Message,
    /// The author had nothing to say and the slot came round anyway.
    ///
    /// It is sealed, chained and counted exactly like a message, so an observer
    /// sees a stream that never goes quiet. It is never reported, so a reader
    /// sees only what somebody meant.
    Filler,
}

const PURPOSE_MESSAGE: u8 = 0;
const PURPOSE_FILLER: u8 = 1;

impl Purpose {
    /// The byte this purpose is written as.
    pub(crate) const fn byte(self) -> u8 {
        match self {
            Self::Message => PURPOSE_MESSAGE,
            Self::Filler => PURPOSE_FILLER,
        }
    }

    /// Reads the byte back.
    pub(crate) const fn of(byte: u8) -> Result<Self, SegmentError> {
        match byte {
            PURPOSE_MESSAGE => Ok(Self::Message),
            PURPOSE_FILLER => Ok(Self::Filler),
            other => Err(SegmentError::UnknownPurpose { purpose: other }),
        }
    }
}

/// Everything a segment carries that the chain does not decide.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Freight {
    pub(crate) payload: Payload,
    pub(crate) purpose: Purpose,
    pub(crate) acknowledged: u64,
}

impl Freight {
    /// Something the author meant to send, acknowledging nothing yet.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the bytes exceed
    /// [`MAX_PAYLOAD`](crate::MAX_PAYLOAD).
    pub fn message(payload: Vec<u8>) -> Result<Self, SegmentError> {
        Ok(Self {
            payload: Payload::new(payload)?,
            purpose: Purpose::Message,
            acknowledged: 0,
        })
    }

    /// A slot filled because it came round, carrying nothing.
    ///
    /// The payload is empty rather than random: the envelope above pads every
    /// drop to one size, so bytes spent here would buy nothing an observer could
    /// not already see.
    ///
    /// # Errors
    ///
    /// Never in practice; the signature matches [`Freight::message`] so that the
    /// two are interchangeable at a call site that chooses between them.
    pub fn filler() -> Result<Self, SegmentError> {
        Ok(Self {
            payload: Payload::new(Vec::new())?,
            purpose: Purpose::Filler,
            acknowledged: 0,
        })
    }

    /// Says how many of the reader's segments this author had verified.
    ///
    /// Carried inside the sealed part, so the host that holds the bytes learns
    /// nothing about how far either side has got.
    #[must_use]
    pub const fn acknowledging(mut self, verified: u64) -> Self {
        self.acknowledged = verified;
        self
    }
}
