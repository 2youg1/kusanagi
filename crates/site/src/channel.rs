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
//! has_peer        1 byte
//! peer         2592 bytes   the peer's verifying key; zeroes when absent
//! peer_standing   1 byte    as above
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

use kusanagi_grant::{Ability, Grant, GrantError, Revocations};
use kusanagi_kernel::{Handle, Instant, Reader, VerifyingKey};
use kusanagi_seal::Secret;

use crate::error::SiteError;

/// The record this build writes and reads.
///
/// Version 3 carries the channel's local name, which version 2 kept in the file
/// name instead. Version 2 named its peer by verifying key where version 1 named
/// it by handle — the two are the same width and neither decodes as the other,
/// which is why the version byte moves at all: a silent reinterpretation would
/// leave an endpoint verifying every segment against 32 bytes that are not a key.
const VERSION: u8 = 3;
const STANDING_ROOT: u8 = 0;
const STANDING_GRANTED: u8 = 1;

/// Why somebody is allowed to be on a channel at all.
///
/// An enum rather than an `Option<Grant>` because the two cases are different
/// facts, not a present and an absent one: the root authority holds no grant
/// because there is nobody above it to have issued one, and saying that in the
/// type stops every caller from having to remember what `None` meant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing {
    /// This handle *is* the authority every grant on the channel descends from.
    Root,
    /// This handle holds a grant that descends from that authority.
    Granted(Grant),
}

impl Standing {
    /// Whether `who` may do `ability` here, at `now`.
    ///
    /// # Errors
    ///
    /// [`GrantError`] naming exactly which link of the chain refused, or
    /// [`GrantError::NotTheHolder`] when a handle claims to be an authority it
    /// is not.
    pub fn permits(
        &self,
        root: &Handle,
        who: &Handle,
        ability: Ability,
        now: Instant,
        revoked: &Revocations,
    ) -> Result<(), GrantError> {
        match self {
            Self::Root => {
                if who == root {
                    Ok(())
                } else {
                    Err(GrantError::NotTheHolder {
                        holder: *root,
                        presenter: *who,
                    })
                }
            }
            Self::Granted(grant) => grant.permits(root, who, ability, now, revoked),
        }
    }

    /// The grant, when there is one.
    #[must_use]
    pub const fn grant(&self) -> Option<&Grant> {
        match self {
            Self::Root => None,
            Self::Granted(grant) => Some(grant),
        }
    }

    fn write(&self, out: &mut Vec<u8>) {
        match self {
            Self::Root => {
                out.push(STANDING_ROOT);
                put_block(out, &[]);
            }
            Self::Granted(grant) => {
                out.push(STANDING_GRANTED);
                put_block(out, &grant.to_canonical_bytes());
            }
        }
    }

    fn read(reader: &mut Reader<'_>) -> Result<Self, SiteError> {
        let tag = reader.take_byte().map_err(malformed)?;
        let block = take_block(reader)?;
        match tag {
            STANDING_ROOT => Ok(Self::Root),
            STANDING_GRANTED => Ok(Self::Granted(Grant::from_canonical_bytes(&block)?)),
            other => Err(SiteError::BadRecord {
                what: "a standing",
                reason: format!("a standing is root or granted, not {other}"),
            }),
        }
    }
}

/// The other end of a conversation, once it has said who it is.
#[derive(Clone, Debug)]
pub struct Peer {
    /// The key that checks the peer's segments.
    pub key: VerifyingKey,
    /// Why the peer is allowed here.
    pub standing: Standing,
}

impl Peer {
    /// What the peer is called: the name their stream is derived through.
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.key.handle()
    }
}

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
        match &self.peer {
            None => {
                out.push(0);
                out.extend_from_slice(&[0_u8; VerifyingKey::WIDTH]);
                Standing::Root.write(&mut out);
            }
            Some(peer) => {
                out.push(1);
                out.extend_from_slice(peer.key.as_bytes());
                peer.standing.write(&mut out);
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

        let has_peer = reader.take_byte().map_err(malformed)?;
        let key = VerifyingKey::from_bytes(
            reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map_err(malformed)?,
        );
        let peer_standing = Standing::read(&mut reader)?;
        let peer = match has_peer {
            0 => None,
            1 => Some(Peer {
                key,
                standing: peer_standing,
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
            peer,
        })
    }
}

/// Writes a length-prefixed block.
///
/// The length is `u16`, which caps a locator and a grant at 64 KiB each. A grant
/// is bounded by its hop limit and a locator is a URL, so the cap is unreachable;
/// saturating rather than wrapping is what keeps an unreachable case from
/// becoming a silently truncated one.
pub(crate) fn put_block(out: &mut Vec<u8>, block: &[u8]) {
    let len = u16::try_from(block.len()).unwrap_or(u16::MAX);
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(block);
}

pub(crate) fn take_block(reader: &mut Reader<'_>) -> Result<Vec<u8>, SiteError> {
    let len = usize::from(u16::from_be_bytes(
        reader.take_array::<2>().map_err(malformed)?,
    ));
    Ok(reader.take(len).map_err(malformed)?.to_vec())
}

pub(crate) fn take_text(reader: &mut Reader<'_>, what: &'static str) -> Result<String, SiteError> {
    String::from_utf8(take_block(reader)?).map_err(|error| SiteError::BadRecord {
        what,
        reason: error.to_string(),
    })
}

pub(crate) fn malformed(error: kusanagi_kernel::Incomplete) -> SiteError {
    SiteError::BadRecord {
        what: "a stored record",
        reason: error.to_string(),
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
    use super::{Channel, Peer, Standing};
    use kusanagi_grant::{Abilities, Ability, Grant, GrantError, Revocations, Scope};
    use kusanagi_kernel::{Instant, Signer};
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
            peer: with_peer.then(|| Peer {
                key: guest.verifying_key(),
                standing: Standing::Granted(Grant::issue(&root, &guest.handle(), scope)),
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
            decoded.peer.map(|peer| peer.handle()),
            original.peer.map(|peer| peer.handle())
        );
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
