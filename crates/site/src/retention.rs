// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Whether a channel's history survives on the host, and what it costs when it
//! does not.
//!
//! By default every drop this endpoint writes stays where it was left. That is
//! what makes law 1 true in the shape it is written: a killed command loses
//! nothing, a stream's height is discovered from the host rather than from a
//! local file, and a site that is deleted costs the channel secret and not the
//! conversation.
//!
//! **Releasing trades exactly those things away.** Once the peer says they have
//! verified a segment, the drop is deleted and the key that would open it is
//! burned, so the only remaining copy of what was said is the reader's own site.
//! That is a real gain — an honest host keeps no history to be subpoenaed, and a
//! dishonest one that kept a copy anyway holds bytes nobody can open — and it is
//! a real loss: without the backup that `export` makes, a lost site is a lost
//! conversation, and reading below the released floor is an error rather than a
//! slow path.
//!
//! **So it is a choice per channel, and the default is to keep.** An enum rather
//! than a flag, because the combination that must not exist — release without a
//! backup — is then a visible fact in `kusanagi channels` instead of an accident
//! somebody discovers later.

use kusanagi_kernel::Reader;

use crate::blocks::malformed;
use crate::error::SiteError;

const KEEP: u8 = 0;
const RELEASE_ON_ACK: u8 = 1;

/// What becomes of a drop once the peer has read it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Retention {
    /// It stays on the host. The default, and what law 1 is written against.
    Keep,
    /// It is deleted, and its key is burned, as soon as the peer acknowledges it.
    ///
    /// The acknowledgement rides inside the peer's own sealed segments, so the
    /// host never learns how far either side has read.
    ReleaseOnAck,
}

impl Retention {
    /// Whether this channel's site is the only copy of what was said.
    #[must_use]
    pub const fn releases(self) -> bool {
        matches!(self, Self::ReleaseOnAck)
    }

    /// The word this appears as in a listing.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::ReleaseOnAck => "release",
        }
    }

    pub(crate) fn write(self, out: &mut Vec<u8>) {
        out.push(match self {
            Self::Keep => KEEP,
            Self::ReleaseOnAck => RELEASE_ON_ACK,
        });
    }

    pub(crate) fn read(reader: &mut Reader<'_>) -> Result<Self, SiteError> {
        match reader.take_byte().map_err(malformed)? {
            KEEP => Ok(Self::Keep),
            RELEASE_ON_ACK => Ok(Self::ReleaseOnAck),
            other => Err(SiteError::BadRecord {
                what: "a retention",
                reason: format!("a channel keeps or releases, not {other}"),
            }),
        }
    }
}
