// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Lies a host can tell while every byte it serves is genuine.
//!
//! `unlinkable.rs` asks what a host learns. This asks what a host can make
//! somebody believe. The difference matters because the lies here need no
//! forgery: the host holds real segments, really signed, and tells its lie by
//! choosing which ones to hand over and where.
//!
//! Found by `adversary/src/Kusanagi/Lying.hs`, which drives the shipped binary
//! and asserts that two readings stand in the right relation. Haskell searched
//! for it; this file is where the repository remembers it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, invite_line, json, scratch, stored};
use kusanagi::{Request, Whose};

/// Deletes whatever the host holds at `address`.
fn vanish(host: &std::path::Path, address: &str) {
    std::fs::remove_file(common::object_path(host, address)).expect("the host held nothing there");
}

#[test]
fn a_host_cannot_talk_a_reader_down_from_a_height_it_verified() {
    let ground = scratch("lying-rollback");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();
    alice.send("bob", "the offer stands");
    let withdrawn = alice.send_reporting("bob", "on second thoughts, it does not");

    let read = |after| {
        bob.run(&Request::Read {
            name: "alice".to_owned(),
            after,
            whose: Whose::Peer,
        })
    };

    // Bob reads both, and now knows something the host would like him to forget.
    assert_eq!(json(&read(None).unwrap())["height"], 1);

    // The host drops the retraction. Nothing is forged: what remains is one real
    // segment, correctly signed, forming a perfect chain of length one. A reader
    // starting from nothing would accept it without a murmur, which is exactly
    // why an endpoint that has read before must not.
    vanish(&host, &withdrawn);
    let refused = read(None).unwrap_err();
    assert_eq!(refused.code(), "kusanagi.history_changed");
    assert!(
        refused.render(false).contains("doctor"),
        "the way out does not name the command that measures the host: {}",
        refused.render(false)
    );

    // A floor below the deleted segment still has to fetch it, so it fails the
    // same way. That is not a weakness of the floor; it is the caller asking for
    // a segment the host is refusing to serve.
    assert_eq!(
        read(Some(0)).unwrap_err().code(),
        "kusanagi.history_changed"
    );

    // The poll an agent runs — a floor at the height it already holds — refuses
    // by construction rather than by comparison: it never looks below what it
    // verified, so there is nothing down there for the host to take away.
    let polled = json(&read(Some(1)).unwrap());
    assert_eq!(
        polled["height"], 1,
        "a poll lost the height bob had verified"
    );
    assert!(polled["segments"].as_array().unwrap().is_empty());

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn genuine_bytes_served_at_the_wrong_address_are_not_a_segment() {
    let ground = scratch("lying-transplant");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();
    let first = alice.send_reporting("bob", "one");
    let second = alice.send_reporting("bob", "two");

    // Serve the second segment from the first segment's address. Both objects
    // are this endpoint's own, minutes apart, same author, same shape. Only the
    // height is a lie, and no signature can see it — the address is what carries
    // the answer, because the key the bytes are sealed under is derived from it.
    let held: Vec<(String, Vec<u8>)> = stored(&host);
    let body = held
        .iter()
        .find(|(address, _)| *address == second)
        .map(|(_, bytes)| bytes.clone())
        .expect("the host is not holding the second segment");
    std::fs::write(common::object_path(&host, &first), body).unwrap();

    let refused = bob
        .run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap_err();
    // `seal.rejected`, not a chain or signature error: the bytes never became a
    // segment at all. Moving an object is answered one layer below verification,
    // by the key the address derives, which is why no check had to be written for
    // it and why removing that derivation would silently open this door.
    assert_eq!(
        refused.code(),
        "seal.rejected",
        "bytes moved to another height were not stopped by the key: {}",
        refused.render(false)
    );

    std::fs::remove_dir_all(&ground).ok();
}
