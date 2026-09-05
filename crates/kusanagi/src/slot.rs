// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Filling one slot on a channel that writes to a schedule.
//!
//! `tick` is a one-shot verb like every other, and that is the whole design.
//! **The scheduler is outside this program** — `schtasks` on Windows, `cron` or
//! `launchd` elsewhere — so nothing here runs in the background, nothing here
//! holds a timer, and killing it loses at most one slot. A resident process that
//! woke itself up would be a second authority on when this endpoint writes, and
//! law 1 says there is none.
//!
//! What one tick does is decided in this order, and it is short on purpose:
//!
//! 1. Work out which slot the clock is in for this channel and this endpoint.
//! 2. If a drop has already been written in that slot, do nothing and say so.
//! 3. Otherwise take the front of the outbox, or a filler segment if it is
//!    empty, and write exactly one drop.
//! 4. Read the peer's lane once, whether or not there was anything to read.
//!
//! **Step 3 is why an observer learns nothing from the traffic.** A slot always
//! produces a drop, and a filler is sealed, chained and counted exactly like a
//! message — the same 131 072 bytes, at an address derived the same way. The
//! difference between an endpoint with everything to say and one with nothing is
//! then not present in what crosses the network.
//!
//! **Step 2 is what makes the verb safe to over-run.** A scheduler that fires
//! twice, or a person who runs it by hand, must not put two segments in one slot:
//! that is a burst, which is the shape being hidden. The slot a segment was
//! written in is not stored anywhere — it is recomputed from the height of the
//! stream and the clock, so a killed process leaves nothing to reconcile.

use kusanagi_door::{Complaint, Outcome};
use kusanagi_kernel::{Instant, Purpose};
use kusanagi_site::Site;

use crate::assembly::signer as take_signer;
use crate::request::Whose;
use crate::traffic::{appended, read};

/// Fills this channel's current slot, if it is not already filled.
///
/// # Errors
///
/// [`Complaint::NotSlotted`] when the channel writes on demand and has no slots
/// to fill, plus everything a send and a read report.
pub(crate) fn tick(site: &Site, name: &str, now: Instant) -> Result<Outcome, Complaint> {
    let channel = site.channel(name)?;
    let me = take_signer(site)?;
    let Some(period) = channel.cadence.period() else {
        return Err(Complaint::NotSlotted {
            name: name.to_owned(),
        });
    };

    let phase = kusanagi_seal::phase(&channel.secret, &me.handle());
    let slot = channel
        .cadence
        .slot(now.as_unix_seconds(), phase)
        .ok_or_else(|| Complaint::NotSlotted {
            name: name.to_owned(),
        })?;

    let filled = site.last_slot(name)? == Some(slot);
    let queued = site.pending(name)?.into_iter().next();
    let wrote = if filled {
        None
    } else {
        // The claim goes down before the drop goes out. A tick killed between
        // the two skips this slot instead of writing twice in it; `site::slots`
        // says why that is the safer of the two wrong answers.
        site.claim_slot(name, slot)?;
        let written = match &queued {
            Some(waiting) => appended(site, &me, name, Purpose::Message, &waiting.payload, now)?,
            None => appended(site, &me, name, Purpose::Filler, &[], now)?,
        };
        // Only once the host has it. A payload cleared before the write would be
        // a message the caller was told had been sent and that nobody will send.
        if let Some(waiting) = &queued {
            site.dequeue(name, &waiting.ticket)?;
        }
        Some(written)
    };

    // The read happens whichever way the write went, because a slot is one drop
    // out and one look in. An endpoint that only looked when it had spoken would
    // be answering the question the slot exists to refuse.
    let heard = read(site, &me, name, None, Whose::Peer, now).ok();

    Ok(Outcome::Ticked {
        name: name.to_owned(),
        slot,
        period,
        wrote: wrote.as_ref().map(|written| written.index),
        carried: match (&wrote, &queued) {
            (None, _) => "nothing",
            (Some(_), Some(_)) => "message",
            (Some(_), None) => "filler",
        },
        waiting: site.pending(name)?.len(),
        heard: heard.as_ref().and_then(height_of),
    })
}

/// The verified height a read reported, when it reported one.
const fn height_of(outcome: &Outcome) -> Option<u64> {
    match outcome {
        Outcome::Read { height, .. } => *height,
        _ => None,
    }
}
