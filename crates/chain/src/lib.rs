// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The rules a sequence of segments must satisfy to be a chain.
//!
//! Verification here is a fold, not a collection: [`Verifier`] holds one author
//! and one head no matter how long the chain is. That is the whole reason this
//! crate exists as something other than a loop at the call site — the memory
//! shape is the design.

mod fork;
mod verify;

pub use fork::{Fork, fork};
pub use verify::{ChainError, Verifier, verify};
