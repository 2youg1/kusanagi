// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a channel with a rhythm, or one that releases, keeps on this disk.
//!
//! Three records that the rest of a site does not have and does not need: a
//! queue of what has been said but not yet sent, the number of the last slot
//! filled, and how far this lane's keys have been destroyed. They are here
//! rather than in `site.rs` because they share one property that nothing else
//! in a site shares — **losing any of them loses something no host and no peer
//! can give back.**
//!
//! A cairn next door can be rebuilt by reading a stream again. A queued payload
//! was promised to a caller and exists nowhere else. A ratchet is the deliberate
//! destruction of a key, and re-deriving one would undo the only thing it does.
//! That is why `export` carries all three, and why a channel opened with
//! `--release` says out loud that a backup has stopped being optional.

use kusanagi_kernel::Handle;
use kusanagi_seal::Ratchet;

use crate::error::SiteError;
use crate::outbox::{self, Queued};
use crate::ratchets;
use crate::site::Site;
use crate::slots;

impl Site {
    /// How far one author's lane on one channel has been burned.
    ///
    /// `None` means no key on this lane has been destroyed yet, which for a
    /// channel that keeps its history is the permanent answer.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::BadRecord`] when the record is not a ratchet — which is a
    /// refusal rather than a miss, because guessing would restart a burned lane.
    pub fn ratchet(&self, name: &str, author: &Handle) -> Result<Option<Ratchet>, SiteError> {
        ratchets::read(self.root(), &self.filed_or_unknown(name)?, author)
    }

    /// Burns every key below `ratchet`'s floor on this lane, irreversibly.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn burn(&self, name: &str, author: &Handle, ratchet: &Ratchet) -> Result<(), SiteError> {
        ratchets::write(self.root(), &self.filed_or_unknown(name)?, author, ratchet)
    }

    /// Adds a payload to the queue a slotted channel drains one slot at a time.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn queue(&self, name: &str, payload: &[u8]) -> Result<(), SiteError> {
        outbox::push(self.root(), &self.filed_or_unknown(name)?, payload)
    }

    /// Everything waiting on this channel, oldest first.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the queue cannot be read.
    pub fn pending(&self, name: &str) -> Result<Vec<Queued>, SiteError> {
        outbox::all(self.root(), &self.filed_or_unknown(name)?)
    }

    /// Takes one payload out of the queue, once it is on the host.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be removed — which is a
    /// failure rather than a shrug: a payload still queued after it was written
    /// would be written again at a height that is already taken.
    pub fn dequeue(&self, name: &str, ticket: &str) -> Result<(), SiteError> {
        outbox::clear(self.root(), &self.filed_or_unknown(name)?, ticket)
    }

    /// Which slot this endpoint last filled on this channel.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be read.
    pub fn last_slot(&self, name: &str) -> Result<Option<u64>, SiteError> {
        slots::read(self.root(), &self.filed_or_unknown(name)?)
    }

    /// Claims `slot`, before the drop that fills it is written.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn claim_slot(&self, name: &str, slot: u64) -> Result<(), SiteError> {
        slots::write(self.root(), &self.filed_or_unknown(name)?, slot)
    }
}
