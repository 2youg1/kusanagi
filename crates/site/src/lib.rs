// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Everything one endpoint keeps on its own disk, and the formats it keeps it in.
//!
//! A site is a directory: one signing seed, one file per [`Channel`], and a list
//! of revoked steps. There is no daemon, no database and no cache, so killing any
//! command loses at most that command — the law `ARCHITECTURE.md` §7 states as
//! "no resident state" is held here by there being nothing here to hold.
//!
//! [`Invite`] lives beside them because an invitation is the same bytes as a
//! channel seen from outside: it is what one endpoint hands over so that another
//! can write the record this crate stores.
//!
//! **What this crate does not do is decide what a failure means.** A
//! [`SiteError`] says what was being done and what was wrong with the bytes. The
//! stable code a caller matches on, and the command that recovers, are assigned
//! by the door in `kusanagi::Complaint` — because recovery is phrased in verbs
//! (`kusanagi channels`, `--root`) that only a front end knows it has.

mod alias;
mod archive;
mod blocks;
mod cadence;
mod cairns;
mod channel;
mod crowd;
mod egress;
mod error;
mod identity;
mod invite;
mod naming;
mod offer;
mod outbox;
mod peer;
mod ratchets;
mod records;
mod retention;
mod revoked;
mod rhythm;
mod roster;
mod site;
mod slots;
mod standing;
mod sweeps;

pub use archive::{export, import};
pub use cadence::Cadence;
pub use channel::Channel;
pub use crowd::MOST_DIGITS;
pub use egress::Egress;
pub use error::SiteError;
pub use invite::Invite;
pub use offer::Offer;
pub use outbox::Queued;
pub use peer::Peer;
pub use retention::Retention;
pub use roster::Roster;
pub use site::Site;
pub use standing::Standing;
pub use sweeps::Swept;
