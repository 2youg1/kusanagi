// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One conversation, as this endpoint knows it.
//!
//! A channel holds four things that cannot be derived from each other: the shared
//! secret that produces every address, the standing that says what this endpoint
//! may do, the locator of the host that holds the bytes, and — once it is known —
//! the peer.
//!
//! The peer is an `Option` because of the order the world happens in. Whoever
//! writes an invitation cannot know who will accept it; the acceptor announces
//! itself on the *introduction stream*, whose address comes from the one-time key
//! carried in the invitation. So a channel begins half-known and completes itself
//! the first time its owner reads.
//!
//! **Two of these fields are keys rather than names, and it is the same reason
//! in both cases: this endpoint has to check a signature with them.** A segment
//! names its author and carries no key, so reading a stream means holding the
//! author's key beforehand — the peer's, for everything they will ever write,
//! and the one-time bearer's, for the greeting that says who the peer is. The
//! root is a name, because a grant carries the keys that check it.
//!
//! ```text
//! version         1 byte
//! name_len        2 bytes   big endian, then that many utf-8 bytes
//! secret         32 bytes
//! root           32 bytes   the handle every grant here descends from
//! introduction 2592 bytes   the one-time key whose stream carries the greeting
//! locator_len     2 bytes   big endian, then that many utf-8 bytes
//! standing        1 byte    0 = root, 1 = granted, then a length-prefixed grant
//! cadence       1|5 bytes   0 = on demand; 1 = slotted, then a u32 of seconds
//! retention       1 byte    0 = keep, 1 = release once acknowledged
//! opened          8 bytes   big endian; the period this record was made in
//! has_peer        1 byte
//! peer         2592 bytes   the peer's verifying key; zeroes when absent
//! peer_standing   1 byte    as above
//! peer_alias     33 bytes   a length, then the name the peer signed for itself, zero-padded
//! ```
//!
//! **The name is in the record because it is no longer in the file name.** A
//! directory listing used to say who this endpoint talks to, in plain text, to
//! any account that could read the directory; see `site.rs` for what the file is
//! called now. Here it means one thing: a record knows what it is called, and
//! `Site` checks that answer against the name it looked the file up under.
//!
//! The two key fields are `VerifyingKey::WIDTH` wide, so a change of signature
//! scheme is a change of record version.

use kusanagi_kernel::{Handle, Period, Reader, VerifyingKey, Ward};
use kusanagi_seal::Secret;

use crate::blocks::{malformed, put_block, take_text};
use crate::cadence::Cadence;
use crate::error::SiteError;
use crate::peer::{ALIAS_BLOCK, Peer, put_alias, take_alias};
use crate::retention::Retention;
use crate::standing::Standing;

/// The record this build writes and reads.
///
/// Version 7 carries the alias the peer declared at introduction, already
/// verified against the key beside it (`kernel::Declaration`), so a listing and
/// a read can name the peer without a second verification. Version 6 carries the period the channel was opened in: the earliest bin a
/// sweep for it can have anything in, and so where a reader with no record of
/// what it swept starts. Version 5 carried the peer's ward, which is where this
/// endpoint files what it writes to them. Version 4 carried the two choices that change what this endpoint does on the
/// network rather than what it knows: a [`Cadence`] and a [`Retention`]. Version
/// 3 carried the channel's local name, which version 2 kept in the file name
/// instead. Version 2 named its peer by verifying key where version 1 named it
/// by handle — the two are the same width and neither decodes as the other,
/// which is why the version byte moves at all: a silent reinterpretation would
/// leave an endpoint verifying every segment against 32 bytes that are not a key.
const VERSION: u8 = 7;

/// One conversation, as this endpoint knows it.
#[derive(Clone, Debug)]
pub struct Channel {
    /// What this endpoint calls the channel. Local, and never sent anywhere.
    pub name: String,
    /// Every address on this channel derives from here.
    pub secret: Secret,
    /// The authority every grant on this channel descends from.
    pub root: Handle,
    /// The one-time key that signs the introduction, and whose handle the
    /// introduction stream is derived through.
    pub introduction: VerifyingKey,
    /// Where the bytes live.
    pub locator: String,
    /// Why this endpoint is allowed here.
    pub standing: Standing,
    /// How often this endpoint writes here.
    pub cadence: Cadence,
    /// What becomes of a drop once the peer has read it.
    pub retention: Retention,
    /// The period this record was made in, before which no drop of this
    /// channel can be filed. A sweep that has no record of how far it got
    /// starts here, so losing every record costs bins and never messages.
    pub opened: Period,
    /// The other end, once it has introduced itself.
    pub peer: Option<Peer>,
}

impl Channel {
    /// The wire form, which is also the on-disk form.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![VERSION];
        put_block(&mut out, self.name.as_bytes());
        out.extend_from_slice(self.secret.as_bytes());
        out.extend_from_slice(self.root.as_bytes());
        out.extend_from_slice(self.introduction.as_bytes());
        put_block(&mut out, self.locator.as_bytes());
        self.standing.write(&mut out);
        self.cadence.write(&mut out);
        self.retention.write(&mut out);
        out.extend_from_slice(&self.opened.count().to_be_bytes());
        match &self.peer {
            None => {
                out.push(0);
                out.extend_from_slice(&[0_u8; VerifyingKey::WIDTH]);
                out.extend_from_slice(&[0_u8; 2]);
                Standing::Root.write(&mut out);
                put_alias(&mut out, None);
            }
            Some(peer) => {
                out.push(1);
                out.extend_from_slice(peer.key.as_bytes());
                out.extend_from_slice(&peer.ward.bits().to_be_bytes());
                peer.standing.write(&mut out);
                put_alias(&mut out, peer.alias.as_ref());
            }
        }
        out
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
                what: "a channel",
                reason: format!("this file is version {version}, and this build reads {VERSION}"),
            });
        }

        let name = take_text(&mut reader, "a channel name")?;
        let secret = Secret::from_bytes(reader.take_array::<32>().map_err(malformed)?);
        let root = Handle::from_bytes(reader.take_array::<32>().map_err(malformed)?);
        let introduction = VerifyingKey::from_bytes(
            reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map_err(malformed)?,
        );
        let locator = take_text(&mut reader, "a locator")?;
        let standing = Standing::read(&mut reader)?;
        let cadence = Cadence::read(&mut reader)?;
        let retention = Retention::read(&mut reader)?;
        let opened = Period::from_count(u64::from_be_bytes(
            reader.take_array::<8>().map_err(malformed)?,
        ));

        let has_peer = reader.take_byte().map_err(malformed)?;
        let key = VerifyingKey::from_bytes(
            reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map_err(malformed)?,
        );
        let ward = Ward::from_bits(u16::from_be_bytes(
            reader.take_array::<2>().map_err(malformed)?,
        ));
        let peer_standing = Standing::read(&mut reader)?;
        let peer = match has_peer {
            0 => {
                reader.take(ALIAS_BLOCK).map_err(malformed)?;
                None
            }
            1 => Some(Peer {
                key,
                ward,
                standing: peer_standing,
                alias: take_alias(&mut reader)?,
            }),
            other => {
                return Err(SiteError::BadRecord {
                    what: "a channel",
                    reason: format!("a peer is present or absent, not {other}"),
                });
            }
        };

        if reader.remaining() != 0 {
            return Err(SiteError::BadRecord {
                what: "a channel",
                reason: format!("{} byte(s) follow a complete record", reader.remaining()),
            });
        }
        Ok(Self {
            name,
            secret,
            root,
            introduction,
            locator,
            standing,
            cadence,
            retention,
            opened,
            peer,
        })
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
    use super::{Cadence, Channel, Peer, Retention};
    use crate::standing::Standing;
    use kusanagi_grant::{Abilities, Ability, Grant, GrantError, Revocations, Scope};
    use kusanagi_kernel::{Alias, Instant, Period, Signer, Ward};
    use kusanagi_seal::Secret;

    fn channel(with_peer: bool) -> Channel {
        let root = Signer::from_seed(&[1; 32]);
        let guest = Signer::from_seed(&[2; 32]);
        let scope = Scope::new(Abilities::ALL, Instant::from_unix_seconds(9_999));
        Channel {
            name: "peer-one".to_owned(),
            secret: Secret::from_bytes([7; 32]),
            root: root.handle(),
            introduction: guest.verifying_key(),
            locator: "http://box.example:8963".to_owned(),
            standing: Standing::Root,
            cadence: Cadence::OnDemand,
            retention: Retention::Keep,
            opened: Period::from_count(2_945_376),
            peer: with_peer.then(|| Peer {
                ward: Ward::from_bits(0x00ab),
                key: guest.verifying_key(),
                standing: Standing::Granted(Grant::issue(&root, &guest.handle(), scope)),
                alias: Some(Alias::new("Bob").unwrap()),
            }),
        }
    }

    #[test]
    fn a_channel_without_a_peer_round_trips() {
        let original = channel(false);
        let decoded = Channel::from_bytes(&original.to_bytes()).unwrap();
        assert_eq!(decoded.to_bytes(), original.to_bytes());
        assert!(decoded.peer.is_none());
        assert_eq!(decoded.name, original.name);
        assert_eq!(decoded.locator, original.locator);
        assert_eq!(decoded.standing, Standing::Root);
    }

    #[test]
    fn a_channel_with_a_peer_round_trips() {
        let original = channel(true);
        let decoded = Channel::from_bytes(&original.to_bytes()).unwrap();
        assert_eq!(decoded.to_bytes(), original.to_bytes());
        assert_eq!(
            decoded.peer.as_ref().map(Peer::handle),
            original.peer.as_ref().map(Peer::handle)
        );
        assert_eq!(
            decoded.peer.and_then(|peer| peer.alias),
            original.peer.and_then(|peer| peer.alias)
        );
    }

    /// A record that carries no name reads back as a peer with none, and one
    /// that carries an unfit name is refused rather than shown.
    #[test]
    fn a_peer_without_an_alias_round_trips_and_an_unfit_one_is_refused() {
        let mut original = channel(true);
        if let Some(peer) = original.peer.as_mut() {
            peer.alias = None;
        }
        let decoded = Channel::from_bytes(&original.to_bytes()).unwrap();
        assert!(decoded.peer.unwrap().alias.is_none());
        let mut bytes = original.to_bytes();
        bytes.pop();
        bytes.extend_from_slice(&[4, b'B', b'o', b'b', b'\n']);
        assert!(Channel::from_bytes(&bytes).is_err());
    }

    #[test]
    fn the_root_authorises_itself_and_nobody_else() {
        let root = Signer::from_seed(&[1; 32]).handle();
        let other = Signer::from_seed(&[9; 32]).handle();
        let now = Instant::EPOCH;
        assert_eq!(
            Standing::Root.permits(&root, &root, Ability::Send, now, &Revocations::new()),
            Ok(())
        );
        assert!(matches!(
            Standing::Root.permits(&root, &other, Ability::Send, now, &Revocations::new()),
            Err(GrantError::NotTheHolder { .. })
        ));
    }

    /// The two policies survive the round trip, and neither one's default is
    /// what the other decodes to.
    #[test]
    fn a_channel_remembers_how_it_writes_and_what_it_keeps() {
        let mut original = channel(true);
        original.cadence = Cadence::Slotted {
            period: core::num::NonZeroU32::new(900).unwrap(),
        };
        original.retention = Retention::ReleaseOnAck;
        let decoded = Channel::from_bytes(&original.to_bytes()).unwrap();
        assert_eq!(decoded.cadence.period(), Some(900));
        assert_eq!(decoded.retention, Retention::ReleaseOnAck);
        assert_eq!(decoded.to_bytes(), original.to_bytes());
    }

    #[test]
    fn trailing_bytes_are_refused() {
        let mut bytes = channel(false).to_bytes();
        bytes.push(0);
        assert!(Channel::from_bytes(&bytes).is_err());
    }

    #[test]
    fn another_version_is_refused_rather_than_guessed() {
        let bytes = vec![9_u8];
        assert!(Channel::from_bytes(&bytes).is_err());
    }
}
