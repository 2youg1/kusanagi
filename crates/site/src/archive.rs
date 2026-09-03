// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A whole site in one sealed byte string, and back again.
//!
//! **Backup stops being optional the moment a site becomes the only copy.**
//! Release deletes the drops a peer has acknowledged, a ratchet burns the key
//! that would reopen them, and encryption at rest ties the whole directory to one
//! Windows account — each of those turns "the host still has it" from a slow path
//! into a false statement. This is what an agent runs instead of owning a GUI.
//!
//! The archive is **plaintext in form and sealed in transit**: the records go in
//! as this build writes them on disk, before any platform storage touches them,
//! so an archive made on Windows opens on Linux. That is also the migration path
//! between platforms, which is why nothing here knows what a platform is.
//!
//! ```text
//! "KSNB" | version u8 | nonce[12] | sealed(kind u8 | len u32 | bytes)*
//! ```
//!
//! The nonce travels with the ciphertext because an archive has no address to
//! derive one from — it is the one thing this workspace seals that is not a drop.
//! The key is `blake3::derive_key("kusanagi/backup/1", recovery)`, and the
//! recovery key is 32 bytes the caller drew and showed to a person once.

use kusanagi_chain::Cairn;
use kusanagi_grant::StepId;
use kusanagi_kernel::{Handle, Reader};
use kusanagi_seal::{Fit, backup_key, open, seal};
use zeroize::Zeroize as _;

use crate::channel::Channel;
use crate::error::SiteError;
use crate::roster::Roster;
use crate::site::Site;

/// What every archive begins with, so that a wrong file is refused as one.
const MAGIC: &[u8; 4] = b"KSNB";

/// The archive layout this build writes and reads.
const VERSION: u8 = 1;

/// What one entry in an archive is.
///
/// A byte rather than a name, and an exhaustive match on the way back in, so an
/// archive from a build that learned a sixth kind is refused rather than
/// half-restored.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Kind {
    /// The identity seed: 32 bytes, and the whole of who this endpoint is.
    Identity = 1,
    /// One channel record, in the form `channel.rs` writes.
    Channel = 2,
    /// One cairn, behind the channel name it belongs to.
    Cairn = 3,
    /// One revoked step identifier.
    Revoked = 4,
    /// One group's roster, in the form `roster.rs` writes.
    Group = 5,
}

impl Kind {
    /// The byte this kind is written as.
    const fn byte(self) -> u8 {
        match self {
            Self::Identity => 1,
            Self::Channel => 2,
            Self::Cairn => 3,
            Self::Revoked => 4,
            Self::Group => 5,
        }
    }

    const fn of(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::Identity),
            2 => Some(Self::Channel),
            3 => Some(Self::Cairn),
            4 => Some(Self::Revoked),
            5 => Some(Self::Group),
            _ => None,
        }
    }
}

/// Writes one length-prefixed entry.
fn put(out: &mut Vec<u8>, kind: Kind, bytes: &[u8]) -> Result<(), SiteError> {
    let len = u32::try_from(bytes.len()).map_err(|_| SiteError::BadRecord {
        what: "an archive entry",
        reason: "a record is larger than four gigabytes".to_owned(),
    })?;
    out.push(kind.byte());
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(bytes);
    Ok(())
}

/// What a malformed archive says.
fn malformed(reason: impl Into<String>) -> SiteError {
    SiteError::BadRecord {
        what: "an archive",
        reason: reason.into(),
    }
}

/// Everything `site` holds, sealed under `recovery`.
///
/// `nonce` must be drawn fresh for every archive: this key is used more than
/// once over a site's life, so the nonce is what keeps the pair unique.
///
/// # Errors
///
/// [`SiteError::Local`] when the disk refuses, and [`SiteError::BadRecord`] when
/// a record on it is not one this build wrote.
pub fn export(site: &Site, recovery: &[u8; 32], nonce: [u8; 12]) -> Result<Vec<u8>, SiteError> {
    let mut plain = Vec::new();
    let mut seed = site.seed()?.ok_or(SiteError::NoIdentity)?;
    put(&mut plain, Kind::Identity, &seed)?;
    seed.zeroize();

    for name in site.names()? {
        let channel = site.channel(&name)?;
        put(&mut plain, Kind::Channel, &channel.to_bytes())?;
        // Both lanes: the peer's stream and this endpoint's own. A cairn names
        // its author, so the channel name is all that has to go with it.
        for author in cairn_authors(site, &channel)? {
            if let Some(cairn) = site.cairn(&name, &author)? {
                let mut entry = Vec::new();
                let len = u16::try_from(name.len())
                    .map_err(|_| malformed("a channel name is longer than a name can be"))?;
                entry.extend_from_slice(&len.to_be_bytes());
                entry.extend_from_slice(name.as_bytes());
                entry.extend_from_slice(&cairn.to_bytes());
                put(&mut plain, Kind::Cairn, &entry)?;
            }
        }
    }

    for step in site.revocations()?.iter() {
        put(&mut plain, Kind::Revoked, step.as_bytes())?;
    }

    // A roster is not recomputable from anything: it is a decision its owner
    // made, and an archive that dropped it would restore a site that had
    // forgotten who it talks to at once.
    for roster in site.groups()? {
        put(&mut plain, Kind::Group, &roster.to_bytes())?;
    }

    let key = backup_key(recovery, nonce);
    let sealed = seal(&key, Fit::Exact, &plain)?;
    plain.zeroize();

    let mut archive = MAGIC.to_vec();
    archive.push(VERSION);
    archive.extend_from_slice(&nonce);
    archive.extend_from_slice(&sealed);
    Ok(archive)
}

/// Whose streams this endpoint keeps a cairn for on `channel`.
fn cairn_authors(site: &Site, channel: &Channel) -> Result<Vec<Handle>, SiteError> {
    let mut authors = Vec::new();
    if let Some(signer) = site.identity()? {
        authors.push(signer.handle());
    }
    if let Some(peer) = &channel.peer {
        authors.push(peer.handle());
    }
    Ok(authors)
}

/// Restores an archive into `site`, which must be empty.
///
/// # Errors
///
/// [`SiteError::BadRecovery`] when the key does not open it, and
/// [`SiteError::BadRecord`] when the bytes are not an archive this build reads.
/// [`SiteError::ChannelExists`] — as a record refusal — when `site` already has
/// an identity: merging two sites would give one endpoint two of everything, and
/// there is no rule for which of them is right.
pub fn import(site: &Site, recovery: &[u8; 32], archive: &[u8]) -> Result<(), SiteError> {
    if site.identity()?.is_some() {
        return Err(malformed(
            "this root already holds an identity; import needs an empty one",
        ));
    }
    let rest = archive
        .strip_prefix(MAGIC)
        .ok_or_else(|| malformed("this is not an archive"))?;
    let (version, rest) = rest
        .split_first()
        .ok_or_else(|| malformed("an archive ends before it begins"))?;
    if *version != VERSION {
        return Err(malformed(format!(
            "this archive is version {version}, and this build reads {VERSION}"
        )));
    }
    let (nonce, sealed) = rest
        .split_at_checked(12)
        .ok_or_else(|| malformed("an archive ends before its nonce"))?;
    let nonce = <[u8; 12]>::try_from(nonce).map_err(|_| malformed("a nonce is twelve bytes"))?;

    let key = backup_key(recovery, nonce);
    let mut plain = open(&key, Fit::Exact, sealed).map_err(|_| SiteError::BadRecovery)?;
    let restored = restore(site, &plain);
    plain.zeroize();
    restored
}

/// Walks the entries and puts each one back where it came from.
fn restore(site: &Site, plain: &[u8]) -> Result<(), SiteError> {
    let mut reader = Reader::new(plain);
    while reader.remaining() > 0 {
        let kind = reader
            .take_byte()
            .ok()
            .and_then(Kind::of)
            .ok_or_else(|| malformed("an entry names a kind this build does not know"))?;
        let len = reader
            .take_u32()
            .map_err(|_| malformed("an entry ends before its length"))?;
        let len = usize::try_from(len).map_err(|_| malformed("an entry is larger than memory"))?;
        let bytes = reader
            .take(len)
            .map_err(|_| malformed("an entry is shorter than it says"))?
            .to_vec();

        match kind {
            Kind::Identity => {
                let mut seed = <[u8; 32]>::try_from(bytes.as_slice())
                    .map_err(|_| malformed("a seed is 32 bytes"))?;
                let adopted = site.adopt(&seed);
                seed.zeroize();
                adopted?;
            }
            Kind::Channel => site.keep(&Channel::from_bytes(&bytes)?)?,
            Kind::Cairn => {
                let (name, cairn) = split_cairn(&bytes)?;
                site.mark(&name, &cairn)?;
            }
            Kind::Revoked => {
                let id = <[u8; 32]>::try_from(bytes.as_slice())
                    .map_err(|_| malformed("a step identifier is 32 bytes"))?;
                site.revoke(StepId::from_bytes(id))?;
            }
            Kind::Group => {
                let text = String::from_utf8_lossy(&bytes);
                let named = text.lines().next().unwrap_or_default().trim().to_owned();
                site.enrol(&Roster::from_bytes(&bytes, &named)?)?;
            }
        }
    }
    Ok(())
}

/// A cairn entry: the channel name it belongs to, then the cairn itself.
fn split_cairn(bytes: &[u8]) -> Result<(String, Cairn), SiteError> {
    let (len, rest) = bytes
        .split_at_checked(2)
        .ok_or_else(|| malformed("a cairn entry ends before its name"))?;
    let len = usize::from(u16::from_be_bytes(
        <[u8; 2]>::try_from(len).map_err(|_| malformed("a name length is two bytes"))?,
    ));
    let (name, cairn) = rest
        .split_at_checked(len)
        .ok_or_else(|| malformed("a cairn entry is shorter than its name says"))?;
    let name = String::from_utf8(name.to_vec())
        .map_err(|_| malformed("a channel name in an archive is not text"))?;
    let cairn = Cairn::from_bytes(cairn)
        .map_err(|error| malformed(format!("a cairn in an archive is not one: {error}")))?;
    Ok((name, cairn))
}
