// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a newcomer says on the introduction stream, and how the inviter learns
//! who arrived.
//!
//! One message, written by `join` and read by `greet`, so its layout lives in
//! one file with both ends of it: the encoder, the decoder, and the one step
//! that turns a declared name into a believed one.

use kusanagi_grant::Grant;
use kusanagi_kernel::{Alias, Declaration, Instant, Reader, VerifyingKey, Ward};
use kusanagi_seal::rendezvous;
use kusanagi_site::{Channel, Peer, Site, Standing};
use kusanagi_waypoint::Place;

use kusanagi_door::Complaint;
use kusanagi_walk::Lane;
use kusanagi_walk::peek;

/// The height of the introduction stream that carries a newcomer's greeting.
pub(crate) const INTRODUCTION: u64 = 0;

/// What a newcomer says on the introduction stream: a key, a name, then a grant.
///
/// ```text
/// key      VerifyingKey::WIDTH bytes   the newcomer's own verifying key
/// ward     2 bytes                     the bin of the host the newcomer reads
/// name     2 + n bytes                 a length, then the newcomer's signed
///                                      declaration (`kernel::Declaration`); 0 = none
/// grant    the rest                    bearer -> that key's handle
/// ```
///
/// **The greeting is signed by the one-time bearer key, not by the newcomer.**
/// It has to be: the inviter is about to learn the newcomer's key *from* this
/// message, so a message only that key could authenticate would be one the
/// inviter could not read. The bearer key is the one thing both ends already
/// hold — the invitation carried its seed — and it is also what the
/// introduction stream's address is derived through, so the author of the
/// greeting and the owner of the lane it sits in are now the same identity.
///
/// The key inside is bound to the grant rather than trusted: `greet` refuses it
/// unless the grant it arrives with was issued to that key's handle.
pub(crate) fn greeting(
    key: &VerifyingKey,
    ward: Ward,
    name: Option<&Declaration>,
    grant: &Grant,
) -> Vec<u8> {
    let mut out = key.as_bytes().to_vec();
    out.extend_from_slice(&ward.bits().to_be_bytes());
    let declared = name.map_or_else(Vec::new, Declaration::to_bytes);
    // `Declaration::to_bytes` is at most 4 660 bytes, so the prefix always fits.
    out.extend_from_slice(
        &u16::try_from(declared.len())
            .unwrap_or(u16::MAX)
            .to_be_bytes(),
    );
    out.extend_from_slice(&declared);
    out.extend_from_slice(&grant.to_canonical_bytes());
    out
}

/// What a greeting or an offer says about its author, once their key vouches for it.
///
/// The one place a declared name becomes a believed one: the signature is
/// checked against the key the same message carries, so a name that arrived
/// beside somebody else's key is refused as `kusanagi.bad_name` rather than shown.
pub(crate) fn believed(
    name: Option<&Declaration>,
    key: &VerifyingKey,
) -> Result<Option<Alias>, Complaint> {
    name.map(|declaration| declaration.verify(key).cloned())
        .transpose()
        .map_err(Into::into)
}

/// Reads a greeting, without deciding whether to believe it.
fn read_greeting(
    payload: &[u8],
    name: &str,
) -> Result<(VerifyingKey, Ward, Option<Declaration>, Grant), Complaint> {
    let mut reader = Reader::new(payload);
    let unreadable = |reason: String| Complaint::BadGreeting {
        name: name.to_owned(),
        reason,
    };
    let key = reader
        .take_array::<{ VerifyingKey::WIDTH }>()
        .map(VerifyingKey::from_bytes)
        .map_err(|error| unreadable(error.to_string()))?;
    let ward = reader
        .take_array::<2>()
        .map(|bytes| Ward::from_bits(u16::from_be_bytes(bytes)))
        .map_err(|error| unreadable(error.to_string()))?;
    let declared = reader
        .take_array::<2>()
        .map(u16::from_be_bytes)
        .map_err(|error| unreadable(error.to_string()))?;
    let declared = reader
        .take(usize::from(declared))
        .map_err(|error| unreadable(error.to_string()))?;
    let declaration = if declared.is_empty() {
        None
    } else {
        Some(Declaration::from_bytes(declared).map_err(|error| unreadable(error.to_string()))?)
    };
    let rest = reader
        .take(reader.remaining())
        .map_err(|error| unreadable(error.to_string()))?;
    let grant = Grant::from_canonical_bytes(rest).map_err(|error| unreadable(error.to_string()))?;
    Ok((key, ward, declaration, grant))
}

/// Learns who accepted an invitation, from the introduction stream.
///
/// This is the one place a read writes: the peer it discovers is a fact that has
/// been verified against the channel's own root, and re-deriving it on every
/// command would mean paying for a request that can only ever give the same
/// answer.
pub(crate) fn greet(
    site: &Site,
    name: &str,
    channel: Channel,
    place: &Place,
    now: Instant,
) -> Result<Channel, Complaint> {
    // The introduction stream never releases and never ratchets: it carries one
    // segment, written by a key that exists for that one purpose, and both ends
    // need it openable until somebody reads it.
    let introduction = Lane {
        keys: kusanagi_seal::Keyring::Standing(
            channel.secret.stream(&channel.introduction.handle()),
        ),
        author: channel.introduction,
        // The greeting sits in the rendezvous bin, not in anybody's ward: it is
        // written by somebody this endpoint has not met, so there is no ward for
        // either end to agree on except the one the channel secret produces.
        bin: rendezvous(&channel.secret),
        opened: channel.opened,
    };
    let Some(said) = peek(place, &introduction, name, INTRODUCTION)? else {
        return Err(Complaint::NoPeerYet {
            name: name.to_owned(),
        });
    };

    let (key, peer_ward, declaration, grant) = read_greeting(said.payload(), name)?;
    // Two things have to agree before a stranger is recorded as the peer: the
    // grant descends from this channel's root, and it was issued to the handle
    // of the key the greeting announces. What the peer may *do* is not decided
    // here — `read` checks they may send before showing their segments, `send`
    // checks they may read before writing — because discovering who arrived and
    // admitting what they may do are different decisions. The greeting itself
    // was already checked against the one-time key when it was decoded, which
    // is what stops anybody but the invitee putting a key here.
    grant.verify(&channel.root, now, &site.revocations()?)?;
    // The key inside is bound to the grant rather than trusted: a greeting that
    // announced one key and carried a grant issued to another would let anybody
    // redirect this channel's peer at a stranger.
    if grant.holder()? != key.handle() {
        return Err(Complaint::BadGreeting {
            name: name.to_owned(),
            reason: "the greeting's grant was not issued to the key it announces".to_owned(),
        });
    }

    let alias = believed(declaration.as_ref(), &key)?;
    let channel = Channel {
        peer: Some(Peer {
            key,
            ward: peer_ward,
            standing: Standing::Granted(grant),
            alias,
        }),
        ..channel
    };
    site.keep(&channel)?;
    Ok(channel)
}
