// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a read does with what it learned, on a channel that releases.
//!
//! Apart from `traffic.rs` because it is the one step of a read that destroys
//! something: the two verbs there move bytes, and this burns the keys that
//! opened them (D-01, D-07).

use kusanagi_kernel::{Instant, Signer};
use kusanagi_site::{Channel, Site};

use crate::assembly::peer_ward;
use kusanagi_door::Complaint;
use kusanagi_walk::Lane;
use kusanagi_walk::Walked;

/// Acts on what a read just learned, on a channel that releases.
///
/// **The ratchet is the whole of it.** The peer said how much of this endpoint's
/// stream they had verified, so the keys that opened those drops are destroyed
/// here, and a host that kept a copy holds bytes nobody can open. This endpoint
/// no longer deletes them: a `DELETE` names an address, which is the one thing
/// a read stopped doing (D-20), and a drop is filed in the period it was
/// written, which its author does not keep. Removing the bytes is the host's
/// hygiene — a lifetime on a bin — and `ARCHITECTURE.md` §3 says so.
pub(crate) fn settle(
    site: &Site,
    name: &str,
    channel: &Channel,
    walked: &Walked,
    theirs: &Lane,
    me: &Signer,
    now: Instant,
) -> Result<(), Complaint> {
    // The peer repeats their acknowledgement in every segment, so the highest
    // one in this walk is the current answer and an older segment cannot undo a
    // newer one.
    let acknowledged = walked
        .held()
        .iter()
        .map(|held| held.segment.acknowledged())
        .max()
        .unwrap_or(0);

    if acknowledged > 0 {
        let ours = Lane::open(
            site,
            name,
            channel,
            &me.verifying_key(),
            peer_ward(channel, name)?,
            now,
        )?;
        ours.burn_below(site, name, acknowledged.saturating_sub(1))?;
    }

    // The peer's own lane burns behind the reader in the same way. What was
    // verified has been handed over; nothing will ask for it again.
    if let Some(head) = walked.head() {
        theirs.burn_below(site, name, head.index())?;
    }
    Ok(())
}
