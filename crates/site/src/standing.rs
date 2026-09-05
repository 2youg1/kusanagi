// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Why somebody is allowed to be on a channel at all.

use kusanagi_grant::{Ability, Grant, GrantError, Revocations};
use kusanagi_kernel::{Handle, Instant, Reader};

use crate::blocks::{malformed, put_block, take_block};

use crate::error::SiteError;

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

    pub(crate) fn write(&self, out: &mut Vec<u8>) {
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

    pub(crate) fn read(reader: &mut Reader<'_>) -> Result<Self, SiteError> {
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
