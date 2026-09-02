// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a program puts in, a program gets back.
//!
//! The reader on the other side of this door is usually an agent, and an agent
//! that cannot recover the exact bytes it sent has no channel — it has a
//! rumour. These two tests are the difference.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, invite_line, json, scratch};
use kusanagi::{Request, Whose};

#[test]
fn a_payload_that_is_not_text_survives_the_round_trip() {
    let ground = scratch("payload");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();

    // A lone 0xff is not valid UTF-8, and a NUL is not something a shell will
    // carry. Both are ordinary bytes to a segment.
    let sent = vec![0xff, 0x00, 0xfe, b'h', b'i'];
    bob.run(&Request::Send {
        name: "alice".to_owned(),
        payload: sent.clone(),
    })
    .expect("bytes that are not text were refused");

    let heard = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .expect("alice could not read bob"),
    );

    // The lossless field is exact.
    assert_eq!(heard["segments"][0]["payload"], "ff00fe6869");
    // The readable one is not, and says so by differing.
    assert_ne!(heard["segments"][0]["text"], "\u{ff}\u{0}\u{fe}hi");
}

#[test]
fn reading_after_a_height_reports_only_what_follows() {
    let ground = scratch("after");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    for line in ["one", "two", "three"] {
        bob.send("alice", line);
    }

    let everything = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    assert_eq!(everything["segments"].as_array().unwrap().len(), 3);

    let rest = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: Some(0),
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    // Two segments follow height 0 …
    assert_eq!(rest["segments"].as_array().unwrap().len(), 2);
    assert_eq!(rest["segments"][0]["text"], "two");
    // … and the verified head is still reported in full, which is what makes
    // one call enough to answer "is there anything new".
    assert_eq!(rest["height"], 2);

    let nothing = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: Some(2),
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    assert_eq!(nothing["segments"].as_array().unwrap().len(), 0);
    assert_eq!(nothing["height"], 2);

    std::fs::remove_dir_all(&ground).ok();
}
