// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The only thing that travels, and the bytes that define it.
//!
//! A segment's identity is the hash of its canonical bytes, so the encoding must
//! be *canonical* in the strict sense: one segment, one byte string, forever. The
//! layout is fixed-width and big-endian, and decoding rejects trailing bytes —
//! without that rejection a segment would have infinitely many spellings and
//! content addressing would stop meaning anything.
//!
//! ```text
//! genesis:  tag 1 + index 8 + author 32 + commit 32
//!           + ack 8 + purpose 1 + payload_len 4 + payload + sig 4627
//! follows:  tag 1 + index 8 + previous 32 + author 32 + reveal 32 + commit 32
//!           + ack 8 + purpose 1 + payload_len 4 + payload
//! ```
//!
//! A genesis segment spends 4 713 bytes besides its payload and a following one
//! spends 150, and the envelope above them shows one length whichever it is.
//!
//! The three fields after the chain are one value, [`Freight`], because a caller
//! settles all three at once — see `freight.rs` for why an acknowledgement is a
//! count and why a filler is a purpose rather than an empty payload.
//!
//! **Only the first segment of a chain is signed.** A signature is transferable,
//! and a peer who is compromised or coerced would otherwise hold proof of
//! everything you ever said, convincing to anybody, forever, without you. Every
//! segment above genesis shows the one-time proof the segment below it committed
//! to, which convinces the reader who holds that commitment and convinces nobody
//! afterwards — see [`Trail`](crate::Trail). Genesis is signed because there is
//! nothing beneath it to commit to it, and because that signature is what stops
//! a peer holding the channel secret from racing to height zero.
//!
//! **What a decoded segment therefore is.** A genesis segment that exists was
//! signed, exactly as before. A following segment that exists is well-formed and
//! names an author whose key the caller supplied; whether its proof answers the
//! commitment below it is `kusanagi_chain::Verifier`'s question, because only a
//! reader walking the chain holds that commitment. Splitting the check that way
//! is what the construction *is*: the authenticator of a segment lives in the
//! segment beneath it.
//!
//! **The author field is a name, so decoding takes the key separately.** Nothing
//! here widens when the signature scheme does: a handle is 32 bytes under any
//! scheme, and a following segment carries no signature at all, so a key riding
//! along in every segment would be paid for by every message and used by none of
//! them.

use core::num::NonZeroU64;

mod canonical;
mod freight;
mod refusal;

pub use freight::{Freight, Purpose};
pub use refusal::SegmentError;

use crate::identifier;
use crate::identity::{Handle, Signer};
use crate::link::{ChainHead, Link};
use crate::trail::{Commitment, Reveal, Trail};
use canonical::signed_bytes;

/// Domain separation for segment identity.
///
/// The version is part of the prefix so that a future layout change produces
/// different identifiers for the same bytes. Two encodings sharing one address
/// space is the failure this prevents.
const SEGMENT_DOMAIN: &[u8] = b"kusanagi.segment.v4";

identifier! {
    /// The content address of a segment.
    SegmentId, 32
}

/// The only thing that crosses a boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Segment {
    link: Link,
    author: Handle,
    freight: Freight,
}

impl Segment {
    /// Starts a chain.
    ///
    /// The one signed segment, and the one that fixes what height one must show.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the payload exceeds [`MAX_PAYLOAD`](crate::MAX_PAYLOAD).
    pub fn genesis(signer: &Signer, trail: &Trail, freight: Freight) -> Result<Self, SegmentError> {
        let author = signer.handle();
        let commit = trail.commitment(1);
        let signature = signer.sign(&signed_bytes(&author, commit));
        Ok(Self {
            link: Link::Genesis { commit, signature },
            author,
            freight,
        })
    }

    /// Extends a chain by one.
    ///
    /// Takes a [`ChainHead`] rather than the predecessor itself, so extending a
    /// long chain does not require holding it. No signer, because nothing above
    /// genesis is signed: the trail is the authority, and the reveal it produces
    /// at this height is what the segment below already promised.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the payload exceeds [`MAX_PAYLOAD`](crate::MAX_PAYLOAD);
    /// [`SegmentError::ChainExhausted`] when the predecessor already sits at the
    /// last representable height, or when the height above it has no successor to
    /// commit to.
    pub fn extend(
        trail: &Trail,
        author: Handle,
        freight: Freight,
        head: ChainHead,
    ) -> Result<Self, SegmentError> {
        let height = head
            .index()
            .checked_add(1)
            .ok_or(SegmentError::ChainExhausted)?;
        // `height` is `head.index + 1`, hence at least one; the zero branch is
        // modelled rather than asserted away.
        let index = NonZeroU64::new(height).ok_or(SegmentError::ChainExhausted)?;
        let next = height.checked_add(1).ok_or(SegmentError::ChainExhausted)?;
        Ok(Self {
            link: Link::Follows {
                index,
                previous: head.id(),
                reveal: trail.reveal(height),
                commit: trail.commitment(next),
            },
            author,
            freight,
        })
    }

    /// This segment's content address.
    #[must_use]
    pub fn id(&self) -> SegmentId {
        let mut hasher = blake3::Hasher::new();
        hasher.update(SEGMENT_DOMAIN);
        hasher.update(&self.to_canonical_bytes());
        SegmentId::from_bytes(*hasher.finalize().as_bytes())
    }

    /// A witness of this segment, for whoever extends the chain next.
    #[must_use]
    pub fn head(&self) -> ChainHead {
        ChainHead::new(self.id(), self.index(), self.commit())
    }

    /// This segment's height in its chain; zero for a genesis segment.
    #[must_use]
    pub const fn index(&self) -> u64 {
        match self.link {
            Link::Genesis { .. } => 0,
            Link::Follows { index, .. } => index.get(),
        }
    }

    /// The segment directly beneath this one, absent only at genesis.
    #[must_use]
    pub const fn previous(&self) -> Option<SegmentId> {
        match self.link {
            Link::Genesis { .. } => None,
            Link::Follows { previous, .. } => Some(previous),
        }
    }

    /// What this segment promises about the one above it.
    #[must_use]
    pub const fn commit(&self) -> Commitment {
        match self.link {
            Link::Genesis { commit, .. } | Link::Follows { commit, .. } => commit,
        }
    }

    /// The proof this segment shows, absent only at genesis.
    #[must_use]
    pub const fn reveal(&self) -> Option<Reveal> {
        match self.link {
            Link::Genesis { .. } => None,
            Link::Follows { reveal, .. } => Some(reveal),
        }
    }

    /// Who wrote it.
    ///
    /// Proven by a signature at genesis and by the chain's own commitments above
    /// it, which is why a caller reads a chain rather than a segment.
    #[must_use]
    pub const fn author(&self) -> Handle {
        self.author
    }

    /// The opaque bytes this segment carries.
    #[must_use]
    pub const fn payload(&self) -> &[u8] {
        self.freight.payload.bytes()
    }

    /// Whether the author meant to say anything, or was filling a slot.
    #[must_use]
    pub const fn purpose(&self) -> Purpose {
        self.freight.purpose
    }

    /// How many of the reader's segments the author had verified when they
    /// wrote this.
    ///
    /// A count, so zero means *none* rather than *height zero*. Whoever wrote it
    /// is saying they no longer need those segments to exist, which is what a
    /// channel that releases acts on.
    #[must_use]
    pub const fn acknowledged(&self) -> u64 {
        self.freight.acknowledged
    }
}
