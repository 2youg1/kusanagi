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
//! version         1 byte    = 1
//! suite           1 byte    = 0, the baseline: BLAKE3, ChaCha20-Poly1305, Ed25519
//! inviter        32 bytes   the root authority for this channel
//! secret         32 bytes   the channel secret
//! bearer_seed    32 bytes   the one-time signing key
//! locator_len     2 bytes   big endian
//! locator         N bytes   utf-8
//! grant_len       2 bytes   big endian
//! grant           M bytes   inviter -> bearer
//! ```

use core::fmt;

use kusanagi_grant::Grant;
use kusanagi_kernel::{Handle, Hex, Reader, Signer, unhex};
use kusanagi_seal::Secret;

use crate::channel::{malformed, put_block, take_block, take_text};
use crate::complaint::Complaint;

const VERSION: u8 = 1;

/// The one cipher suite every endpoint must implement.
///
/// A network whose members can disagree about this is not one network: two
/// endpoints with different derivations cannot even compute each other's
/// addresses, so they are not degraded, they are partitioned.
const BASELINE_SUITE: u8 = 0;

/// The prefix that makes an invitation recognisable when pasted into anything.
const PREFIX: &str = "kusanagi1:";

/// Everything a newcomer needs, and nothing else.
#[derive(Clone, Debug)]
pub struct Invite {
    /// Who issued it, and the root of every grant on the channel.
    pub inviter: Handle,
    /// The channel secret.
    pub secret: Secret,
    /// The one-time key the grant was issued to.
    pub bearer_seed: [u8; 32],
    /// Where the drops live.
    pub locator: String,
    /// The grant from the inviter to the one-time key.
    pub grant: Grant,
}

impl Invite {
    /// The one-time signer this invitation carries.
    #[must_use]
    pub fn bearer(&self) -> Signer {
        Signer::from_seed(&self.bearer_seed)
    }

    /// Parses an invitation.
    ///
    /// # Errors
    ///
    /// [`Complaint::Malformed`] when the text is not an invitation this build
    /// understands, including one for a different cipher suite.
    pub fn parse(text: &str) -> Result<Self, Complaint> {
        let body = text
            .trim()
            .strip_prefix(PREFIX)
            .ok_or(Complaint::Malformed {
                what: "an invitation",
                reason: format!("an invitation starts with `{PREFIX}`"),
            })?;
        let bytes = unhex(body)?;

        let mut reader = Reader::new(&bytes);
        let version = reader.take_byte().map_err(malformed)?;
        let suite = reader.take_byte().map_err(malformed)?;
        if version != VERSION || suite != BASELINE_SUITE {
            return Err(Complaint::Malformed {
                what: "an invitation",
                reason: format!(
                    "this invitation is version {version} suite {suite}; \
                     this build speaks version {VERSION} suite {BASELINE_SUITE}"
                ),
            });
        }

        let inviter = Handle::from_bytes(reader.take_array::<32>().map_err(malformed)?);
        let secret = Secret::from_bytes(reader.take_array::<32>().map_err(malformed)?);
        let bearer_seed = reader.take_array::<32>().map_err(malformed)?;
        let locator = take_text(&mut reader)?;
        let grant = Grant::from_canonical_bytes(&take_block(&mut reader)?)?;

        if reader.remaining() != 0 {
            return Err(Complaint::Malformed {
                what: "an invitation",
                reason: format!(
                    "{} byte(s) follow a complete invitation",
                    reader.remaining()
                ),
            });
        }
        Ok(Self {
            inviter,
            secret,
            bearer_seed,
            locator,
            grant,
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
        bytes.extend_from_slice(self.inviter.as_bytes());
        bytes.extend_from_slice(self.secret.as_bytes());
        bytes.extend_from_slice(&self.bearer_seed);
        put_block(&mut bytes, self.locator.as_bytes());
        put_block(&mut bytes, &self.grant.to_canonical_bytes());
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
    use super::{Invite, PREFIX};
    use kusanagi_grant::{Abilities, Grant, Scope};
    use kusanagi_kernel::{Instant, Signer};
    use kusanagi_seal::Secret;

    fn invite() -> Invite {
        let inviter = Signer::from_seed(&[1; 32]);
        let bearer_seed = [2_u8; 32];
        let bearer = Signer::from_seed(&bearer_seed);
        Invite {
            inviter: inviter.handle(),
            secret: Secret::from_bytes([3; 32]),
            bearer_seed,
            locator: "http://box.example:8443".to_owned(),
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
        assert_eq!(parsed.inviter, original.inviter);
        assert_eq!(parsed.secret.as_bytes(), original.secret.as_bytes());
        assert_eq!(parsed.bearer_seed, original.bearer_seed);
        assert_eq!(parsed.locator, original.locator);
        assert_eq!(parsed.grant, original.grant);
    }

    #[test]
    fn surrounding_whitespace_is_forgiven() {
        let text = format!("  {}\n", invite());
        assert!(Invite::parse(&text).is_ok());
    }

    #[test]
    fn text_without_the_prefix_is_refused() {
        let text = invite().to_string();
        let stripped = text.strip_prefix(PREFIX).unwrap();
        assert!(Invite::parse(stripped).is_err());
    }

    #[test]
    fn a_flipped_character_never_becomes_a_different_valid_invitation() {
        let text = invite().to_string();
        let mut damaged: Vec<char> = text.chars().collect();
        let at = damaged.len() - 3;
        damaged[at] = if damaged[at] == 'a' { 'b' } else { 'a' };
        let damaged: String = damaged.into_iter().collect();
        match Invite::parse(&damaged) {
            Err(_) => {}
            Ok(parsed) => assert_ne!(parsed.grant, invite().grant),
        }
    }

    #[test]
    fn a_future_suite_is_refused_rather_than_guessed() {
        let text = invite().to_string();
        let body = text.strip_prefix(PREFIX).unwrap();
        // byte 1 is the suite; its second hex character is at index 3
        let bumped = format!("{PREFIX}{}9{}", &body[..3], &body[4..]);
        assert!(Invite::parse(&bumped).is_err());
    }
}
