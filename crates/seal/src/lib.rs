// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where a segment goes, and how it is closed before it goes there.
//!
//! Two operations, both assemblies of standard parts: [`derive`] turns a shared
//! secret into an endless supply of mutually unrelated addresses, and [`seal`]
//! closes the bytes that are left at one. Nothing here is invented. The privacy
//! of this network rests on how the standard parts are wired together, which is
//! written out in `secret.rs`, and on nothing else.

mod envelope;
mod secret;
mod veil;

pub use envelope::{Fit, Key, OpenFailed, open, seal};
pub use secret::{Secret, Stream, backup_key, derive, offer};
pub use veil::DROP;
