// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How much of somebody else's disk a stranger may spend.
//!
//! A host answers anybody, so a write costs the sender one request and the host
//! a drop. Without a ceiling, filling a box is an afternoon's work for one
//! script, and the person running it finds out when something else on the
//! machine stops working.
//!
//! **What happens at the ceiling is in `serve.rs`, and it is nothing.** A write
//! that would cross it is dropped and answered exactly like every other write —
//! a host that reported being full would be letting a stranger measure how much
//! of it they had used.
//!
//! Not rate-limited by address, either. A box has no reason to remember one, and
//! a limit it cannot enforce without remembering is a reason to start.

use std::path::Path;

/// How many bytes a host will hold before it stops accepting more.
///
/// A gigabyte is eight thousand drops: more than any two people will exchange,
/// and small enough that filling it is somebody's project rather than somebody's
/// afternoon. `kusanagi host --cap` moves it.
pub const CAPACITY: u64 = 1_073_741_824;

/// What is already stored beneath `root`, in bytes.
///
/// Walked per write rather than counted incrementally, because a counter is
/// state and **a host that is killed loses it** — which is the same law every
/// verb in this program holds. A box holds thousands of files, not millions, and
/// the walk is one `readdir` per shard.
///
/// A directory that cannot be read counts as empty. The alternative is refusing
/// every write because one shard is briefly locked, and the ceiling is a
/// courtesy to the host's operator rather than a security boundary.
pub(crate) fn held(root: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(root) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(data) if data.is_dir() => held(&entry.path()),
            Ok(data) => data.len(),
            Err(_) => 0,
        })
        .fold(0_u64, u64::saturating_add)
}
