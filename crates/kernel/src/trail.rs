// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One author's private sequence of one-time proofs for one stream.
//!
//! A signature is transferable. That is what it is for, and it is why a peer who
//! is compromised or coerced holds not merely knowledge of what you said but
//! proof of it that convinces anybody, forever, without your participation. A
//! trail replaces the proof with a commitment:
//!
//! ```text
//! secret(i)  = KDF("…trail…", seed ‖ i)
//! commit(i)  = H("…commit…" ‖ secret(i))
//! segment i  carries reveal = secret(i) and commit = commit(i + 1)
//! a reader   accepts segment i when H(reveal) is the commitment segment i − 1 made
//! ```
//!
//! **Why it is sound.** Forging segment *i*, or racing to height *i* before its
//! author reaches it, needs a preimage of the commitment the segment below it
//! published. The author has revealed nothing at that height yet, and the
//! address is write-once, so there is nothing to copy and nothing to guess.
//!
//! **Why it is deniable.** Anybody who has read the stream can afterwards
//! fabricate a different stream that verifies exactly as well, because in their
//! own fabrication they choose the commitments. A quotation is therefore an
//! assertion by whoever quotes it, not evidence about whoever is quoted.
//!
//! **Nothing is stored.** The seed is derived on demand and erased with the
//! value that held it; `secret(i)` is O(1) in *i*, so a stream of any length
//! needs no chain of hashes kept anywhere. Law 1 in `ARCHITECTURE.md` §7 holds
//! unchanged: kill any process and the next one recomputes the same trail.

use zeroize::{Zeroize as _, ZeroizeOnDrop};

use crate::identifier;

/// Context for the per-height secret. BLAKE3's key derivation mode, because
/// this output is key material and nothing else in this workspace derives key
/// material any other way.
const SECRET_CONTEXT: &str = "kusanagi 2026-01-01 trail: one author's one-time proofs";

/// Domain separation for a commitment. A plain hash rather than a derivation:
/// a commitment is published in a segment, so it is an identifier.
const COMMIT_DOMAIN: &[u8] = b"kusanagi.trail.commit.v1";

identifier! {
    /// What a segment shows to prove that its author wrote the one below it.
    ///
    /// Published, and therefore worthless afterwards: a reveal that has appeared
    /// in a segment authenticates nothing that has not already been written,
    /// because the address at its height is taken.
    Reveal, 32
}

identifier! {
    /// What a segment promises about the segment above it.
    Commitment, 32
}

impl Reveal {
    /// The commitment this reveal answers.
    ///
    /// The whole of verification: a reader holds the commitment the previous
    /// segment made and compares it against this, in constant time, because
    /// every [`Digest`](crate::Digest) compares in constant time.
    #[must_use]
    pub fn commitment(&self) -> Commitment {
        let mut hasher = blake3::Hasher::new();
        hasher.update(COMMIT_DOMAIN);
        hasher.update(self.as_bytes());
        Commitment::from_bytes(*hasher.finalize().as_bytes())
    }
}

/// The seed every proof on one stream comes from.
///
/// Held only for as long as a command runs, erased on the way out, and never
/// written down. Deriving it is `kusanagi_seal::Stream::trail`, which is where
/// every other derivation in this network lives; this type is the arithmetic and
/// holds no opinion about where the seed came from.
///
/// **Deliberately not comparable and not printable.** Two trails are never
/// compared, and a trail that reaches a log has handed somebody the ability to
/// write the rest of a stream.
#[derive(Clone, ZeroizeOnDrop)]
pub struct Trail([u8; 32]);

impl Trail {
    /// Wraps a derived seed.
    #[must_use]
    pub const fn from_seed(seed: [u8; 32]) -> Self {
        Self(seed)
    }

    /// The proof this author shows at `index`.
    #[must_use]
    pub fn reveal(&self, index: u64) -> Reveal {
        let mut hasher = blake3::Hasher::new_derive_key(SECRET_CONTEXT);
        hasher.update(&self.0);
        hasher.update(&index.to_be_bytes());
        let mut derived = *hasher.finalize().as_bytes();
        let reveal = Reveal::from_bytes(derived);
        derived.zeroize();
        reveal
    }

    /// What a segment at `index - 1` promises about `index`.
    #[must_use]
    pub fn commitment(&self, index: u64) -> Commitment {
        self.reveal(index).commitment()
    }
}

impl core::fmt::Debug for Trail {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("Trail(redacted)")
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod tests {
    use super::{Reveal, Trail};
    use std::collections::BTreeSet;

    fn trail() -> Trail {
        Trail::from_seed([3_u8; 32])
    }

    #[test]
    fn a_seed_determines_every_proof_on_the_stream() {
        assert_eq!(trail().reveal(7), Trail::from_seed([3_u8; 32]).reveal(7));
        assert_ne!(trail().reveal(7), Trail::from_seed([4_u8; 32]).reveal(7));
    }

    #[test]
    fn a_reveal_answers_its_own_commitment_and_no_other() {
        assert_eq!(trail().reveal(1).commitment(), trail().commitment(1));
        assert_ne!(trail().reveal(1).commitment(), trail().commitment(2));
    }

    #[test]
    fn a_thousand_heights_are_a_thousand_proofs() {
        let seen: BTreeSet<Reveal> = (0..1_000).map(|index| trail().reveal(index)).collect();
        assert_eq!(seen.len(), 1_000, "two heights shared a proof");
    }

    /// The property the whole construction rests on, stated as an experiment a
    /// reader can run: what a segment publishes says nothing about what the next
    /// one will publish. A commitment that could be inverted would let anybody
    /// who read height *i* write height *i + 1*.
    #[test]
    fn a_published_reveal_does_not_produce_the_next_one() {
        let published = trail().reveal(4);
        assert_ne!(published.commitment(), trail().commitment(5));
        assert_ne!(Reveal::from_bytes(*published.as_bytes()), trail().reveal(5));
    }

    #[test]
    fn a_trail_never_prints_itself() {
        assert_eq!(format!("{:?}", trail()), "Trail(redacted)");
    }

    /// Compiles only while the seed erases itself.
    #[test]
    fn a_trail_erases_itself() {
        const fn erases<T: zeroize::ZeroizeOnDrop>() {}
        erases::<Trail>();
    }
}
