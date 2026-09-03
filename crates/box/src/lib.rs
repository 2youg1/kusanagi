// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The untrusted half of the network, as a program somebody can run.
//!
//! A box holds sealed bytes at opaque addresses. It cannot read them, cannot say
//! who wrote them, and cannot tell which of them belong together — so running one
//! for other people costs a directory and a port, and being wrong about the
//! operator costs the writers nothing.
//!
//! It is a crate of its own rather than a module of `waypoint` because the two
//! are opposite ends of the same protocol: `waypoint` is what an endpoint uses to
//! reach a host, and this is the host. An endpoint that never hosts anything does
//! not compile a server, and `docs/box-protocol.md` describes what the two halves
//! agree on.

mod exchange;
mod serve;

pub use serve::{CAPACITY, Server};
