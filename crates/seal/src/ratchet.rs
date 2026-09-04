// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Keys that only move forwards, and the one type that hides whether they do.
//!
//! `derive` gives every height of a stream a key computable from the stream
//! itself, forever. That is what makes an endpoint recoverable from nothing but
//! its channel record — and it is also what makes a host that quietly kept a
//! copy of a released drop able to open it the day somebody takes the record.
//!
//! A ratchet closes that. The state advances by one hash per height and the
//! previous state is overwritten, so a key that has been walked past cannot be
//! recomputed by anybody, including its owner:
//!
//! ```text
//! state_0     = KDF("…ratchet root…", stream)
//! state_{i+1} = KDF("…ratchet step…", state_i)
//! key_i       = KDF("…ratchet key…",  state_i)
//! ```
//!
//! **The address is deliberately not ratcheted.** An address is published to the
//! host the moment it is used, so hiding it afterwards protects nothing, while a
//! ratcheted address would make a walk unable to look one height ahead without
//! first burning the height it is standing on.
//!
//! **This costs what release costs, and for the same reason**, so the two ride
//! together on one decision: a channel whose `Retention` is `ReleaseOnAck` gets
//! both, and one that keeps its history gets neither. Deletion is the honest
//! host's half; the ratchet is the dishonest host's half. Splitting them into two
//! settings would let somebody choose the half that does nothing on its own.

use kusanagi_kernel::{DropAddr, Reader, Signer, Trail};
use zeroize::{Zeroize as _, ZeroizeOnDrop};

use crate::envelope::Key;
use crate::secret::{Stream, address_of};

const ROOT_CONTEXT: &str = "kusanagi 2026-01-01 ratchet root for one lane";
const STEP_CONTEXT: &str = "kusanagi 2026-01-01 ratchet step";
const RATCHET_KEY_CONTEXT: &str = "kusanagi 2026-01-01 ratchet drop key and nonce";

/// The record layout, so that a later shape is refused rather than misread.
const VERSION: u8 = 1;

/// A key that has been walked past, and cannot be computed again.
///
/// The one error this module has, because it is the one thing that makes a
/// ratchet different from a derivation: what is behind you is gone. A caller
/// meeting it has asked to read something this endpoint deliberately destroyed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("the key for height {wanted} was burned; this stream now opens from {floor}")]
pub struct Burned {
    /// The height that was asked for.
    pub wanted: u64,
    /// The lowest height this ratchet can still open.
    pub floor: u64,
}

impl Burned {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "seal.burned"
    }
}

/// How far one lane's keys have been advanced, and the state to advance from.
///
/// Not comparable and self-erasing, for the reasons [`Stream`] is not and is.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Ratchet {
    state: [u8; 32],
    #[zeroize(skip)]
    index: u64,
}

impl Ratchet {
    /// The ratchet a lane starts at, before anything has been written on it.
    #[must_use]
    pub fn start(stream: &Stream) -> Self {
        let mut state = blake3::derive_key(ROOT_CONTEXT, stream.as_bytes());
        let ratchet = Self { state, index: 0 };
        state.zeroize();
        ratchet
    }

    /// The lowest height this ratchet can still open.
    #[must_use]
    pub const fn floor(&self) -> u64 {
        self.index
    }

    /// The key for the height this ratchet stands at.
    #[must_use]
    pub fn key(&self) -> Key {
        let mut cipher_key = [0_u8; 32];
        let mut nonce = [0_u8; 12];
        let mut hasher = blake3::Hasher::new_derive_key(RATCHET_KEY_CONTEXT);
        hasher.update(&self.state);
        let mut output = hasher.finalize_xof();
        output.fill(&mut cipher_key);
        output.fill(&mut nonce);
        let key = Key::new(cipher_key, nonce);
        cipher_key.zeroize();
        nonce.zeroize();
        key
    }

    /// The same lane one height further on.
    ///
    /// `None` at the last representable height, which is a stream that has
    /// nothing above it rather than a failure.
    #[must_use]
    pub fn advanced(&self) -> Option<Self> {
        let index = self.index.checked_add(1)?;
        let mut state = blake3::derive_key(STEP_CONTEXT, &self.state);
        let next = Self { state, index };
        state.zeroize();
        Some(next)
    }

    /// This lane fast-forwarded to `wanted`.
    ///
    /// # Errors
    ///
    /// [`Burned`] when `wanted` is behind this ratchet, which is exactly the
    /// case a ratchet exists to make impossible.
    pub fn at(&self, wanted: u64) -> Result<Self, Burned> {
        if wanted < self.index {
            return Err(Burned {
                wanted,
                floor: self.index,
            });
        }
        let mut walking = self.clone();
        while walking.index < wanted {
            walking = walking.advanced().ok_or(Burned {
                wanted,
                floor: self.index,
            })?;
        }
        Ok(walking)
    }

    /// The record this site keeps, which is the whole of what a backup must
    /// carry: losing it loses every message the peer has not resent.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = vec![VERSION];
        out.extend_from_slice(&self.index.to_be_bytes());
        out.extend_from_slice(&self.state);
        out
    }

    /// Reads the record back.
    ///
    /// # Errors
    ///
    /// [`Burned`] is not what this reports; a malformed record is `None`,
    /// because every caller treats an unreadable ratchet the same way it treats
    /// an absent one — as a lane that cannot be opened at all.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        let mut reader = Reader::new(bytes);
        if reader.take_byte().ok()? != VERSION {
            return None;
        }
        let index = reader.take_u64().ok()?;
        let state = reader.take_array::<32>().ok()?;
        (reader.remaining() == 0).then_some(Self { state, index })
    }
}

impl core::fmt::Debug for Ratchet {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Ratchet(at {}, redacted)", self.index)
    }
}

/// Where the address and the key for one height of one lane come from.
///
/// **One type so that no verb has to branch on it.** A walk asks for the address
/// and the key at a height; whether the answer is derivable forever or burns
/// behind is a property of the channel, settled once where the channel is
/// opened. A caller that had to know would be a caller that could forget.
#[derive(Debug)]
pub enum Keyring {
    /// Every height's key follows from the stream, for as long as it exists.
    Standing(Stream),
    /// Keys move forward and burn behind, from `floor` upward.
    Ratcheting {
        /// The lane, which still supplies every address.
        stream: Stream,
        /// The lowest height that can still be opened.
        floor: Ratchet,
    },
}

impl Keyring {
    /// Where the drop for `index` sits, which is public either way.
    #[must_use]
    pub fn address(&self, index: u64) -> DropAddr {
        match self {
            Self::Standing(stream) | Self::Ratcheting { stream, .. } => address_of(stream, index),
        }
    }

    /// The lowest height this keyring can still open.
    #[must_use]
    pub const fn floor(&self) -> u64 {
        match self {
            Self::Standing(_) => 0,
            Self::Ratcheting { floor, .. } => floor.floor(),
        }
    }

    /// The trail `author` uses on this lane.
    ///
    /// Forwarded rather than reached around, so that a caller holding a keyring
    /// never has to take the lane apart to write on it.
    #[must_use]
    pub fn trail(&self, author: &Signer) -> Trail {
        match self {
            Self::Standing(stream) | Self::Ratcheting { stream, .. } => stream.trail(author),
        }
    }

    /// The key that opens the drop for `index`.
    ///
    /// # Errors
    ///
    /// [`Burned`] when this endpoint has walked past that height on a ratcheting
    /// channel and destroyed the key.
    pub fn key(&self, index: u64) -> Result<Key, Burned> {
        match self {
            Self::Standing(stream) => Ok(crate::derive(stream, index).1),
            Self::Ratcheting { floor, .. } => Ok(floor.at(index)?.key()),
        }
    }

    /// The ratchet this keyring would stand at once `index` is behind it.
    ///
    /// `None` for a standing keyring, which has nothing to write down. The
    /// caller stores what this returns *after* it has handed the segments to
    /// whoever asked for them, because storing it is what destroys them.
    #[must_use]
    pub fn burned_through(&self, index: u64) -> Option<Ratchet> {
        match self {
            Self::Standing(_) => None,
            Self::Ratcheting { floor, .. } => floor.at(index).ok()?.advanced(),
        }
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
    use super::{Keyring, Ratchet};
    use crate::Secret;
    use kusanagi_kernel::Signer;

    fn lane() -> crate::Stream {
        Secret::from_bytes([3; 32]).stream(&Signer::from_seed(&[4; 32]).handle())
    }

    #[test]
    fn every_height_gets_its_own_key_and_the_same_one_each_time() {
        let start = Ratchet::start(&lane());
        assert_ne!(
            start.key().as_parts(),
            start.at(1).unwrap().key().as_parts()
        );
        assert_eq!(
            start.at(7).unwrap().key().as_parts(),
            Ratchet::start(&lane()).at(7).unwrap().key().as_parts()
        );
    }

    /// The whole point: once the floor has moved, the key below it is gone.
    #[test]
    fn a_height_below_the_floor_cannot_be_opened_again() {
        let floor = Ratchet::start(&lane()).at(5).unwrap();
        assert_eq!(floor.floor(), 5);
        let refused = floor.at(4).unwrap_err();
        assert_eq!(refused.wanted, 4);
        assert_eq!(refused.floor, 5);
        assert_eq!(refused.code(), "seal.burned");
    }

    #[test]
    fn a_ratchet_survives_being_written_down() {
        let floor = Ratchet::start(&lane()).at(9).unwrap();
        let back = Ratchet::from_bytes(&floor.to_bytes()).unwrap();
        assert_eq!(back.floor(), 9);
        assert_eq!(back.key().as_parts(), floor.key().as_parts());
        assert!(Ratchet::from_bytes(&[0; 41]).is_none());
        let truncated = floor.to_bytes();
        assert!(Ratchet::from_bytes(truncated.get(..40).unwrap()).is_none());
    }

    /// A ratcheting keyring gives the same addresses as a standing one, because
    /// an address is published the moment it is used and hiding it later buys
    /// nothing.
    #[test]
    fn the_address_is_the_same_whichever_keyring_asks() {
        let standing = Keyring::Standing(lane());
        let ratcheting = Keyring::Ratcheting {
            stream: lane(),
            floor: Ratchet::start(&lane()),
        };
        for index in 0..4 {
            assert_eq!(standing.address(index), ratcheting.address(index));
        }
        assert_ne!(
            standing.key(2).unwrap().as_parts(),
            ratcheting.key(2).unwrap().as_parts()
        );
        assert!(standing.burned_through(3).is_none());
        assert_eq!(ratcheting.burned_through(3).unwrap().floor(), 4);
    }
}
