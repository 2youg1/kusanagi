// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What an invitation points at: who is inviting, and by what authority.
//!
//! Apart from `invite` because parsing a line somebody pasted and decoding a
//! drop the host held fail for different reasons and change for different
//! reasons: a new invitation version is a prefix here, and a new offer layout
//! is a version byte there.

use kusanagi_grant::Grant;
use kusanagi_kernel::{Declaration, Reader, VerifyingKey, Ward};

use crate::blocks::{put_block, take_block};

use crate::error::SiteError;
use crate::invite::mangled;
use crate::retention::Retention;

/// What an invitation points at: who is inviting, and by what authority.
///
/// Sealed into one drop at the address [`kusanagi_seal::offer`] derives from the
/// channel secret. Public data, kept off the line because it is large, and kept
/// out of the clear because an address nobody can compute costs less than an
/// argument about whether it mattered.
///
/// ```text
/// version    1 byte     = 4
/// inviter 2592 bytes    the inviter's verifying key
/// ward       2 bytes    the bin of the host the inviter reads
/// retention  1 byte     0 = keep, 1 = release once acknowledged
/// name     2+n bytes    a length, then the inviter's signed declaration; 0 = none
/// grant      the rest   inviter -> bearer
/// ```
///
/// The name travels here because this is the one drop the newcomer reads about
/// the inviter before either has a lane: a `kernel::Declaration`, checked by
/// `join` against the key beside it, so it costs no segment and no second
/// exchange. Renaming later reaches only channels opened afterwards.
///
/// Retention travels here because it is a property of the channel, not of
/// one endpoint: a releasing lane ratchets its keys, so two ends that disagree
/// about it derive two key schedules and every read fails `seal.rejected`.
///
/// The ward travels here because a writer has to know where its reader looks.
/// It is the inviter's, and it is public to whoever holds this invitation: the
/// bin hides *which object of a crowd was wanted*, never that the crowd exists.
#[derive(Clone, Debug)]
pub struct Offer {
    /// Who issued the invitation, and the root of every grant on the channel.
    pub inviter: VerifyingKey,
    /// Which bin of the host the inviter reads, so a newcomer can write to them.
    pub ward: Ward,
    /// What becomes of a drop once the peer acknowledges it, on both ends.
    pub retention: Retention,
    /// What the inviter calls itself, signed by the key above; absent when
    /// they declared nothing.
    pub declaration: Option<Declaration>,
    /// The grant from the inviter to the one-time key.
    pub grant: Grant,
}

/// The layout of an offer, which versions apart from the invitation's own.
const OFFER_VERSION: u8 = 4;

impl Offer {
    /// The bytes that go in the drop.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![OFFER_VERSION];
        out.extend_from_slice(self.inviter.as_bytes());
        out.extend_from_slice(&self.ward.bits().to_be_bytes());
        self.retention.write(&mut out);
        put_block(
            &mut out,
            &self
                .declaration
                .as_ref()
                .map_or_else(Vec::new, Declaration::to_bytes),
        );
        out.extend_from_slice(&self.grant.to_canonical_bytes());
        out
    }

    /// Reads what [`Self::to_bytes`] wrote.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadInvitation`] when the bytes are not an offer this build
    /// reads. They opened under a key derived from the channel secret, so
    /// whoever wrote them held it: this is a build disagreement or damage,
    /// never a forgery.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SiteError> {
        let mut reader = Reader::new(bytes);
        let version = reader.take_byte().map_err(mangled)?;
        if version != OFFER_VERSION {
            return Err(SiteError::BadInvitation {
                reason: format!(
                    "this invitation points at a version {version} offer; this build reads {OFFER_VERSION}"
                ),
            });
        }
        let inviter = VerifyingKey::from_bytes(
            reader
                .take_array::<{ VerifyingKey::WIDTH }>()
                .map_err(mangled)?,
        );
        let ward = Ward::from_bits(u16::from_be_bytes(
            reader.take_array::<2>().map_err(mangled)?,
        ));
        let retention = Retention::read(&mut reader)?;
        let declared = take_block(&mut reader)?;
        let declaration = if declared.is_empty() {
            None
        } else {
            Some(
                Declaration::from_bytes(&declared).map_err(|error| SiteError::BadInvitation {
                    reason: format!("the inviter's name does not read: {error}"),
                })?,
            )
        };
        let rest = reader.take(reader.remaining()).map_err(mangled)?;
        Ok(Self {
            inviter,
            ward,
            retention,
            declaration,
            grant: Grant::from_canonical_bytes(rest)?,
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
    use super::{OFFER_VERSION, Offer};
    use crate::retention::Retention;
    use kusanagi_grant::{Abilities, Grant, Scope};
    use kusanagi_kernel::{Alias, Declaration, Instant, Signer, Ward};

    fn offer() -> Offer {
        let inviter = Signer::from_seed(&[1; 32]);
        let bearer = Signer::from_seed(&[2; 32]);
        Offer {
            inviter: inviter.verifying_key(),
            ward: Ward::from_bits(0x3c5a),
            retention: Retention::ReleaseOnAck,
            declaration: Some(Declaration::sign(&inviter, Alias::new("Alice").unwrap())),
            grant: Grant::issue(
                &inviter,
                &bearer.handle(),
                Scope::new(Abilities::ALL, Instant::from_unix_seconds(9_999)),
            ),
        }
    }

    #[test]
    fn an_offer_round_trips_through_one_drop() {
        let original = offer();
        let parsed = Offer::from_bytes(&original.to_bytes()).unwrap();
        assert_eq!(parsed.inviter.as_bytes(), original.inviter.as_bytes());
        assert_eq!(parsed.ward, original.ward);
        assert_eq!(parsed.retention, original.retention);
        assert_eq!(parsed.declaration, original.declaration);
        assert_eq!(parsed.grant, original.grant);
        let unnamed = Offer {
            declaration: None,
            ..original
        };
        assert!(
            Offer::from_bytes(&unnamed.to_bytes())
                .unwrap()
                .declaration
                .is_none()
        );
    }

    #[test]
    fn an_offer_from_another_version_is_refused_rather_than_guessed() {
        let mut bytes = offer().to_bytes();
        if let Some(version) = bytes.first_mut() {
            *version = OFFER_VERSION + 1;
        }
        assert!(Offer::from_bytes(&bytes).is_err());
    }
}
