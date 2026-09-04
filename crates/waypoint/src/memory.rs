// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A waypoint that forgets when the process does.
//!
//! Its job is to be the second implementation of the seam. Without it the
//! `Waypoint` trait would be a description of `DirWaypoint` rather than a
//! contract, and nobody would know which of its behaviours were essential.

use std::collections::BTreeMap;
use std::collections::btree_map::Entry;
use std::sync::{Mutex, MutexGuard, PoisonError};

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};

/// An in-process waypoint.
#[derive(Debug, Default)]
pub struct MemoryWaypoint {
    drops: Mutex<BTreeMap<DropAddr, Vec<u8>>>,
}

impl MemoryWaypoint {
    /// An empty waypoint.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Takes the lock, recovering from poisoning.
    ///
    /// Everything done under this lock is a single map insertion or lookup, so a
    /// thread that panicked elsewhere cannot have left the map inconsistent.
    /// **If a future change performs a multi-step update here, this recovery must
    /// become a propagated error instead** — the justification is the single-step
    /// update, not convenience.
    fn drops(&self) -> MutexGuard<'_, BTreeMap<DropAddr, Vec<u8>>> {
        self.drops.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl Waypoint for MemoryWaypoint {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        match self.drops().entry(*addr) {
            Entry::Occupied(_) => Ok(PutOutcome::AlreadyPresent),
            Entry::Vacant(slot) => {
                slot.insert(bytes.to_vec());
                Ok(PutOutcome::Stored)
            }
        }
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        Ok(self.drops().get(addr).cloned())
    }

    fn delete(&self, addr: &DropAddr) -> Result<(), WaypointError> {
        self.drops().remove(addr);
        Ok(())
    }
}
