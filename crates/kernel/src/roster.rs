// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who is in a room, signed so that the claim travels with the founder's key.
//!
//! Beside `alias.rs` because it is the same shape: a claim about members,
//! signed by the key it belongs to, so every member reads one roster every
//! other member can check. A roster moved under another founder's key is a
//! forgery rather than a roster, because the founder's handle is inside what
//! was signed.
//!
//! **Members are keys, not handles.** A reader verifies every member's stream
//! against the key the roster names; a handle alone would leave nothing to
//! check a signature with, and a segment names its author without carrying the
//! key. Thirty-two keys is eighty-one kibibytes, which a veiled drop holds.
//!
//! **A roster never enters a payload.** It is metadata about the room,
//! exchanged at invitation and on change, and rendered outside the fence that
//! marks a member's own bytes; a list spliced into the text would be words any
//! member could forge in another member's half of the answer.

use crate::identity::{Handle, Signature, Signer, VerifyingKey};
use crate::wire::Reader;

/// Domain separation for the signature over a roster.
const ROOM_DOMAIN: &[u8] = b"kusanagi/room/1";

/// How wide an ML-DSA-87 signature is on the wire.
const SIGNATURE: usize = 4_627;

/// The most members one room holds. Thirty-two lanes is thirty-two streams a
/// reader matches against one sweep; beyond that a room stops being a room and
/// wants the multicast the protocol does not have.
pub const MOST_MEMBERS: usize = 32;

/// Who is in a room, as the founder signed it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Roster {
    members: Vec<VerifyingKey>,
    signature: Signature,
}

/// The bytes a roster signs: domain, the founder's handle, then every member.
fn claimed(founder: &Handle, members: &[VerifyingKey]) -> Vec<u8> {
    let mut out = ROOM_DOMAIN.to_vec();
    out.extend_from_slice(founder.as_bytes());
    for member in members {
        out.extend_from_slice(member.as_bytes());
    }
    out
}

impl Roster {
    /// Signs `members` as the room of `founder`'s making.
    ///
    /// # Errors
    ///
    /// [`RosterError::TooMany`] when the list names more members than a room
    /// holds: a roster that could not be written down is not signed.
    pub fn sign(founder: &Signer, members: Vec<VerifyingKey>) -> Result<Self, RosterError> {
        if members.len() > MOST_MEMBERS {
            return Err(RosterError::TooMany {
                count: members.len(),
                limit: MOST_MEMBERS,
            });
        }
        let signature = founder.sign(&claimed(&founder.handle(), &members));
        Ok(Self { members, signature })
    }

    /// The members, as claimed; [`Roster::verify`] is what makes them believed.
    #[must_use]
    pub fn members(&self) -> &[VerifyingKey] {
        &self.members
    }

    /// The wire form: `count u8 ‖ members ‖ signature`.
    ///
    /// # Errors
    ///
    /// [`RosterError::TooMany`] when the list names more members than a room
    /// holds, which [`Roster::sign`] refuses to make and [`Roster::from_bytes`]
    /// refuses to read; here it is the one guard on the count byte.
    pub fn to_bytes(&self) -> Result<Vec<u8>, RosterError> {
        let count = u8::try_from(self.members.len())
            .ok()
            .filter(|_| self.members.len() <= MOST_MEMBERS)
            .ok_or(RosterError::TooMany {
                count: self.members.len(),
                limit: MOST_MEMBERS,
            })?;
        let mut out = vec![count];
        for member in &self.members {
            out.extend_from_slice(member.as_bytes());
        }
        out.extend_from_slice(self.signature.as_bytes());
        Ok(out)
    }

    /// Reads what [`Roster::to_bytes`] wrote, without believing it.
    ///
    /// # Errors
    ///
    /// [`RosterError::Malformed`] when the bytes are not exactly a roster, and
    /// [`RosterError::TooMany`] when the list names more members than a room
    /// holds.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RosterError> {
        let mut reader = Reader::new(bytes);
        let roster = Self::read(&mut reader)?;
        if reader.remaining() != 0 {
            return Err(RosterError::Malformed);
        }
        Ok(roster)
    }

    /// Reads one roster out of `reader` and leaves whatever follows it.
    ///
    /// A roster says its own width — the count byte, then that many keys, then
    /// one signature — so a record that carries one needs no length in front
    /// of it, and no length field that a room of thirty-two could overflow.
    ///
    /// # Errors
    ///
    /// [`RosterError::Malformed`] when the bytes run out, and
    /// [`RosterError::TooMany`] when the count names more members than a room
    /// holds.
    pub fn read(reader: &mut Reader<'_>) -> Result<Self, RosterError> {
        let malformed = |_| RosterError::Malformed;
        let count = usize::from(reader.take_byte().map_err(malformed)?);
        if count > MOST_MEMBERS {
            return Err(RosterError::TooMany {
                count,
                limit: MOST_MEMBERS,
            });
        }
        let mut members = Vec::with_capacity(count);
        for _ in 0..count {
            let key = reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map_err(malformed)?;
            members.push(VerifyingKey::from_bytes(key));
        }
        let signature = Signature::from_bytes(reader.take_array::<SIGNATURE>().map_err(malformed)?);
        Ok(Self { members, signature })
    }

    /// The members, once `key` is shown to have signed them as its own room.
    ///
    /// # Errors
    ///
    /// [`RosterError::Forged`] when the signature is not `key`'s over these
    /// members and `key`'s handle — which is also what a roster moved from one
    /// founder to another fails with.
    pub fn verify(&self, key: &VerifyingKey) -> Result<&[VerifyingKey], RosterError> {
        key.verify(&claimed(&key.handle(), &self.members), &self.signature)
            .map_err(|_| RosterError::Forged)?;
        Ok(&self.members)
    }
}

/// Why a roster was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RosterError {
    /// The bytes are not a roster.
    #[error("these bytes are not a room roster")]
    Malformed,
    /// The list names more members than a room holds.
    #[error("a room holds at most {limit} members, not {count}")]
    TooMany {
        /// How many were named.
        count: usize,
        /// How many fit.
        limit: usize,
    },
    /// The signature is not the founder's over these members.
    #[error("this roster was not signed by the founder it is claimed for")]
    Forged,
}
