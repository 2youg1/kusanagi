// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One author's lane on one channel, opened the way that channel says.
//!
//! Two facts have to be settled together before anything can be read or written:
//! where the drops sit, and what opens them. On most channels the second follows
//! from the first forever; on a channel that releases, it burns behind. **No verb
//! should have to know which**, so this is where the question is asked, once, and
//! the answer is a [`Keyring`] that behaves the same either way.
//!
//! It is also where the two irreversible things meet the two recoverable ones. A
//! cairn says how far a lane was verified and can always be rebuilt; a ratchet
//! says how far its keys were destroyed and can be rebuilt by nobody. On a
//! releasing channel losing the first while keeping the second is a state this
//! endpoint must refuse rather than paper over — a walk from height zero would
//! find deleted drops, conclude the stream never started, and report that as
//! fact.

use kusanagi_kernel::{Bin, DropAddr, Handle, Object, Period, VerifyingKey, Ward};
use kusanagi_seal::{Keyring, Ratchet};
use kusanagi_site::{Channel, Site};

use kusanagi_door::Complaint;

/// One author's lane, with whatever opens it.
pub struct Lane {
    /// Where the drops are and what opens them.
    pub keys: Keyring,
    /// Whose signature every segment on it must carry.
    pub author: VerifyingKey,
    /// Which bin of the host this lane's drops are filed in.
    ///
    /// **The reader's ward, never the writer's.** A drop exists to be collected,
    /// and it is collected by whoever sweeps the bin it sits in, so a lane
    /// authored by this endpoint is filed where the peer looks and a lane
    /// authored by the peer is filed where this endpoint looks. Deciding it here
    /// rather than at each call site is what stops a verb from filing a segment
    /// somewhere nobody sweeps — a message that is neither delivered nor lost.
    pub bin: Bin,
}

impl Lane {
    /// Where a drop of this lane sits on the host.
    #[must_use]
    pub const fn at(&self, addr: DropAddr) -> Object {
        Object::new(self.bin, addr)
    }

    /// Where the drop at `index` sits on the host.
    #[must_use]
    pub fn holding(&self, index: u64) -> Object {
        self.at(self.keys.address(index))
    }
}

impl Lane {
    /// Opens `author`'s lane on `channel`, as this channel's retention says.
    ///
    /// `reader` is whose ward the lane's drops are filed in: the peer's for a
    /// lane this endpoint writes, this endpoint's own for a lane it reads.
    ///
    /// **The period is fixed at zero until reads sweep.** The key layout carries
    /// the column from the first day so that no host has to relearn it, and the
    /// clock moves into it in the same change that makes a read take a whole bin
    /// — the two are one idea, because a bin can only be taken whole if it is
    /// finite.
    ///
    /// # Errors
    ///
    /// [`Complaint::NeedsCairn`] when this channel releases, its keys have been
    /// burned, and the record of what was read is gone — which means the bytes
    /// those keys opened are gone with it.
    pub fn open(
        site: &Site,
        name: &str,
        channel: &Channel,
        author: &VerifyingKey,
        reader: Ward,
    ) -> Result<Self, Complaint> {
        let named = author.handle();
        let stream = channel.secret.stream(&named);
        let bin = Bin::new(Period::from_count(0), reader);
        if !channel.retention.releases() {
            return Ok(Self {
                keys: Keyring::Standing(stream),
                author: *author,
                bin,
            });
        }

        let floor = match site.ratchet(name, &named)? {
            None => Ratchet::start(&stream),
            Some(burned) => {
                if burned.floor() > 0 && site.cairn(name, &named)?.is_none() {
                    return Err(Complaint::NeedsCairn {
                        name: name.to_owned(),
                    });
                }
                burned
            }
        };
        Ok(Self {
            keys: Keyring::Ratcheting { stream, floor },
            author: *author,
            bin,
        })
    }

    /// Destroys every key below `above` on this lane, if it ratchets at all.
    ///
    /// Called **after** whatever was read has been handed to the caller, because
    /// this is the step that makes it unreadable. A standing keyring writes
    /// nothing, so a channel that keeps its history pays nothing for this call
    /// existing.
    ///
    /// # Errors
    ///
    /// [`Complaint::Site`] when the record cannot be written — which is a
    /// failure rather than a shrug, because an endpoint that believes it burned
    /// a key and did not is an endpoint making a false promise.
    pub fn burn_below(&self, site: &Site, name: &str, above: u64) -> Result<(), Complaint> {
        let Some(burned) = self.keys.burned_through(above) else {
            return Ok(());
        };
        site.burn(name, &self.author.handle(), &burned)?;
        Ok(())
    }
}

/// How many of `author`'s segments this endpoint has verified on `name`.
///
/// A count rather than a height, which is what a segment carries and what a
/// release deletes against: zero is *none*, and there is no sentinel to get
/// wrong. A cairn at height 4 means heights 0 through 4 are verified, which is
/// five segments.
///
/// # Errors
///
/// [`Complaint::Site`] when the cairn cannot be read.
pub fn verified(site: &Site, name: &str, author: &Handle) -> Result<u64, Complaint> {
    Ok(site
        .cairn(name, author)?
        .map_or(0, |cairn| cairn.head().index().saturating_add(1)))
}
