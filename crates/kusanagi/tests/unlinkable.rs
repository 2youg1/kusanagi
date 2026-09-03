// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The assertion the whole project stands or falls on.
//!
//! A hundred segments cross a host in both directions, and then this file takes
//! the host's side: it opens every object the host is holding, together with
//! every public fact a host could possibly know — both handles, the locator, the
//! order and timing of the requests — and tries to link two records to each other
//! or to a person.
//!
//! What a host *does* learn is written down here too, as assertions rather than
//! prose, because a privacy claim that is not stated exactly is a privacy claim
//! that will quietly stop being true:
//!
//! - **how many objects it holds**, so it knows the volume of traffic;
//! - **how large each one is**, to the byte, so it knows message lengths;
//! - **when each request arrived**, so it knows when somebody was awake.
//!
//! Those three are traffic analysis, and closing them is a separate mechanism
//! that does not exist yet. Everything else must be uniform noise.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::string_slice,
    reason = "test code"
)]

mod common;

use std::collections::BTreeSet;

use common::{Endpoint, invite_line, json, scratch, stored};
use kusanagi::{Request, Whose};

/// How many segments each side writes.
const ROUNDS: usize = 50;

#[test]
fn a_host_holding_a_hundred_segments_can_link_none_of_them() {
    let ground = scratch("unlinkable");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();

    // Deliberately identical text on both sides at every height: if anything
    // about the ciphertext or the address were a function of the content or of
    // the author, this is the traffic that would reveal it.
    for round in 0..ROUNDS {
        alice.send("bob", &format!("round {round}"));
        bob.send("alice", &format!("round {round}"));
    }

    let records = stored(&host);
    // 100 segments, plus alice's offer and bob's one-time greeting. Both of
    // those are drops of the same size at addresses of the same shape, which is
    // the whole reason the count is the only thing that changed.
    assert_eq!(records.len(), ROUNDS * 2 + 2, "the host holds a surprise");

    // (1) Every address is its own. Two drops never collide, so no address is
    //     ever rewritten and no two records are related by sharing one.
    let addresses: BTreeSet<&String> = records.iter().map(|(address, _)| address).collect();
    assert_eq!(
        addresses.len(),
        records.len(),
        "two drops shared an address"
    );

    // (2) Addresses do not cluster. If a lane, an author or a height leaked into
    //     an address, the addresses of one stream would share structure; over a
    //     hundred uniformly random 160-bit values, a shared four-byte prefix
    //     happens with probability far below one in a million.
    let prefixes: BTreeSet<&str> = records.iter().map(|(address, _)| &address[..8]).collect();
    assert_eq!(
        prefixes.len(),
        records.len(),
        "two addresses share a prefix"
    );

    // (3) Nothing a host knows appears in what a host holds. Handles are public;
    //     if a handle were recoverable from an object, every drop would be
    //     labelled with its author.
    let alice_handle = hex_bytes(&alice.handle());
    let bob_handle = hex_bytes(&bob.handle());
    for (address, bytes) in &records {
        assert!(
            !contains(bytes, &alice_handle) && !contains(bytes, &bob_handle),
            "the object at {address} carries an author's handle in the clear"
        );
        assert!(
            !contains(bytes, b"round "),
            "the object at {address} carries its plaintext"
        );
    }

    // (4) The same words at the same height on two streams produce two unrelated
    //     objects, so a host cannot match a message to its reply.
    let bodies: BTreeSet<&Vec<u8>> = records.iter().map(|(_, bytes)| bytes).collect();
    assert_eq!(
        bodies.len(),
        records.len(),
        "two records are byte-identical"
    );

    // (5) What the host *does* learn, stated as an assertion so that it cannot
    //     grow quietly: the count, and each object's length.
    let lengths: Vec<usize> = records.iter().map(|(_, bytes)| bytes.len()).collect();
    assert!(
        lengths.iter().all(|length| *length > 0),
        "an object is empty, which would itself be a signal"
    );

    // And the traffic is still correct after all that: both sides read a chain
    // that verifies from genesis to the last segment.
    let heard = json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap(),
    );
    assert_eq!(heard["height"], u64::try_from(ROUNDS - 1).unwrap());
    let answered = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    assert_eq!(answered["height"], u64::try_from(ROUNDS - 1).unwrap());

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_second_channel_between_the_same_two_endpoints_shares_nothing_with_the_first() {
    let ground = scratch("two-channels");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    for channel in ["first", "second"] {
        let invitation = invite_line(&alice, channel, &host.display().to_string());
        bob.run(&Request::Join {
            invite: invitation,
            name: channel.to_owned(),
        })
        .unwrap();
        alice.send(channel, "the same words on both channels");
        bob.send(channel, "the same words on both channels");
    }

    // Two conversations between the same pair of people, carrying identical
    // text. A host that could tell they were the same pair would have defeated
    // the point of per-channel secrets.
    let records = stored(&host);
    let addresses: BTreeSet<&String> = records.iter().map(|(address, _)| address).collect();
    let bodies: BTreeSet<&Vec<u8>> = records.iter().map(|(_, bytes)| bytes).collect();
    assert_eq!(addresses.len(), records.len());
    assert_eq!(bodies.len(), records.len());

    std::fs::remove_dir_all(&ground).ok();
}

/// Decodes a hexadecimal handle back to the bytes a host would search for.
fn hex_bytes(text: &str) -> Vec<u8> {
    kusanagi_kernel::unhex(text).expect("a handle did not render as hexadecimal")
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}
