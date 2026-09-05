// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A host with an access log, for the tests that read it.

#![allow(clippy::unwrap_in_result, reason = "test code")]

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use kusanagi_kernel::{Listing, Object, PutOutcome, Sweep, Waypoint, WaypointError};
use kusanagi_waypoint::DirWaypoint;

/// One line of a host's access log.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Asked {
    List(String),
    Get(Object),
}

/// A host that writes down every request, in the order it arrived. This is the
/// cheapest thing a real host can do, and every one of them does it.
pub struct Watching {
    inner: DirWaypoint,
    asked: Mutex<Vec<Asked>>,
    open: AtomicUsize,
    busiest: AtomicUsize,
}

impl Watching {
    pub fn new(root: &std::path::Path) -> Self {
        Self {
            inner: DirWaypoint::new(root),
            asked: Mutex::new(Vec::new()),
            open: AtomicUsize::new(0),
            busiest: AtomicUsize::new(0),
        }
    }

    pub fn busiest(&self) -> usize {
        self.busiest.load(Ordering::SeqCst)
    }

    pub fn asked(&self) -> Vec<Asked> {
        self.asked.lock().unwrap().clone()
    }

    pub fn forget(&self) {
        self.asked.lock().unwrap().clear();
    }

    pub fn lists(&self) -> Vec<String> {
        self.asked()
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::List(prefix) => Some(prefix),
                Asked::Get(_) => None,
            })
            .collect()
    }

    pub fn gets(&self) -> Vec<Object> {
        self.asked()
            .into_iter()
            .filter_map(|asked| match asked {
                Asked::Get(object) => Some(object),
                Asked::List(_) => None,
            })
            .collect()
    }
}

impl Waypoint for Watching {
    fn put_if_absent(&self, at: &Object, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        self.inner.put_if_absent(at, bytes)
    }

    fn get(&self, at: &Object) -> Result<Option<Vec<u8>>, WaypointError> {
        self.asked.lock().unwrap().push(Asked::Get(*at));
        let now = self.open.fetch_add(1, Ordering::SeqCst) + 1;
        self.busiest.fetch_max(now, Ordering::SeqCst);
        std::thread::sleep(std::time::Duration::from_millis(20));
        let found = self.inner.get(at);
        self.open.fetch_sub(1, Ordering::SeqCst);
        found
    }

    fn delete(&self, at: &Object) -> Result<(), WaypointError> {
        self.inner.delete(at)
    }
}

impl Listing for Watching {
    fn list(&self, sweep: &Sweep) -> Result<Vec<Object>, WaypointError> {
        self.asked.lock().unwrap().push(Asked::List(sweep.prefix()));
        self.inner.list(sweep)
    }
}
