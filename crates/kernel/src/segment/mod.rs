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
//! genesis:  tag 1 + index 8 + author 32 + commit 32 + payload_len 4 + payload + sig 4627
//! follows:  tag 1 + index 8 + previous 32 + author 32 + reveal 32 + commit 32
//!           + payload_len 4 + payload
//! ```
//!
//! A genesis segment spends 4 704 bytes besides its payload and a following one
//! spends 141, and the envelope above them shows one length whichever it is.
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

mod refusal;

pub use refusal::SegmentError;

use crate::identifier;
use crate::identity::{Handle, Signature, Signer, VerifyingKey};
use crate::link::{ChainHead, Link};
use crate::payload::Payload;
use crate::trail::{Commitment, Reveal, Trail};
use crate::wire::Reader;

/// Domain separation for segment identity.
///
/// The version is part of the prefix so that a future layout change produces
/// different identifiers for the same bytes. Two encodings sharing one address
/// space is the failure this prevents.
const SEGMENT_DOMAIN: &[u8] = b"kusanagi.segment.v3";

/// Domain separation for what the author signs.
///
/// Distinct from [`SEGMENT_DOMAIN`] so that a segment identifier can never be
/// mistaken for something an author agreed to, in either direction.
const SIGNING_DOMAIN: &[u8] = b"kusanagi.segment.v3.sign";

const TAG_GENESIS: u8 = 0;
const TAG_FOLLOWS: u8 = 1;

identifier! {
    /// The content address of a segment.
    SegmentId, 32
}

/// The only thing that crosses a boundary.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Segment {
    link: Link,
    author: Handle,
    payload: Payload,
}

impl Segment {
    /// Starts a chain.
    ///
    /// The one signed segment, and the one that fixes what height one must show.
    ///
    /// # Errors
    ///
    /// [`SegmentError::PayloadTooLarge`] when the payload exceeds [`MAX_PAYLOAD`](crate::MAX_PAYLOAD).
    pub fn genesis(signer: &Signer, trail: &Trail, payload: Vec<u8>) -> Result<Self, SegmentError> {
        let author = signer.handle();
        let payload = Payload::new(payload)?;
        let commit = trail.commitment(1);
        let signature = signer.sign(&signed_bytes(&author, commit));
        Ok(Self {
            link: Link::Genesis { commit, signature },
            author,
            payload,
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
        payload: Vec<u8>,
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
        let payload = Payload::new(payload)?;
        Ok(Self {
            link: Link::Follows {
                index,
                previous: head.id(),
                reveal: trail.reveal(height),
                commit: trail.commitment(next),
            },
            author,
            payload,
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
        self.payload.bytes()
    }

    /// Encodes this segment into its one canonical byte string.
    #[must_use]
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        match self.link {
            Link::Genesis { commit, signature } => {
                let mut out = genesis_body(&self.author, commit, &self.payload);
                out.extend_from_slice(signature.as_bytes());
                out
            }
            Link::Follows {
                index,
                previous,
                reveal,
                commit,
            } => {
                let mut out = Vec::new();
                out.push(TAG_FOLLOWS);
                out.extend_from_slice(&index.get().to_be_bytes());
                out.extend_from_slice(previous.as_bytes());
                out.extend_from_slice(self.author.as_bytes());
                out.extend_from_slice(reveal.as_bytes());
                out.extend_from_slice(commit.as_bytes());
                out.extend_from_slice(&self.payload.declared_len());
                out.extend_from_slice(self.payload.bytes());
                out
            }
        }
    }

    /// Decodes a segment from its canonical byte string.
    ///
    /// Re-encoding the body before checking a genesis signature makes canonicity
    /// part of authenticity: bytes that decode to this segment but are not the
    /// bytes this segment encodes to produce a different signed message, and are
    /// therefore refused.
    ///
    /// `author` is whose segment the caller expects, and one naming anybody else
    /// is refused before anything else is looked at. A caller that has no
    /// expectation cannot call this at all, which is the difference between
    /// "somebody wrote these bytes" and "the peer I am reading wrote them".
    ///
    /// **A following segment is not authenticated here**, because what
    /// authenticates it is the commitment made by the segment beneath it, and
    /// only a reader walking the chain holds that. `kusanagi_chain::Verifier`
    /// makes the comparison, and a segment that never passes through it has been
    /// read but not believed.
    ///
    /// # Errors
    ///
    /// Every malformed input has its own variant of [`SegmentError`]; nothing here
    /// panics, and trailing bytes are refused so that one segment keeps exactly
    /// one spelling.
    pub fn from_canonical_bytes(bytes: &[u8], author: &VerifyingKey) -> Result<Self, SegmentError> {
        let mut reader = Reader::new(bytes);
        let tag = reader.take_byte()?;
        let height = reader.take_u64()?;

        let previous = match tag {
            TAG_GENESIS if height != 0 => {
                return Err(SegmentError::GenesisIndexNotZero { index: height });
            }
            TAG_GENESIS => None,
            TAG_FOLLOWS => Some(SegmentId::from_bytes(reader.take_array::<32>()?)),
            other => return Err(SegmentError::UnknownTag { tag: other }),
        };

        let named = Handle::from_bytes(reader.take_array::<32>()?);
        // The name first: a segment by somebody else is a different fact from a
        // forgery, and reporting the forgery would point a reader at the wrong
        // problem — usually a host serving a drop from a stream they did not ask
        // for.
        let expected = author.handle();
        if named != expected {
            return Err(SegmentError::NotTheAuthor {
                expected,
                found: named,
            });
        }

        let reveal = match previous {
            None => None,
            Some(_) => Some(Reveal::from_bytes(reader.take_array::<32>()?)),
        };
        let commit = Commitment::from_bytes(reader.take_array::<32>()?);
        let declared = reader.take_u32()?;
        let wanted = usize::try_from(declared)
            .map_err(|_| SegmentError::PayloadUnrepresentable { len: declared })?;
        let payload = Payload::new(reader.take(wanted)?.to_vec())?;

        let link = match (previous, reveal) {
            (None, _) => {
                let signature = Signature::from_bytes(reader.take_array::<4_627>()?);
                author.verify(&signed_bytes(&named, commit), &signature)?;
                Link::Genesis { commit, signature }
            }
            (Some(previous), Some(reveal)) => {
                let index = NonZeroU64::new(height).ok_or(SegmentError::FollowsIndexZero)?;
                Link::Follows {
                    index,
                    previous,
                    reveal,
                    commit,
                }
            }
            // Unreachable by construction: `reveal` is `Some` exactly when
            // `previous` is. Modelled rather than asserted away, because an
            // `unreachable!` here would be a panic on attacker-supplied bytes.
            (Some(_), None) => return Err(SegmentError::FollowsIndexZero),
        };

        if reader.remaining() != 0 {
            return Err(SegmentError::TrailingBytes {
                count: reader.remaining(),
            });
        }

        Ok(Self {
            link,
            author: named,
            payload,
        })
    }
}

/// Everything a genesis segment is, except the signature over it.
fn genesis_body(author: &Handle, commit: Commitment, payload: &Payload) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(TAG_GENESIS);
    out.extend_from_slice(&0_u64.to_be_bytes());
    out.extend_from_slice(author.as_bytes());
    out.extend_from_slice(commit.as_bytes());
    out.extend_from_slice(&payload.declared_len());
    out.extend_from_slice(payload.bytes());
    out
}

/// The exact message an author signs, once, at the foot of a chain.
///
/// **The payload is deliberately outside it**, and that omission is the whole of
/// what makes a stream deniable. A signature over what was said is proof of what
/// was said, transferable to anybody, forever, without the author's
/// participation — which is the property this design exists to destroy. What is
/// signed is only that this author opened a stream committing to this first
/// proof: enough to stop a peer who holds the channel secret from racing to
/// height zero, and not enough to convict anybody of a sentence.
///
/// A reader loses nothing by it. The bytes arrive inside an authenticated
/// envelope sealed under a key derived from the channel secret, at an address
/// derived from the same secret, so a host cannot alter a payload and a stranger
/// cannot supply one. The only party who can put different words at this height
/// is the peer who already read it — and that is exactly the party who must not
/// be able to prove what the words were.
fn signed_bytes(author: &Handle, commit: Commitment) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIGNING_DOMAIN.len().saturating_add(72));
    out.extend_from_slice(SIGNING_DOMAIN);
    out.extend_from_slice(author.as_bytes());
    out.extend_from_slice(commit.as_bytes());
    out.extend_from_slice(&0_u64.to_be_bytes());
    out
}
