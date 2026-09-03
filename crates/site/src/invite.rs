// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One line that is enough to join a network.
//!
//! There is no configuration file, no account, no directory service and no
//! bootstrap node. Everything a newcomer needs is in this string, because the
//! requirement is that somebody with no technical background can be handed one
//! thing and be a member of the network afterwards.
//!
//! It carries a **one-time key**, not a name for the invitee. Whoever writes an
//! invitation cannot know who will accept it, so the grant inside is issued to a
//! key that travels in the invitation itself; the acceptor immediately delegates
//! it onward to their own handle and the one-time key is never used again. That
//! is why an invitation is a bearer token and must be handed over the way a key
//! is handed over — and why revoking the one-time step cuts off exactly the
//! person who used it, and nobody else.
//!
//! ```text
//! version         1 byte    = 2
//! suite           1 byte    = 1, the baseline: BLAKE3, ChaCha20-Poly1305, ML-DSA-87
//! secret         32 bytes   the channel secret
//! bearer_seed    32 bytes   the one-time signing key
//! locator         N bytes   utf-8, to the end
//! ```
//!
//! **Everything else moved to a drop on the host.** Version 1 carried the
//! inviter's 2 592-byte verifying key and a grant chain in the line itself,
//! which made an ordinary invitation 20 028 characters: too long to read out,
//! too long to type, and on Windows only transportable through a clipboard that
//! keeps a history. None of that bulk is secret — a verifying key and a grant
//! are public by construction — so **the secret was being held hostage by the
//! public data beside it**. What is left here is the 64 bytes that actually are
//! secret, plus where to look; the rest sits in an [`Offer`] at an address only
//! the holder of this line can compute. About 180 characters.
//!
//! The inviter still arrives as a key rather than a name, because the acceptor
//! will read their stream and a segment names its author without carrying the
//! key that checks it. It arrives in the offer instead of the line, and the
//! grant's root step carries the same key: [`Grant::verify`] makes the two agree.

use core::fmt;

use kusanagi_grant::Grant;
use kusanagi_kernel::{Hex, Reader, Signer, VerifyingKey, unhex};
use kusanagi_seal::Secret;

use crate::error::SiteError;

const VERSION: u8 = 2;

/// The prefix version 1 used, refused by name.
///
/// A build that still mints them exists, and its invitations are not damaged —
/// they are from a format this one does not read. Saying so is the difference
/// between "ask for a new invitation" and an afternoon spent looking for a paste
/// that went wrong.
const PREVIOUS_PREFIX: &str = "kusanagi1:";

/// An invitation that ends in the middle of a field.
///
/// Its own function rather than the one `channel.rs` uses, because the same
/// truncation means two different things: half a record on disk is damage, and
/// half an invitation is a paste that was cut short.
fn mangled(error: kusanagi_kernel::Incomplete) -> SiteError {
    SiteError::BadInvitation {
        reason: error.to_string(),
    }
}

/// The one cipher suite every endpoint must implement.
///
/// A network whose members can disagree about this is not one network: two
/// endpoints with different derivations cannot even compute each other's
/// addresses, so they are not degraded, they are partitioned.
///
/// **Suite 0 was the same field with Ed25519 in it.** The number moved when the
/// signature scheme did, because the whole job of this byte is the one
/// `ARCHITECTURE.md` §8 gives it — an endpoint "refuses one it does not know".
/// Leaving it at 0 would leave a build from before the change believing it knew
/// this suite, accepting the invitation, and then reporting a 2 592-byte
/// verifying key as a damaged paste. The one failure this byte exists to
/// prevent is the one it would have caused.
const BASELINE_SUITE: u8 = 1;

/// The prefix that makes an invitation recognisable when pasted into anything.
const PREFIX: &str = "kusanagi2:";

/// Everything a newcomer needs, and nothing else.
#[derive(Clone, Debug)]
pub struct Invite {
    /// The channel secret.
    pub secret: Secret,
    /// The one-time key the grant was issued to.
    pub bearer_seed: [u8; 32],
    /// Where the drops live.
    pub locator: String,
}

/// What an invitation points at: who is inviting, and by what authority.
///
/// Sealed into one drop at the address [`kusanagi_seal::offer`] derives from the
/// channel secret. Public data, kept off the line because it is large, and kept
/// out of the clear because an address nobody can compute costs less than an
/// argument about whether it mattered.
///
/// ```text
/// version    1 byte     = 1
/// inviter 2592 bytes    the inviter's verifying key
/// grant      the rest   inviter -> bearer
/// ```
#[derive(Clone, Debug)]
pub struct Offer {
    /// Who issued the invitation, and the root of every grant on the channel.
    pub inviter: VerifyingKey,
    /// The grant from the inviter to the one-time key.
    pub grant: Grant,
}

/// The layout of an offer, which versions apart from the invitation's own.
const OFFER_VERSION: u8 = 1;

impl Offer {
    /// The bytes that go in the drop.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![OFFER_VERSION];
        out.extend_from_slice(self.inviter.as_bytes());
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
        let rest = reader.take(reader.remaining()).map_err(mangled)?;
        Ok(Self {
            inviter,
            grant: Grant::from_canonical_bytes(rest)?,
        })
    }
}

impl Invite {
    /// The one-time signer this invitation carries.
    #[must_use]
    pub fn bearer(&self) -> Signer {
        Signer::from_seed(&self.bearer_seed)
    }

    /// Four hexadecimal digits two people can read to each other.
    ///
    /// **This is what makes an invitation checkable in person.** Both ends
    /// compute it from the same 32 bytes, so a line altered in transit produces
    /// four different characters at the other end. Four is short enough to say
    /// out loud and long enough that somebody rewriting a line in flight has to
    /// be lucky one time in 65 536 — and they get one try, because the wrong
    /// answer is spoken aloud.
    #[must_use]
    pub fn check(&self) -> String {
        let digest = blake3::hash(self.secret.as_bytes());
        Hex(&digest.as_bytes()[..2]).to_string()
    }

    /// Parses an invitation.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadInvitation`] when the text is not an invitation this
    /// build understands, including one for a different cipher suite or an
    /// earlier format.
    pub fn parse(text: &str) -> Result<Self, SiteError> {
        let text = text.trim();
        if text.starts_with(PREVIOUS_PREFIX) {
            return Err(SiteError::BadInvitation {
                reason: "this is a version 1 invitation, which carried the inviter key and \
                         the grant inline; this build reads version 2"
                    .to_owned(),
            });
        }
        let body = text.strip_prefix(PREFIX).ok_or(SiteError::BadInvitation {
            reason: format!("an invitation starts with `{PREFIX}`"),
        })?;
        let bytes = unhex(body)?;

        let mut reader = Reader::new(&bytes);
        let version = reader.take_byte().map_err(mangled)?;
        let suite = reader.take_byte().map_err(mangled)?;
        if version != VERSION || suite != BASELINE_SUITE {
            return Err(SiteError::BadInvitation {
                reason: format!(
                    "this invitation is version {version} suite {suite}; \
                     this build speaks version {VERSION} suite {BASELINE_SUITE}"
                ),
            });
        }

        let secret = Secret::from_bytes(reader.take_array::<32>().map_err(mangled)?);
        let bearer_seed = reader.take_array::<32>().map_err(mangled)?;
        // The locator runs to the end: there is nothing behind it to be
        // delimited from, and a length prefix would be two bytes spent saying so.
        let rest = reader.take(reader.remaining()).map_err(mangled)?;
        let locator = String::from_utf8(rest.to_vec()).map_err(|_| SiteError::BadInvitation {
            reason: "the locator in this invitation is not text".to_owned(),
        })?;
        if locator.is_empty() {
            return Err(SiteError::BadInvitation {
                reason: "this invitation names no waypoint".to_owned(),
            });
        }
        Ok(Self {
            secret,
            bearer_seed,
            locator,
        })
    }
}

impl fmt::Display for Invite {
    /// Renders the invitation as the single line that is handed to somebody.
    ///
    /// This prints a private key, because that is what a bearer invitation is.
    /// Anything that logs one has given away a channel.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = vec![VERSION, BASELINE_SUITE];
        bytes.extend_from_slice(self.secret.as_bytes());
        bytes.extend_from_slice(&self.bearer_seed);
        bytes.extend_from_slice(self.locator.as_bytes());
        write!(f, "{PREFIX}{}", Hex(&bytes))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::string_slice,
    reason = "test code"
)]
mod tests {
    use super::{Invite, Offer, PREFIX, PREVIOUS_PREFIX, SiteError};
    use kusanagi_grant::{Abilities, Grant, Scope};
    use kusanagi_kernel::{Instant, Signer};
    use kusanagi_seal::Secret;

    fn invite() -> Invite {
        Invite {
            secret: Secret::from_bytes([3; 32]),
            bearer_seed: [2_u8; 32],
            locator: "http://box.example:8443".to_owned(),
        }
    }

    fn offer() -> Offer {
        let inviter = Signer::from_seed(&[1; 32]);
        let bearer = Signer::from_seed(&[2; 32]);
        Offer {
            inviter: inviter.verifying_key(),
            grant: Grant::issue(
                &inviter,
                &bearer.handle(),
                Scope::new(Abilities::ALL, Instant::from_unix_seconds(9_999)),
            ),
        }
    }

    #[test]
    fn an_invitation_round_trips_through_one_line() {
        let original = invite();
        let text = original.to_string();
        assert!(text.starts_with(PREFIX));
        assert!(!text.contains(char::is_whitespace));

        let parsed = Invite::parse(&text).unwrap();
        assert_eq!(parsed.secret.as_bytes(), original.secret.as_bytes());
        assert_eq!(parsed.bearer_seed, original.bearer_seed);
        assert_eq!(parsed.locator, original.locator);
    }

    /// The whole point of version 2: a line somebody can read out.
    #[test]
    fn an_invitation_is_short_enough_to_read_to_somebody() {
        let text = invite().to_string();
        assert!(
            text.len() < 200,
            "an invitation is {} characters, which is not a line",
            text.len()
        );
    }

    #[test]
    fn an_offer_round_trips_through_one_drop() {
        let original = offer();
        let parsed = Offer::from_bytes(&original.to_bytes()).unwrap();
        assert_eq!(parsed.inviter.as_bytes(), original.inviter.as_bytes());
        assert_eq!(parsed.grant, original.grant);
    }

    #[test]
    fn an_offer_from_another_version_is_refused_rather_than_guessed() {
        let mut bytes = offer().to_bytes();
        bytes[0] = 9;
        assert!(Offer::from_bytes(&bytes).is_err());
    }

    #[test]
    fn the_check_code_follows_the_secret_and_nothing_else() {
        let one = invite();
        let mut other = invite();
        other.locator = "http://elsewhere:8443".to_owned();
        assert_eq!(one.check(), other.check());
        assert_eq!(one.check().len(), 4);

        other.secret = Secret::from_bytes([4; 32]);
        assert_ne!(one.check(), other.check());
    }

    #[test]
    fn surrounding_whitespace_is_forgiven() {
        let text = format!(
            "  {}
",
            invite()
        );
        assert!(Invite::parse(&text).is_ok());
    }

    #[test]
    fn text_without_the_prefix_is_refused() {
        let text = invite().to_string();
        let stripped = text.strip_prefix(PREFIX).unwrap();
        assert!(Invite::parse(stripped).is_err());
    }

    /// The format that carried the inviter key and the grant inline.
    ///
    /// It has to be refused by name. An endpoint that reads a version 1 line as
    /// a version 2 one reports a corrupted paste to somebody whose paste was
    /// perfect, and sends them looking for a problem that is not there.
    #[test]
    fn the_format_this_network_has_left_behind_is_refused_by_name() {
        let text = invite().to_string();
        let body = text.strip_prefix(PREFIX).unwrap();
        let old = format!("{PREVIOUS_PREFIX}{body}");
        let refused = Invite::parse(&old).unwrap_err();
        let SiteError::BadInvitation { reason } = refused else {
            panic!("a version 1 invitation was not refused as an invitation");
        };
        assert!(reason.contains("version 1"), "{reason}");
    }

    /// Byte 1 is the suite, and its second hexadecimal character is at index 3.
    fn with_suite(text: &str, suite: char) -> String {
        let body = text.strip_prefix(PREFIX).unwrap();
        format!("{PREFIX}{}{suite}{}", &body[..3], &body[4..])
    }

    #[test]
    fn a_future_suite_is_refused_rather_than_guessed() {
        let text = invite().to_string();
        assert!(Invite::parse(&with_suite(&text, '9')).is_err());
    }

    #[test]
    fn the_suite_this_network_has_left_behind_is_refused() {
        let text = invite().to_string();
        let refused = Invite::parse(&with_suite(&text, '0'));
        assert!(matches!(refused, Err(SiteError::BadInvitation { .. })));
    }
}
