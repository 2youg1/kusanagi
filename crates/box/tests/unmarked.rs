// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What somebody scanning the internet learns from a box.
//!
//! Every other test in this crate takes the side of a caller who holds an
//! address. This one takes the side of somebody who holds nothing and is asking
//! every host on a port range the same handful of questions — which is how a
//! network of privacy tools is turned into a list of its users, and it costs the
//! adversary one request per address.
//!
//! Two properties, and the second is the one with teeth:
//!
//! 1. **No answer names this program.** Not a banner, not a version, not a
//!    reason, not a header.
//! 2. **A caller without an address cannot recover the address grammar.** Every
//!    question that is not about a drop somebody already knows gets a byte-for-
//!    byte identical answer — including a well-formed address that is simply not
//!    there. If a bad path and an empty drop answered differently, a scanner
//!    would learn the shape `/d/<40 hex>` from the difference and would be able
//!    to tell this host from any other server on the same port.
//!
//! Both are asserted against the raw socket rather than through `HttpWaypoint`,
//! because the client is not the thing under suspicion here.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{host, probe};

/// Words that would give the game away, in the case a scanner would try first.
const TELLS: [&str; 6] = ["kusanagi", "drop", "segment", "box", "write-once", "expiry"];

/// The questions a scanner asks, in the order a scanner asks them.
fn questions() -> Vec<&'static str> {
    vec![
        "GET / HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET /health HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET /status HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET /index.html HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET /.well-known/security.txt HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET /d/ HTTP/1.1\r\nHost: h\r\n\r\n",
        "GET /d/not-an-address HTTP/1.1\r\nHost: h\r\n\r\n",
        // A well-formed address that nothing was ever written to.
        "GET /d/0123456789abcdef0123456789abcdef01234567 HTTP/1.1\r\nHost: h\r\n\r\n",
        "POST /d/0123456789abcdef0123456789abcdef01234567 HTTP/1.1\r\nHost: h\r\n\r\n",
        "DELETE /d/0123456789abcdef0123456789abcdef01234567 HTTP/1.1\r\nHost: h\r\n\r\n",
        "OPTIONS * HTTP/1.1\r\nHost: h\r\n\r\n",
        "HEAD / HTTP/1.1\r\nHost: h\r\n\r\n",
    ]
}

#[test]
fn no_answer_this_host_gives_names_the_program_giving_it() {
    let asked = questions();
    let (address, root) = host("unmarked-tells", asked.len());

    for question in &asked {
        let answer = String::from_utf8_lossy(&probe(&address, question)).to_lowercase();
        for tell in TELLS {
            assert!(
                !answer.contains(tell),
                "a host answered `{}` with the word `{tell}`: {answer:?}",
                question.lines().next().unwrap_or_default()
            );
        }
    }

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn a_caller_without_an_address_cannot_find_the_address_grammar() {
    let asked = questions();
    let (address, root) = host("unmarked-grammar", asked.len());

    let mut answers = asked.iter().map(|question| probe(&address, question));
    let first = answers.next().expect("there is at least one question");
    for (question, answer) in asked.iter().skip(1).zip(answers) {
        assert_eq!(
            answer,
            first,
            "a host answered `{}` differently from `{}`; the difference is what \
             tells a scanner that this is not an ordinary server",
            question.lines().next().unwrap_or_default(),
            asked[0].lines().next().unwrap_or_default()
        );
    }

    // And that one answer is the answer an ordinary static server gives.
    let said = String::from_utf8_lossy(&first);
    assert!(said.starts_with("HTTP/1.1 404 Not Found\r\n"), "{said:?}");
    assert!(said.contains("Content-Length: 0\r\n"), "{said:?}");

    std::fs::remove_dir_all(&root).ok();
}
