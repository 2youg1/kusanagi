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
mod channel;
mod complaint;
mod invite;
mod report;
mod request;
mod site;
mod walk;
mod world;

pub use assembly::run;
pub use channel::{Channel, Peer, Standing};
pub use complaint::Complaint;
pub use invite::Invite;
pub use report::Outcome;
pub use request::Request;
pub use site::Site;
pub use walk::{Held, Walked, peek, walk};
pub use world::{SystemClock, fresh_seed};
