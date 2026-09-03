// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a host does when a stranger describes a request that is not true.
//!
//! A host answers anybody, so every number in a request head is a number
//! somebody chose. The one that matters is `Content-Length`: a host that sizes a
//! buffer from it is a host anybody can exhaust with a few bytes, and the cost of
//! the attack is one connection.
//!
//! Over a socket rather than through `HttpWaypoint`, because a client that
//! refuses to send a lie proves nothing about a server that receives one.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::disallowed_methods,
    clippy::format_push_string,
    reason = "test code"
)]

mod common;

use std::io::{Read as _, Write as _};
use std::net::TcpStream;
use std::time::{Duration, Instant};

use common::{host, probe, status};

/// A well-formed address, used only so the request reaches the routing.
const ADDRESS: &str = "0123456789abcdef0123456789abcdef01234567";

/// Heads that describe something the sender is not going to send.
fn lies() -> Vec<(&'static str, String)> {
    vec![
        (
            "a body larger than any drop",
            format!("PUT /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\nContent-Length: 4294967295\r\n\r\n"),
        ),
        (
            "a body larger than memory",
            format!(
                "PUT /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\n\
                 Content-Length: 18446744073709551615\r\n\r\n"
            ),
        ),
        (
            "a length that is not a number",
            format!("PUT /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\nContent-Length: -1\r\n\r\n"),
        ),
        (
            "a length with a sign in front of it",
            format!("PUT /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\nContent-Length: +8\r\n\r\n"),
        ),
        (
            "a length said twice",
            format!(
                "PUT /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\n\
                 Content-Length: 1\r\nContent-Length: 99999999\r\n\r\nx"
            ),
        ),
    ]
}

#[test]
fn a_length_a_stranger_declares_is_never_a_length_this_host_allocates() {
    let asked = lies();
    let (address, root) = host("robust-lengths", asked.len());

    for (what, request) in &asked {
        let answer = probe(&address, request);
        assert!(!answer.is_empty(), "{what}: the host said nothing at all");
        assert_eq!(
            status(&answer),
            400,
            "{what}: answered {} instead of refusing",
            status(&answer)
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_head_that_never_ends_is_cut_off_rather_than_collected() {
    let (address, root) = host("robust-head", 1);
    // Sixteen kilobytes of headers against an eight-kilobyte limit. The limit
    // has to bite while the head is being read, not after it is complete, or the
    // limit is a report rather than a bound.
    let mut request = format!("GET /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\n");
    for index in 0..512 {
        request.push_str(&format!("X-{index}: {}\r\n", "p".repeat(24)));
    }
    request.push_str("\r\n");

    // Written and read tolerantly, because a host that stops reading mid-request
    // and closes will reset the connection under a client still writing. That
    // reset **is** the refusal; what is asserted is that it arrives quickly and
    // that nothing more than a refusal comes back.
    let began = Instant::now();
    let mut stream = TcpStream::connect(&address).expect("the host is not listening");
    // A deadline of our own: the host holds a connection open for its idle
    // timeout after answering, and what is being timed here is how long it takes
    // to *refuse*, not how long it waits afterwards.
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok();
    stream.write_all(request.as_bytes()).ok();
    stream.flush().ok();
    let mut answer = Vec::new();
    stream.read_to_end(&mut answer).ok();
    let took = began.elapsed();

    assert!(
        took < Duration::from_secs(5),
        "the host was still reading after {took:?}"
    );
    assert!(
        answer.is_empty() || status(&answer) == 400,
        "a head over the limit was answered with {}",
        String::from_utf8_lossy(&answer)
    );

    std::fs::remove_dir_all(&root).ok();
}
