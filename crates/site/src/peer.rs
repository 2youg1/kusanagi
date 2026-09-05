// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The other end of a conversation, and how its name is written down.
//!
//! Apart from `channel.rs` because the two change for different reasons: a
//! channel record gains a field when this endpoint learns something new about
//! the conversation, and a peer gains one when it learns something new about
//! the person. The alias codec sits here beside the one struct that carries it.

use kusanagi_kernel::{Alias, Handle, Reader, VerifyingKey, Ward};

use crate::blocks::malformed;
use crate::error::SiteError;
use crate::standing::Standing;

/// The other end of a conversation, once it has said who it is.
#[derive(Clone, Debug)]
pub struct Peer {
    /// The key that checks the peer's segments.
    pub key: VerifyingKey,
    /// Which bin of the host the peer reads, and so where to write to them.
    ///
    /// Not an `Option`. A peer this endpoint knows is a peer it can write to,
    /// and both ways of learning one — an offer for the newcomer, a greeting for
    /// the inviter — carry the ward beside the key. A peer without a ward would
    /// be a peer whose messages go somewhere nobody sweeps.
    pub ward: Ward,
    /// Why the peer is allowed here.
    pub standing: Standing,
    /// What the peer calls itself, signed by their key and checked when it
    /// arrived. Absent when they declared nothing.
    pub alias: Option<Alias>,
}

impl Peer {
    /// What the peer is called: the name their stream is derived through.
    #[must_use]
    pub fn handle(&self) -> Handle {
        self.key.handle()
    }
}

/// One length byte and thirty-two bytes of name, zero-padded.
///
/// Fixed width rather than a block, so that a record is one size whether the
/// peer is named, unnamed or absent: `tests/robust.rs` holds that a listing of
/// this directory gives up nothing about who has joined.
pub(crate) const ALIAS_BLOCK: usize = 1 + Alias::MOST;

/// Writes an optional alias into one [`ALIAS_BLOCK`]; a length of zero means none.
pub(crate) fn put_alias(out: &mut Vec<u8>, alias: Option<&Alias>) {
    let name = alias.map_or(&[][..], |alias| alias.as_str().as_bytes());
    // `Alias::new` bounds a name at 32 bytes, so the prefix always fits.
    out.push(u8::try_from(name.len()).unwrap_or(u8::MAX));
    out.extend_from_slice(name);
    out.resize(
        out.len()
            .saturating_add(Alias::MOST.saturating_sub(name.len())),
        0,
    );
}

/// Reads what [`put_alias`] wrote, holding the name to the rule an alias has.
pub(crate) fn take_alias(reader: &mut Reader<'_>) -> Result<Option<Alias>, SiteError> {
    let len = usize::from(reader.take_byte().map_err(malformed)?);
    let block = reader.take_array::<{ Alias::MOST }>().map_err(malformed)?;
    let unfit = |reason: String| SiteError::BadRecord {
        what: "a peer's alias",
        reason,
    };
    let (name, pad) = block
        .split_at_checked(len)
        .ok_or_else(|| unfit("a name is at most 32 bytes".to_owned()))?;
    if pad.iter().any(|byte| *byte != 0) {
        return Err(unfit("the padding after a name is not zero".to_owned()));
    }
    if len == 0 {
        return Ok(None);
    }
    let text = core::str::from_utf8(name).map_err(|error| unfit(error.to_string()))?;
    Alias::new(text)
        .map(Some)
        .map_err(|error| unfit(error.to_string()))
}
