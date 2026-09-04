// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The one byte string a segment has, and the only door back out of it.
//!
//! Apart from `mod.rs` because the two answer different questions: what a
//! segment *is* does not change when the way it is written down does, and a
//! layout change has to move exactly one file.

use core::num::NonZeroU64;

use crate::identity::{Handle, Signature, VerifyingKey};
use crate::link::Link;
use crate::payload::Payload;
use crate::segment::freight::{Freight, Purpose};
use crate::segment::{Segment, SegmentError, SegmentId};
use crate::trail::{Commitment, Reveal};
use crate::wire::Reader;

/// Domain separation for what the author signs.
///
/// Distinct from the identity domain in `mod.rs` so that a segment identifier
/// can never be mistaken for something an author agreed to, in either direction.
const SIGNING_DOMAIN: &[u8] = b"kusanagi.segment.v4.sign";

pub(crate) const TAG_GENESIS: u8 = 0;
pub(crate) const TAG_FOLLOWS: u8 = 1;

impl Segment {
    /// Encodes this segment into its one canonical byte string.
    #[must_use]
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        match self.link {
            Link::Genesis { commit, signature } => {
                let mut out = genesis_body(&self.author, commit, &self.freight);
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
                freight_bytes(&mut out, &self.freight);
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
    /// Every malformed input has its own variant of [`SegmentError`]; nothing
    /// here panics, and trailing bytes are refused so that one segment keeps
    /// exactly one spelling.
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
        let freight = take_freight(&mut reader)?;

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
            freight,
        })
    }
}

/// The three fields that travel under the chain, in their fixed order.
fn freight_bytes(out: &mut Vec<u8>, freight: &Freight) {
    out.extend_from_slice(&freight.acknowledged.to_be_bytes());
    out.push(freight.purpose.byte());
    out.extend_from_slice(&freight.payload.declared_len());
    out.extend_from_slice(freight.payload.bytes());
}

/// Reads them back, refusing a purpose this build does not know.
fn take_freight(reader: &mut Reader<'_>) -> Result<Freight, SegmentError> {
    let acknowledged = reader.take_u64()?;
    let purpose = Purpose::of(reader.take_byte()?)?;
    let declared = reader.take_u32()?;
    let wanted = usize::try_from(declared)
        .map_err(|_| SegmentError::PayloadUnrepresentable { len: declared })?;
    let payload = Payload::new(reader.take(wanted)?.to_vec())?;
    Ok(Freight {
        payload,
        purpose,
        acknowledged,
    })
}

/// Everything a genesis segment is, except the signature over it.
pub(crate) fn genesis_body(author: &Handle, commit: Commitment, freight: &Freight) -> Vec<u8> {
    let mut out = Vec::new();
    out.push(TAG_GENESIS);
    out.extend_from_slice(&0_u64.to_be_bytes());
    out.extend_from_slice(author.as_bytes());
    out.extend_from_slice(commit.as_bytes());
    freight_bytes(&mut out, freight);
    out
}

/// The exact message an author signs, once, at the foot of a chain.
///
/// **The freight is deliberately outside it**, and that omission is the whole of
/// what makes a stream deniable. A signature over what was said is proof of what
/// was said, transferable to anybody, forever, without the author's
/// participation — which is the property this design exists to destroy. The
/// acknowledgement is outside for the same reason and a sharper one: a signature
/// over *how much of you I read* is proof that a conversation happened at all,
/// which is the fact this network spends every derived address to hide.
///
/// What is signed is only that this author opened a stream committing to this
/// first proof: enough to stop a peer who holds the channel secret from racing
/// to height zero, and not enough to convict anybody of a sentence.
///
/// A reader loses nothing by it. The bytes arrive inside an authenticated
/// envelope sealed under a key derived from the channel secret, at an address
/// derived from the same secret, so a host cannot alter a payload and a stranger
/// cannot supply one. The only party who can put different words at this height
/// is the peer who already read it — and that is exactly the party who must not
/// be able to prove what the words were.
pub(crate) fn signed_bytes(author: &Handle, commit: Commitment) -> Vec<u8> {
    let mut out = Vec::with_capacity(SIGNING_DOMAIN.len().saturating_add(72));
    out.extend_from_slice(SIGNING_DOMAIN);
    out.extend_from_slice(author.as_bytes());
    out.extend_from_slice(commit.as_bytes());
    out.extend_from_slice(&0_u64.to_be_bytes());
    out
}
