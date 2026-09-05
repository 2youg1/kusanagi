// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where a walk gets sealed bytes from.
//!
//! A seam because there are two honest answers and the walk must not know
//! which it got: a host asked address by address, which is what a test that
//! watches addresses drives; and a bin taken whole and matched here, which is
//! what every verb does (D-20). Verification is not behind this seam — the walk
//! opens, decodes and checks whatever comes back — so an implementation can
//! change what is fetched and never what a segment has to satisfy.

use kusanagi_kernel::Waypoint;

use crate::lane::Lane;
use kusanagi_door::Complaint;

/// Where a walk gets sealed bytes from.
///
/// A seam with two implementations: any [`Waypoint`], which asks the host for
/// each address and is what a test that watches addresses drives; and
/// [`Sweeping`], which asks the host for bins and matches addresses here. The walk
/// opens, decodes and verifies what either hands back, so neither can change
/// what a segment has to satisfy.
pub trait Source {
    /// The sealed bytes at `width` consecutive heights from `from`, in height
    /// order, `None` where nothing is.
    ///
    /// # Errors
    ///
    /// Whatever fetching them reports.
    fn sealed(
        &self,
        lane: &Lane,
        from: u64,
        width: usize,
    ) -> Result<Vec<Option<Vec<u8>>>, Complaint>;
}

impl<W: Waypoint + Sync> Source for W {
    /// **In flight together, delivered in order.** The concurrency is what a
    /// host sees; the ordering is what the verifier needs.
    fn sealed(
        &self,
        lane: &Lane,
        from: u64,
        width: usize,
    ) -> Result<Vec<Option<Vec<u8>>>, Complaint> {
        let objects: Vec<_> = (0..width)
            .filter_map(|step| from.checked_add(u64::try_from(step).ok()?))
            .map(|index| lane.holding(index))
            .collect();
        let fetched: Vec<Result<Option<Vec<u8>>, Complaint>> = std::thread::scope(|scope| {
            let running: Vec<_> = objects
                .iter()
                .map(|object| scope.spawn(move || Ok(self.get(object)?)))
                .collect();
            running
                .into_iter()
                .map(|handle| {
                    handle.join().unwrap_or_else(|_| {
                        Err(Complaint::Local {
                            action: "read a drop",
                            source: std::io::Error::other("a reader did not finish"),
                        })
                    })
                })
                .collect()
        });
        fetched.into_iter().collect()
    }
}
