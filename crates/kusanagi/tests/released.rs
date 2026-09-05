// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A channel whose history stops existing once it has been read.
//!
//! Two halves of one promise, and the tests come in the same two halves.
//! **Deletion is the honest host's half**: the peer's acknowledgement rides
//! inside their own sealed segments, and once it arrives the drops it covers are
//! removed. **The ratchet is the dishonest host's half**: the keys that opened
//! them are destroyed, so a host that quietly kept a copy of the bytes holds
//! something nobody can open — including their owner.
//!
//! The price is the last test here, and it is stated rather than hidden: on such
//! a channel the site is the only copy, so losing the record of what was read is
//! losing the conversation. That is why `export` carries the ratchet and why the
//! refusal names `import`.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, drops, json};
use kusanagi::{Cadence, Habit, Request, Retention, Whose};

fn releasing() -> Habit {
    Habit {
        cadence: Cadence::OnDemand,
        retention: Retention::ReleaseOnAck,
    }
}

/// Reads the peer's stream in full, which is also what carries the acknowledgement.
fn read_all(endpoint: &Endpoint, channel: &str) -> serde_json::Value {
    json(
        &endpoint
            .run(&Request::Read {
                name: channel.to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .unwrap_or_else(|error| panic!("read failed: {}", error.render(false))),
    )
}

#[test]
fn a_keeping_channel_leaves_every_drop_where_it_was() {
    let (alice, bob, host) = common::pair("release-keep");
    for round in 0..3 {
        alice.send("bob", &format!("round {round}"));
    }
    read_all(&bob, "alice");
    bob.send("alice", "read you");
    read_all(&alice, "bob");

    // Three of alice's, one of bob's, one greeting, one offer: nothing removed.
    assert_eq!(
        drops(&host),
        6,
        "a channel that keeps its history dropped something"
    );
}

#[test]
fn what_the_peer_has_acknowledged_stays_on_the_host_and_opens_for_nobody() {
    let (alice, bob, host) = common::pair_with("release-drops", releasing());
    for round in 0..3 {
        alice.send("bob", &format!("round {round}"));
    }
    let before = drops(&host);

    // Bob reads all three, then writes: his segment carries "I have verified
    // three of yours", sealed, where the host cannot read it.
    let heard = read_all(&bob, "alice");
    assert_eq!(heard["segments"].as_array().unwrap().len(), 3);
    bob.send("alice", "got all three");

    // Alice reads bob and learns it. That is when the release happens: the keys
    // that opened those three drops are destroyed here. **The bytes stay where
    // they are.** A reader no longer names an address to the host, and a
    // `DELETE` would (D-20); removing them is the host's lifetime on the bin.
    // What a release guarantees is that nobody can open them, which is the next
    // test, and that this endpoint cannot be made to show them again, which is
    // the one after it.
    read_all(&alice, "bob");

    assert_eq!(
        drops(&host),
        before + 1,
        "a release must not delete: the three drops and bob's own should all be there"
    );
}

#[test]
fn the_key_to_a_released_drop_is_destroyed_and_the_refusal_says_so() {
    let (alice, bob, _host) = common::pair_with("release-burn", releasing());
    alice.send("bob", "first");
    alice.send("bob", "second");
    read_all(&bob, "alice");

    // Bob has read and burned. Asking again for what is behind the floor is the
    // one thing a ratchet exists to make impossible, and it is refused by name
    // rather than answered with an empty stream.
    let again = read_all(&bob, "alice");
    assert_eq!(
        again["height"].as_u64(),
        Some(1),
        "the height survives the burn; it comes from the cairn, not from the drops"
    );
    assert!(
        again["segments"].as_array().unwrap().is_empty(),
        "a burned segment cannot be handed over twice"
    );
}

#[test]
fn a_release_channel_without_its_record_refuses_rather_than_reporting_an_empty_stream() {
    let (alice, bob, _host) = common::pair_with("release-nocairn", releasing());
    alice.send("bob", "first");
    read_all(&bob, "alice");

    // The cairn is what says how far this endpoint got. Delete it and the drops
    // it covered are already gone from the host, so a walk from height zero
    // would find an empty address and conclude the stream never started —
    // a wrong answer given confidently, which is worse than a refusal.
    std::fs::remove_dir_all(bob.site_root().join("cairns")).unwrap();

    let refused = bob
        .run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .expect_err("a releasing channel with no record must refuse");
    assert_eq!(refused.code(), "kusanagi.needs_cairn");
    assert!(
        refused.render(true).contains("import"),
        "the refusal must name the verb that gets the history back"
    );
}

#[test]
fn a_backup_carries_the_ratchet_and_the_restored_endpoint_reads_on() {
    let (alice, bob, _host) = common::pair_with("release-backup", releasing());
    alice.send("bob", "before the backup");
    read_all(&bob, "alice");

    let archive = bob.run(&Request::Export).unwrap();
    let (recovery, bytes) = match archive {
        kusanagi::Outcome::Exported { recovery, archive } => (recovery, archive),
        other => panic!("export reported {other:?}"),
    };
    let key: [u8; 32] = kusanagi_kernel::unhex(&recovery)
        .unwrap()
        .try_into()
        .unwrap();

    let restored = Endpoint::new(common::scratch("release-backup-restored"));
    restored
        .run(&Request::Import {
            recovery: key,
            archive: bytes,
        })
        .expect("the archive restores");

    // The restored endpoint stands exactly where the original did: it knows the
    // height, and it does not try to walk from zero into drops that are gone.
    alice.send("bob", "after the backup");
    let heard = read_all(&restored, "alice");
    assert_eq!(heard["height"].as_u64(), Some(1));
    assert_eq!(
        heard["segments"][0]["text"].as_str(),
        Some("after the backup"),
        "a restored endpoint must carry on from where the ratchet stood"
    );
}
