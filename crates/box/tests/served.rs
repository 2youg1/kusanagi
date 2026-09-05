// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The server behind a real socket, driven through the client a person uses.
//!
//! What crosses between the two threads is a TCP connection either way, which
//! is the part `serve.rs`'s unit tests never exercise.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]

use kusanagi_box::Server;
use kusanagi_kernel::{FixedClock, Instant, PutOutcome, Signer, Waypoint as _};
use kusanagi_seal::{Fit, Secret, Stream, derive, seal};
use kusanagi_waypoint::{Access, Conditional as _, Fetched, HttpWaypoint, TtlOutcome};
use std::net::TcpListener;

fn namespace(tag: u8) -> Stream {
    Secret::from_bytes([tag; 32]).stream(&Signer::from_seed(&[tag; 32]).handle())
}

/// A body the box accepts: sealed under the namespace's own key, the one size
/// everything this network writes, over `text`.
fn sealed(namespace: &Stream, index: u64, text: &[u8]) -> Vec<u8> {
    let (_, key) = derive(namespace, index);
    seal(&key, Fit::Veil, text).expect("a sealed segment")
}

/// Starts a real server on a real port and returns a client pointed at it.
///
/// The two processes of the acceptance criterion become two threads here;
/// what crosses between them is a TCP connection either way, which is the
/// part that had never been exercised before this module existed.
fn box_on(tag: &str, clock: FixedClock) -> (HttpWaypoint, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("kusanagi-serve-{}-{tag}", std::process::id()));
    let listener = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
    let port = listener.local_addr().expect("no local address").port();
    let directory = root.clone();
    std::thread::spawn(move || {
        let host = Server::new(&directory, clock);
        match host.serve(&listener) {
            Ok(()) => {}
            Err(error) => eprintln!("test host stopped: {error}"),
        }
    });
    (
        HttpWaypoint::new(&format!("http://127.0.0.1:{port}"), &Access::default()),
        root,
    )
}

#[test]
fn a_segment_crosses_a_tcp_connection_and_comes_back_whole() {
    let (client, root) = box_on(
        "roundtrip",
        FixedClock::at(Instant::from_unix_seconds(1_000)),
    );
    let (addr, _) = derive(&namespace(1), 0);
    let body = sealed(&namespace(1), 0, b"a segment");

    assert_eq!(client.get(&addr).unwrap(), None);
    assert_eq!(
        client.put_if_absent(&addr, &body).unwrap(),
        PutOutcome::Stored
    );
    assert_eq!(client.get(&addr).unwrap(), Some(body));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_claimed_drop_is_refused_a_second_time() {
    let (client, root) = box_on(
        "write-once",
        FixedClock::at(Instant::from_unix_seconds(1_000)),
    );
    let (addr, _) = derive(&namespace(2), 0);
    let first = sealed(&namespace(2), 0, b"first");
    let second = sealed(&namespace(2), 0, b"second");
    assert_ne!(first, second);
    // Identical bytes rewritten report Stored, not AlreadyPresent: a resend
    // after a lost acknowledgement finds its own bytes there, and the correct
    // answer is to carry on. Occupancy is therefore asserted with different
    // bytes.
    assert_eq!(
        client.put_if_absent(&addr, &first).unwrap(),
        PutOutcome::Stored
    );
    assert_eq!(
        client.put_if_absent(&addr, &second).unwrap(),
        PutOutcome::AlreadyPresent
    );
    assert_eq!(client.get(&addr).unwrap(), Some(first));

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_reader_that_is_current_is_told_so_without_the_bytes() {
    let (client, root) = box_on(
        "conditional",
        FixedClock::at(Instant::from_unix_seconds(1_000)),
    );
    let (addr, _) = derive(&namespace(3), 0);
    client
        .put_if_absent(&addr, &sealed(&namespace(3), 0, b"a segment"))
        .unwrap();

    let Fetched::Fresh { validator, .. } = client.get_if_changed(&addr, None).unwrap() else {
        panic!("the host did not send the bytes it was holding");
    };
    let validator = validator.expect("the host named no version");
    assert_eq!(
        client.get_if_changed(&addr, Some(&validator)).unwrap(),
        Fetched::Unchanged
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_object_written_already_expired_is_never_served() {
    let (client, root) = box_on("expiry", FixedClock::at(Instant::from_unix_seconds(1_000)));
    let (addr, _) = derive(&namespace(4), 0);

    assert_eq!(
        client
            .put_with_ttl(&addr, &sealed(&namespace(4), 0, b"transient"), 0)
            .unwrap(),
        TtlOutcome::Accepted
    );
    assert_eq!(client.get(&addr).unwrap(), None);

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn the_whole_contract_holds_over_tcp() {
    let (client, root) = box_on(
        "conformance",
        FixedClock::at(Instant::from_unix_seconds(1_000)),
    );
    kusanagi_waypoint::conformance::run(&client, &namespace(5))
        .expect("the box broke the contract");
    std::fs::remove_dir_all(&root).ok();
}
