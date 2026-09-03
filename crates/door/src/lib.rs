// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The door: what a verb answers, and how a failure recovers.
//!
//! Everything a caller of kusanagi ever sees comes from this crate, and nothing
//! in it can reach a waypoint, a disk or a clock. That is the whole reason it is
//! a crate rather than three modules of the binary: the shape of an answer is a
//! published contract, and a contract that lives beside the code that carries it
//! out gets changed by accident whenever that code is changed on purpose.
//!
//! The caller on the other side of this door is usually an agent, so both
//! renderings are contracts. `--json` is [`Outcome`] as `serde` writes it; the
//! prose is the same value said to somebody reading a terminal.

mod complaint;
mod fence;
mod prose;
mod report;
mod rows;

pub use complaint::Complaint;
pub use fence::Fence;
pub use report::{CONTRACT, Outcome};
pub use rows::{Carried, Entry, Measured, Summary};
