// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! kusanagi as a library: every verb, without a command line.
//!
//! The binary in `main.rs` translates arguments into a [`Request`] and prints the
//! [`Outcome`]. Everything else lives here, so a test — or a second front end —
//! can drive the whole program in process. That split is the reason the
//! end-to-end behaviour of this network is checked by `cargo test` rather than by
//! a shell script nobody runs.

mod assembly;
mod complaint;
mod report;
mod request;
mod walk;
mod world;

pub use assembly::run;
pub use complaint::Complaint;
pub use report::Outcome;
pub use request::Request;
pub use walk::{Held, Walked, peek, walk};
pub use world::{SystemClock, fresh_seed};

// What an endpoint keeps on its own disk is a crate of its own, and a caller of
// this one should not have to know that. These are re-exported rather than
// re-declared so that there is one definition of a channel record, not two.
pub use kusanagi_site::{Channel, Invite, Peer, Site, SiteError, Standing};
