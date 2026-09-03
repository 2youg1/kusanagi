// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What this endpoint announces about itself before anybody answers.
//!
//! `crates/box/tests/unmarked.rs` takes the side of somebody scanning for a
//! host. This file takes the other side: a host, a proxy, a corporate middlebox
//! or a log on the path, reading what the *client* volunteers. That reader does
//! not have to scan for anything — the request arrives at them.
//!
//! Everything sealing does is downstream of this. A request head that says which
//! program sent it identifies the sender to every party on the route, and no
//! amount of unlinkable addressing takes that back.
//!
//! The assertion is not "these exact headers", which would break every time a
//! dependency reorders one. It is that **no header name or value names this
//! program or the library it is built from**, and that every header sent is one
//! ordinary web traffic already carries.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use std::io::{BufRead as _, BufReader, Write as _};
use std::net::TcpListener;
use std::sync::mpsc;

use kusanagi_kernel::{DropAddr, Waypoint as _};
use kusanagi_waypoint::{Conditional as _, HttpWaypoint};

/// Words no request may contain, in the case a reader would match on.
const TELLS: [&str; 4] = ["kusanagi", "ureq", "rust", "drop"];

/// Header names ordinary web traffic already carries.
///
/// A name outside this list is not automatically wrong — it is a decision
/// somebody has to make deliberately, which is the point of listing them.
const ORDINARY: [&str; 7] = [
    "accept",
    "host",
    "content-type",
    "content-length",
    "if-none-match",
    "cache-control",
    "accept-encoding",
];

/// Takes one request, answers 404, and hands back the head it was sent.
fn overhear(request: impl FnOnce(&str) + Send + 'static) -> Vec<String> {
    let listener = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
    let port = listener.local_addr().expect("no local address").port();
    let (sender, receiver) = mpsc::channel();

    let listening = std::thread::spawn(move || {
        let (stream, _) = listener.accept().expect("nobody called");
        let mut reader = BufReader::new(stream);
        let mut head = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap_or(0) == 0 {
                break;
            }
            if line.trim_end().is_empty() {
                break;
            }
            head.push(line.trim_end().to_owned());
        }
        let answer = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
        reader
            .get_mut()
            .write_all(answer.as_bytes())
            .expect("could not answer");
        sender.send(head).expect("nobody wanted the head");
    });

    request(&format!("http://127.0.0.1:{port}"));
    let head = receiver
        .recv_timeout(std::time::Duration::from_secs(10))
        .expect("no request arrived");
    listening.join().ok();
    head
}

fn address() -> DropAddr {
    "0123456789abcdef0123456789abcdef01234567"
        .parse()
        .expect("that is an address")
}

#[test]
fn a_read_names_nothing_about_the_program_making_it() {
    let head = overhear(|base| {
        HttpWaypoint::new(base).get(&address()).ok();
    });
    judge(&head);
}

#[test]
fn a_write_names_nothing_about_the_program_making_it() {
    let head = overhear(|base| {
        HttpWaypoint::new(base)
            .put_if_absent(&address(), &[0_u8; 64])
            .ok();
    });
    judge(&head);
}

#[test]
fn asking_for_a_lifetime_does_not_name_this_project() {
    // The header this used to send was `X-Kusanagi-Ttl`, which put the product's
    // name in front of every proxy on the route. What replaced it is the header
    // a CDN sends.
    let head = overhear(|base| {
        HttpWaypoint::new(base)
            .put_with_ttl(&address(), &[0_u8; 64], 3_600)
            .ok();
    });
    judge(&head);
    let said = head.join("\n").to_lowercase();
    assert!(
        said.contains("cache-control: max-age=3600"),
        "the lifetime was not asked for in the ordinary way: {head:?}"
    );
}

/// Every rule this file has, applied to one request head.
fn judge(head: &[String]) {
    assert!(!head.is_empty(), "nothing was overheard");
    let said = head.join("\n").to_lowercase();
    for tell in TELLS {
        assert!(
            !said.contains(tell),
            "a request announced `{tell}` to whoever is on the path: {head:?}"
        );
    }

    for line in head.iter().skip(1) {
        let name = line
            .split_once(':')
            .map(|(name, _)| name.trim().to_lowercase())
            .unwrap_or_default();
        assert!(
            ORDINARY.contains(&name.as_str()),
            "a request carries the header `{name}`, which ordinary traffic does \
             not. Adding one is a decision, and this list is where it is made."
        );
    }
}

#[test]
fn what_a_request_actually_carries_is_written_down() {
    // Not an assertion about a list — a record of the whole head, so that a
    // dependency that starts sending something new is seen rather than inferred.
    let head = overhear(|base| {
        HttpWaypoint::new(base).get(&address()).ok();
    });
    let names: Vec<String> = head
        .iter()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| name.trim().to_lowercase())
        .collect();
    // Two headers, both of which anything speaking HTTP sends, and no agent
    // string at all. This is the whole of what a read announces.
    assert_eq!(
        names,
        vec!["accept", "host"],
        "the set of headers this client sends has changed: {head:?}"
    );
}
