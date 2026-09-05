// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Putting an archive back: one entry, one home.
//!
//! Apart from `archive.rs` because that file is at its line limit: export
//! writes entries and import verifies the seal, and restoring them is a third
//! reason to change. The match is exhaustive on purpose — an archive from a
//! build that learned a tenth kind is refused rather than half-restored.

use kusanagi_chain::Cairn;
use kusanagi_grant::StepId;
use kusanagi_kernel::{Alias, Handle, Reader, Ward};
use kusanagi_seal::Ratchet;
use zeroize::Zeroize as _;

use crate::channel::Channel;
use crate::error::SiteError;
use crate::roster::Roster;
use crate::site::Site;

use super::archive::{Kind, malformed, split_named};

/// Walks the entries and puts each one back where it came from.
pub(crate) fn restore(site: &Site, plain: &[u8]) -> Result<(), SiteError> {
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
                let (Some(head), Some(tail)) = (bytes.get(..32), bytes.get(32..34)) else {
                    return Err(malformed("an identity is a 32-byte seed and a ward"));
                };
                let (Ok(mut seed), Ok(ward)) =
                    (<[u8; 32]>::try_from(head), <[u8; 2]>::try_from(tail))
                else {
                    return Err(malformed("an identity is a 32-byte seed and a ward"));
                };
                let adopted = site.adopt(&seed, Ward::from_bits(u16::from_be_bytes(ward)));
                seed.zeroize();
                adopted?;
            }
            Kind::Channel => site.keep(&Channel::from_bytes(&bytes)?)?,
            Kind::Cairn => {
                let (name, rest) = split_named(&bytes)?;
                let cairn = Cairn::from_bytes(rest).map_err(|error| {
                    malformed(format!("a cairn in an archive is not one: {error}"))
                })?;
                site.mark(&name, &cairn)?;
            }
            Kind::Ratchet => {
                let (name, rest) = split_named(&bytes)?;
                let (author, state) = rest
                    .split_at_checked(32)
                    .ok_or_else(|| malformed("a ratchet entry ends before its lane"))?;
                let author = Handle::from_bytes(
                    <[u8; 32]>::try_from(author).map_err(|_| malformed("a handle is 32 bytes"))?,
                );
                let ratchet = Ratchet::from_bytes(state)
                    .ok_or_else(|| malformed("a ratchet in an archive is not one"))?;
                site.burn(&name, &author, &ratchet)?;
            }
            Kind::Outbox => {
                let (name, payload) = split_named(&bytes)?;
                site.queue(&name, payload)?;
            }
            Kind::Revoked => {
                let id = <[u8; 32]>::try_from(bytes.as_slice())
                    .map_err(|_| malformed("a step identifier is 32 bytes"))?;
                site.revoke(StepId::from_bytes(id))?;
            }
            Kind::Alias => {
                let text = String::from_utf8(bytes)
                    .map_err(|_| malformed("an alias in an archive is not text"))?;
                let alias = Alias::new(&text)
                    .map_err(|error| malformed(format!("an alias in an archive: {error}")))?;
                site.set_alias(Some(&alias))?;
            }
            Kind::Group => {
                let text = String::from_utf8_lossy(&bytes);
                let named = text.lines().next().unwrap_or_default().trim().to_owned();
                site.enrol(&Roster::from_bytes(&bytes, &named)?)?;
            }
            Kind::Room => site.keep_room(&crate::room::Room::from_bytes(&bytes)?)?,
        }
    }
    Ok(())
}
