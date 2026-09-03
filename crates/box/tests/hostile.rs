// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A caller who is trying it on, rather than one who made a mistake.
//!
//! A host answers strangers. Every rule it has is therefore a rule somebody will
//! attack directly, and the interesting attacks are the ones that go through the
//! protocol rather than around it: a header spelled almost right, a number large
//! enough to wrap, an address spelled two ways.
//!
//! Each test here corresponds to one thing that would be a real defect. None of
//! them found one — every rule already held — and they are committed because a
//! rule that holds by accident and a rule that holds on purpose look identical
//! until somebody changes the code.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{host, probe, status};

/// A well-formed address, in the one spelling the parser accepts.
const ADDRESS: &str = "0123456789abcdef0123456789abcdef01234567";

/// A request that writes `body` at `target` with these extra header lines.
fn put(target: &str, headers: &str, body: &str) -> String {
    format!(
        "PUT {target} HTTP/1.1\r\nHost: h\r\n{headers}Content-Length: {}\r\n\r\n{body}",
        body.len()
    )
}

#[test]
fn a_lifetime_too_large_to_add_does_not_take_the_host_down() {
    // `now + u64::MAX` is the arithmetic a stranger controls. Overflow-checks are
    // on in every profile this workspace builds, so a wrapping add here would be
    // a panic on the serving thread, reachable by anybody who can reach the port.
    let (address, root) = host("hostile-ttl", 2);

    let written = probe(
        &address,
        &put(
            &format!("/d/{ADDRESS}"),
            "If-None-Match: *\r\nCache-Control: max-age=18446744073709551615\r\n",
            "a segment",
        ),
    );
    assert_eq!(status(&written), 201, "a saturating lifetime was refused");

    // And the object is still there afterwards, which is what says the expiry
    // saturated at the end of time rather than wrapping round to before now.
    let read = probe(
        &address,
        &format!("GET /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\n\r\n"),
    );
    assert_eq!(
        status(&read),
        200,
        "an object with a saturating lifetime vanished"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn only_the_exact_conditional_header_gets_a_write() {
    // Every one of these is a plausible near-miss, and each would be a way to
    // overwrite a drop if the check were loose. `If-None-Match: "*"` is the
    // dangerous one: it is what a client library sends when it quotes an ETag
    // value automatically.
    let nearly = [
        "",
        "If-None-Match: \"*\"\r\n",
        "If-None-Match: **\r\n",
        "If-None-Match: W/*\r\n",
        "If-None-Match:\r\n",
        "If-Match: *\r\n",
    ];
    let (address, root) = host("hostile-conditional", nearly.len() + 1);

    for headers in nearly {
        let answer = probe(
            &address,
            &put(&format!("/d/{ADDRESS}"), headers, "sneaked in"),
        );
        assert_eq!(
            status(&answer),
            428,
            "a write with headers {headers:?} was not refused"
        );
    }

    // Nothing above stored anything, so the address is still free.
    let answer = probe(
        &address,
        &put(
            &format!("/d/{ADDRESS}"),
            "If-None-Match: *\r\n",
            "the real one",
        ),
    );
    assert_eq!(status(&answer), 201, "the near misses had claimed the drop");

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn an_address_has_exactly_one_spelling() {
    // If the hex parser took uppercase, `/d/ABC…` and `/d/abc…` would be two
    // names for one drop and write-once would be bypassable by re-spelling the
    // address. It does not, and this is what keeps it that way.
    let upper = ADDRESS.to_uppercase();
    let (address, root) = host("hostile-spelling", 3);

    let claimed = probe(
        &address,
        &put(
            &format!("/d/{ADDRESS}"),
            "If-None-Match: *\r\n",
            "the first",
        ),
    );
    assert_eq!(status(&claimed), 201);

    let again = probe(
        &address,
        &put(&format!("/d/{upper}"), "If-None-Match: *\r\n", "the second"),
    );
    assert_eq!(
        status(&again),
        404,
        "an address spelled in upper case was accepted as a second address"
    );

    let read = probe(
        &address,
        &format!("GET /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\n\r\n"),
    );
    assert!(
        String::from_utf8_lossy(&read).ends_with("the first"),
        "the drop was overwritten through a second spelling"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_body_larger_than_it_says_is_not_believed() {
    // The header claims more than the socket will carry, so `read_exact` waits
    // and then the connection is dropped. What must not happen is the host
    // allocating what it was told to and answering as though the body arrived.
    let (address, root) = host("hostile-length", 2);

    let answer = probe(
        &address,
        &format!(
            "PUT /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\nIf-None-Match: *\r\n\
             Content-Length: 1048577\r\n\r\nshort"
        ),
    );
    assert_eq!(
        status(&answer),
        400,
        "a body larger than this host accepts was not refused"
    );

    let read = probe(
        &address,
        &format!("GET /d/{ADDRESS} HTTP/1.1\r\nHost: h\r\n\r\n"),
    );
    assert_eq!(status(&read), 404, "the refused write stored something");

    std::fs::remove_dir_all(&root).ok();
}
