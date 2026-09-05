// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How one author's stream is read out of a host: the lane it lives on, the
//! sweep that fetches a bin without naming an address, and the walk that
//! verifies what the sweep found.
//!
//! One idea, moved out of `kusanagi` when that crate reached its line budget
//! (`ARCHITECTURE.md` §5): the verbs decide *what* to read and this crate
//! decides *how a read reaches a host* — which is the privacy decision D-20
//! records. Nothing here samples a clock or draws randomness; every function
//! takes the instant it is at.

mod lane;
mod source;
mod sweep;
mod walk;

pub use lane::{Lane, verified};
pub use source::Source;
pub use sweep::{CAP, DIGITS, Sweeping};
pub use walk::{Held, Reach, WINDOW, Walked, peek, track, walk};
