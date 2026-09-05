// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How much of somebody else's disk a stranger may spend.
//!
//! A host answers anybody, so a write costs the sender one request and the host
//! a drop. Without a ceiling, filling a box is an afternoon's work for one
//! script, and the person running it finds out when something else on the
//! machine stops working.
//!
//! **The ceiling is silent.** A host that answered "full" would be telling a
//! stranger how much of it they had used, which is a measurement they should not
//! be able to take. So the answer is the same empty `404` as everything else, and
//! the writer finds out by reading the address back — which is how a writer finds
//! out anything about a host it does not trust.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use std::net::TcpListener;
use std::path::PathBuf;

use kusanagi_box::Server;
use kusanagi_kernel::{FixedClock, Instant, PutOutcome, Waypoint as _};
use kusanagi_seal::{Fit, Secret, derive, seal};
use kusanagi_waypoint::{Access, HttpWaypoint};

/// A host holding at most `capacity` bytes, and the client that talks to it.
fn box_holding(tag: &str, capacity: u64, requests: usize) -> (HttpWaypoint, PathBuf) {
    let root = std::env::temp_dir().join(format!("kusanagi-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let directory = root.clone();
    std::thread::spawn(move || {
        let server = Server::new(
            &directory,
            FixedClock::at(Instant::from_unix_seconds(1_000)),
        )
        .holding(capacity);
        for _ in 0..requests {
            match listener.accept() {
                Ok((stream, _)) => server.answer(stream).ok(),
                Err(_) => break,
            };
        }
    });
    (
        HttpWaypoint::new(&format!("http://127.0.0.1:{port}"), &Access::default()),
        root,
    )
}

#[test]
fn a_host_that_is_full_keeps_nothing_more_and_says_nothing_about_it() {
    // Room for exactly three sealed drops, envelope included.
    let namespace =
        Secret::from_bytes([1; 32]).stream(&kusanagi_kernel::Handle::from_bytes([2; 32]));
    let said = |index: u64| {
        let (_, key) = derive(&namespace, index);
        seal(&key, Fit::Veil, &[7; 1_000]).expect("a sealed drop")
    };
    let capacity = 3 * (said(0).len() as u64 + 8);
    // Two requests per write — the write and the read that confirms it.
    let (client, root) = box_holding("capacity", capacity, 8);

    for index in 0..3 {
        let (addr, _) = derive(&namespace, index);
        assert_eq!(
            client.put_if_absent(&addr, &said(index)).unwrap(),
            PutOutcome::Stored,
            "the host refused write {index}, which was within its capacity"
        );
    }

    let (addr, _) = derive(&namespace, 3);
    let error = client
        .put_if_absent(&addr, &said(3))
        .expect_err("a full host kept a fourth drop");
    assert_eq!(error.code(), "waypoint.unwritten");

    std::fs::remove_dir_all(&root).ok();
}
