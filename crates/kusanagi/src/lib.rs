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
mod lane;
mod membership;
mod port;
mod request;
mod roots;
mod slot;
mod tools;
mod traffic;
mod walk;
mod world;

pub use assembly::{HOST_ADDRESS, run};
pub use lane::Lane;
pub use port::serve;
pub use request::{Habit, Request, Whose};
pub use roots::{default_host_dir, default_root};
pub use walk::{Held, Reach, WINDOW, Walked, peek, track, walk};
pub use world::{SystemClock, fresh_fence, fresh_seed};

// What an endpoint keeps on its own disk is a crate of its own, and so is what a
// verb answers; a caller of this one should not have to know that. These are
// re-exported rather than re-declared so that there is one definition of a
// channel record and one of an outcome, not two.
pub use kusanagi_door::{CONTRACT, Carried, Complaint, Entry, Fence, Measured, Outcome, Summary};
pub use kusanagi_site::{Cadence, Channel, Invite, Peer, Retention, Site, SiteError, Standing};
