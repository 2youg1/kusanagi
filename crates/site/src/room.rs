// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One room, as this endpoint knows it.
//!
//! A room holds what cannot be derived from anything else here: the shared
//! secret every member's lane derives from, the ward every member sweeps, the
//! signed roster that says who is in it and the founder's height it was taken
//! at, the one-time keys of invitations still open, the locator of the host
//! that holds the bytes, and the period it was opened in — where a reader with
//! no sweep record starts.
//!
//! Apart from `channel.rs` because the two change for different reasons: a
//! channel record gains a field when this endpoint learns something new about
//! one conversation, and a room gains one when it learns something new about a
//! crowd. The roster codec sits beside the one struct that carries it.
//!
//! ```text
//! version     1 byte    = 3
//! name_len    2 bytes   big endian, then that many utf-8 bytes
//! secret     32 bytes   every member's lane derives from here
//! ward        2 bytes   the bin of the host every member sweeps
//! roster      n bytes   the founder-signed member list, which says its own width
//! roster_at   9 bytes   0, or 1 then the founder's height the roster was read at
//! ushers    1+n bytes   a count, then that many one-time verifying keys
//! locator_len 2 bytes   big endian, then that many utf-8 bytes
//! opened      8 bytes   big endian; the period this record was made in
//! ```
//!
//! **The roster and the ushers carry no length field**, because both say their
//! own width and both can exceed what a two-byte length holds: thirty-two keys
//! are eighty-one kibibytes.
//!
//! The name is in the record because it is no longer in the file name, for the
//! same reason as a channel's: a directory listing says how many rooms there
//! are and nothing about who is in them.

use kusanagi_kernel::{Handle, Period, Reader, Roster, RosterError, VerifyingKey, Ward};
use kusanagi_seal::Secret;

use crate::blocks::{malformed, put_block, take_text};
use crate::error::SiteError;
use crate::invite::mangled;

/// The record this build writes and reads.
const VERSION: u8 = 3;

/// One room, as this endpoint knows it.
#[derive(Clone, Debug)]
pub struct Room {
    /// What this endpoint calls the room. Local, and never sent anywhere.
    pub name: String,
    /// Every member's lane derives from here, through their own handle.
    pub secret: Secret,
    /// Which bin of the host every member sweeps.
    pub ward: Ward,
    /// Who is in the room, signed by the founder.
    pub roster: Roster,
    /// The height on the founder's stream this roster was read at, or none
    /// when it came with the invitation. A read walks the founder from here,
    /// so a roster segment met and not yet written down is met again.
    pub roster_at: Option<u64>,
    /// The one-time keys whose streams carry greetings: one per invitation
    /// this endpoint minted. A newcomer writes on the stream of the key their
    /// invitation carried, and the founder reads every one to learn who
    /// arrived — the same shape as a channel's single introduction key,
    /// with one entry per invitation instead of one.
    pub ushers: Vec<VerifyingKey>,
    /// Where the bytes live.
    pub locator: String,
    /// The period this record was made in, before which no drop of this room
    /// can be filed.
    pub opened: Period,
}

impl Room {
    /// The founder: the first member of the roster, whose key signed it.
    #[must_use]
    pub fn founder(&self) -> Option<Handle> {
        self.roster.members().first().map(VerifyingKey::handle)
    }

    /// The wire form, which is also the on-disk form.
    ///
    /// # Errors
    ///
    /// [`RosterError::TooMany`] when the roster or the open invitations name
    /// more than a room holds: neither can be written down.
    pub fn to_bytes(&self) -> Result<Vec<u8>, RosterError> {
        let mut out = vec![VERSION];
        put_block(&mut out, self.name.as_bytes());
        out.extend_from_slice(self.secret.as_bytes());
        out.extend_from_slice(&self.ward.bits().to_be_bytes());
        out.extend_from_slice(&self.roster.to_bytes()?);
        match self.roster_at {
            None => out.extend_from_slice(&[0; 9]),
            Some(height) => {
                out.push(1);
                out.extend_from_slice(&height.to_be_bytes());
            }
        }
        put_keys(&mut out, &self.ushers)?;
        put_block(&mut out, self.locator.as_bytes());
        out.extend_from_slice(&self.opened.count().to_be_bytes());
        Ok(out)
    }

    /// Reads the wire form.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadRecord`] for any shape this decoder does not recognise,
    /// including a version it was not written for.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SiteError> {
        let mut reader = Reader::new(bytes);
        let version = reader.take_byte().map_err(malformed)?;
        if version != VERSION {
            return Err(SiteError::BadRecord {
                what: "a room",
                reason: format!("this file is version {version}, and this build reads {VERSION}"),
            });
        }
        let name = take_text(&mut reader, "a room name")?;
        let secret = Secret::from_bytes(reader.take_array::<32>().map_err(malformed)?);
        let ward = Ward::from_bits(u16::from_be_bytes(
            reader.take_array::<2>().map_err(malformed)?,
        ));
        let roster = Roster::read(&mut reader).map_err(|error| SiteError::BadRecord {
            what: "a room roster",
            reason: error.to_string(),
        })?;
        let roster_at = match reader.take_byte().map_err(malformed)? {
            0 => {
                reader.take_array::<8>().map_err(malformed)?;
                None
            }
            1 => Some(u64::from_be_bytes(
                reader.take_array::<8>().map_err(malformed)?,
            )),
            other => {
                return Err(SiteError::BadRecord {
                    what: "a room",
                    reason: format!(
                        "a roster height is marked {other}, and this build knows 0 and 1"
                    ),
                });
            }
        };
        let ushers = take_keys(&mut reader)?;
        let locator = take_text(&mut reader, "a locator")?;
        let opened = Period::from_count(u64::from_be_bytes(
            reader.take_array::<8>().map_err(malformed)?,
        ));
        if reader.remaining() != 0 {
            return Err(SiteError::BadRecord {
                what: "a room",
                reason: format!("{} byte(s) follow a complete record", reader.remaining()),
            });
        }
        Ok(Self {
            name,
            secret,
            ward,
            roster,
            roster_at,
            ushers,
            locator,
            opened,
        })
    }
}

/// Writes `count u8 ‖ keys`: a list that says its own width.
fn put_keys(out: &mut Vec<u8>, keys: &[VerifyingKey]) -> Result<(), RosterError> {
    let count = u8::try_from(keys.len())
        .ok()
        .filter(|_| keys.len() <= kusanagi_kernel::MOST_MEMBERS)
        .ok_or(RosterError::TooMany {
            count: keys.len(),
            limit: kusanagi_kernel::MOST_MEMBERS,
        })?;
    out.push(count);
    for key in keys {
        out.extend_from_slice(key.as_bytes());
    }
    Ok(())
}

/// Reads what [`put_keys`] wrote.
fn take_keys(reader: &mut Reader<'_>) -> Result<Vec<VerifyingKey>, SiteError> {
    let count = usize::from(reader.take_byte().map_err(malformed)?);
    (0..count)
        .map(|_| {
            reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map(VerifyingKey::from_bytes)
                .map_err(malformed)
        })
        .collect()
}

/// What a room invitation points at: who founded the room, which ward every
/// member sweeps, and the signed roster that says who is in it.
///
/// Sealed into one drop at the address [`kusanagi_seal::offer`] derives from
/// the room secret. The room secret is what the invitation line carries, so
/// only the holder of the line can compute the address; the founder's key and
/// the roster beside it are public by construction, like a channel offer's.
///
/// ```text
/// version   1 byte     = 2
/// founder 2592 bytes    the founder's verifying key
/// ward       2 bytes    the bin of the host every member sweeps
/// roster     n bytes    the founder-signed member list, which says its own width
/// ```
#[derive(Clone, Debug)]
pub struct RoomOffer {
    /// Who founded the room, and whose key signs the roster.
    pub founder: VerifyingKey,
    /// Which bin of the host every member sweeps.
    pub ward: Ward,
    /// Who is in the room, signed by the key above.
    pub roster: Roster,
}

/// The layout of a room offer, versioned apart from the invitation's own.
const ROOM_OFFER_VERSION: u8 = 2;

impl RoomOffer {
    /// The bytes that go in the drop.
    ///
    /// # Errors
    ///
    /// [`RosterError::TooMany`] when the roster names more than a room holds:
    /// it cannot be sealed into an offer.
    pub fn to_bytes(&self) -> Result<Vec<u8>, RosterError> {
        let mut out = vec![ROOM_OFFER_VERSION];
        out.extend_from_slice(self.founder.as_bytes());
        out.extend_from_slice(&self.ward.bits().to_be_bytes());
        out.extend_from_slice(&self.roster.to_bytes()?);
        Ok(out)
    }

    /// Reads what [`Self::to_bytes`] wrote.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadInvitation`] when the bytes are not a room offer this
    /// build reads.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SiteError> {
        let mut reader = Reader::new(bytes);
        let version = reader.take_byte().map_err(mangled)?;
        if version != ROOM_OFFER_VERSION {
            return Err(SiteError::BadInvitation {
                reason: format!(
                    "this invitation points at a version {version} room offer; this build reads {ROOM_OFFER_VERSION}"
                ),
            });
        }
        let founder = VerifyingKey::from_bytes(
            reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map_err(mangled)?,
        );
        let ward = Ward::from_bits(u16::from_be_bytes(
            reader.take_array::<2>().map_err(mangled)?,
        ));
        let roster = Roster::read(&mut reader).map_err(|error| SiteError::BadInvitation {
            reason: error.to_string(),
        })?;
        if reader.remaining() != 0 {
            return Err(SiteError::BadInvitation {
                reason: format!(
                    "{} byte(s) follow a complete room offer",
                    reader.remaining()
                ),
            });
        }
        Ok(Self {
            founder,
            ward,
            roster,
        })
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::{Room, VERSION};
    use kusanagi_kernel::{Period, Roster, Signer, Ward};
    use kusanagi_seal::Secret;

    fn room() -> Room {
        let founder = Signer::from_seed(&[7; 32]);
        let bob = Signer::from_seed(&[8; 32]);
        Room {
            name: "team".to_owned(),
            secret: Secret::from_bytes([11; 32]),
            ward: Ward::from_bits(0x00ab),
            roster: Roster::sign(&founder, vec![founder.verifying_key(), bob.verifying_key()])
                .unwrap(),
            roster_at: Some(4),
            ushers: vec![founder.verifying_key()],
            locator: "http://box.example:8963".to_owned(),
            opened: Period::from_count(2_945_376),
        }
    }

    #[test]
    fn a_room_round_trips() {
        let original = room();
        let bytes = original.to_bytes().unwrap();
        let decoded = Room::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.to_bytes().unwrap(), bytes);
        assert_eq!(decoded.name, "team");
        assert_eq!(decoded.roster.members().len(), 2);
        assert_eq!(decoded.roster_at, Some(4));
        assert_eq!(
            decoded.founder(),
            Some(Signer::from_seed(&[7; 32]).handle())
        );
    }

    #[test]
    fn a_full_room_of_thirty_two_round_trips_in_a_record_and_in_an_offer() {
        use super::RoomOffer;
        use kusanagi_kernel::MOST_MEMBERS;
        let founder = Signer::from_seed(&[7; 32]);
        let members: Vec<_> = (0..MOST_MEMBERS)
            .map(|seed| Signer::from_seed(&[u8::try_from(seed).unwrap(); 32]).verifying_key())
            .collect();
        let mut full = room();
        full.roster = Roster::sign(&founder, members.clone()).unwrap();
        full.ushers = members.clone();
        let bytes = full.to_bytes().unwrap();
        assert!(
            bytes.len() > usize::from(u16::MAX),
            "a full roster outgrows a two-byte length"
        );
        assert_eq!(
            Room::from_bytes(&bytes).unwrap().roster.members().len(),
            MOST_MEMBERS
        );
        let offer = RoomOffer {
            founder: founder.verifying_key(),
            ward: Ward::from_bits(1),
            roster: full.roster,
        };
        let offered = offer.to_bytes().unwrap();
        assert!(
            offered.len() < kusanagi_seal::DROP,
            "an offer of thirty-two fits one drop"
        );
        assert_eq!(
            RoomOffer::from_bytes(&offered)
                .unwrap()
                .roster
                .members()
                .len(),
            MOST_MEMBERS
        );
    }

    #[test]
    fn trailing_bytes_and_another_version_are_refused() {
        let mut bytes = room().to_bytes().unwrap();
        bytes.push(0);
        assert!(Room::from_bytes(&bytes).is_err());
        assert!(Room::from_bytes(&[VERSION + 1]).is_err());
    }

    #[test]
    fn a_room_offer_round_trips_and_refuses_another_version() {
        use super::{ROOM_OFFER_VERSION, RoomOffer};
        let founder = Signer::from_seed(&[7; 32]);
        let offer = RoomOffer {
            founder: founder.verifying_key(),
            ward: Ward::from_bits(0x00ab),
            roster: Roster::sign(&founder, vec![founder.verifying_key()]).unwrap(),
        };
        let bytes = offer.to_bytes().unwrap();
        let decoded = RoomOffer::from_bytes(&bytes).unwrap();
        assert_eq!(decoded.to_bytes().unwrap(), bytes);
        let mut other = bytes.clone();
        if let Some(version) = other.first_mut() {
            *version = ROOM_OFFER_VERSION + 1;
        }
        assert!(RoomOffer::from_bytes(&other).is_err());
    }
}
