// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Sending to several people at once, which here is sending to each of them.
//!
//! A group in this network is a list and nothing else: no group key, no roster
//! anybody else holds, no agreement about who may remove whom. Every property
//! below is a consequence of that, and together they are what says the design
//! was not quietly replaced by one with a shared secret in it.

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
use serde_json::Value;

/// One writer, three readers, each on a channel of its own.
struct Cohort {
    ground: std::path::PathBuf,
    alice: Endpoint,
    readers: Vec<Endpoint>,
}

/// Opens `names.len()` channels from alice, each with its own host directory.
///
/// A host per member, so that "one member's host is unreachable" is a state a
/// test can produce by deleting a directory rather than by mocking anything.
fn cohort(tag: &str, names: &[&str]) -> Cohort {
    let ground = scratch(tag);
    let alice = Endpoint::new(ground.join("alice"));
    let mut readers = Vec::new();
    for name in names {
        let waypoint = ground.join(format!("host-{name}"));
        std::fs::create_dir_all(&waypoint).unwrap();
        let invitation = invite_line(&alice, name, &waypoint.display().to_string());
        let reader = Endpoint::new(ground.join(name));
        reader
            .run(&Request::Join {
                invite: invitation,
                name: "alice".to_owned(),
            })
            .unwrap_or_else(|error| panic!("{name} could not join: {}", error.render(false)));
        readers.push(reader);
    }
    Cohort {
        ground,
        alice,
        readers,
    }
}

fn enrol(endpoint: &Endpoint, group: &str, members: &[&str]) -> Value {
    json(
        &endpoint
            .run(&Request::Group {
                name: group.to_owned(),
                members: members.iter().map(|name| (*name).to_owned()).collect(),
            })
            .unwrap_or_else(|error| panic!("the roster was refused: {}", error.render(false))),
    )
}

fn fanout(endpoint: &Endpoint, group: &str, text: &str) -> Value {
    json(
        &endpoint
            .run(&Request::Fanout {
                group: group.to_owned(),
                payload: text.as_bytes().to_vec(),
            })
            .unwrap_or_else(|error| panic!("the fan-out failed: {}", error.render(false))),
    )
}

/// What one member heard, in order.
fn heard(reader: &Endpoint) -> Vec<String> {
    let read = json(
        &reader
            .run(&Request::Read {
                name: "alice".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .unwrap_or_else(|error| panic!("a member could not read: {}", error.render(false))),
    );
    read["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["text"].as_str().unwrap().to_owned())
        .collect()
}

/// One send, three drops, three unrelated addresses, and no shared bytes.
#[test]
fn one_send_to_a_group_leaves_one_drop_for_each_member() {
    let cohort = cohort("group-fanout", &["bob", "carol", "dave"]);
    enrol(&cohort.alice, "team", &["bob", "carol", "dave"]);

    let before: Vec<usize> = ["bob", "carol", "dave"]
        .iter()
        .map(|name| stored(&cohort.ground.join(format!("host-{name}"))).len())
        .collect();

    let report = fanout(&cohort.alice, "team", "the standup is at ten");
    let delivered = report["delivered"].as_array().unwrap();
    assert_eq!(delivered.len(), 3);
    for row in delivered {
        assert_eq!(row["status"], "sent", "{row}");
    }

    // Each member's host gained exactly one object, and the three of them are
    // three different sets of bytes: five members are five keys, not one.
    let mut bodies = Vec::new();
    for (index, name) in ["bob", "carol", "dave"].iter().enumerate() {
        let after = stored(&cohort.ground.join(format!("host-{name}")));
        assert_eq!(after.len(), before[index] + 1, "{name} did not get a drop");
        bodies.extend(after.into_iter().map(|(_, body)| body));
    }
    let distinct: std::collections::HashSet<_> = bodies.iter().collect();
    assert_eq!(distinct.len(), bodies.len(), "two members share a drop");

    for reader in &cohort.readers {
        assert_eq!(heard(reader), vec!["the standup is at ten".to_owned()]);
    }

    std::fs::remove_dir_all(&cohort.ground).ok();
}

/// Removing somebody is writing the roster without them, and nothing else.
#[test]
fn a_member_taken_off_the_roster_gets_nothing_more() {
    let cohort = cohort("group-removal", &["bob", "carol"]);
    enrol(&cohort.alice, "team", &["bob", "carol"]);
    fanout(&cohort.alice, "team", "everybody hears this");

    enrol(&cohort.alice, "team", &["bob"]);
    let report = fanout(&cohort.alice, "team", "only bob hears this");
    assert_eq!(report["delivered"].as_array().unwrap().len(), 1);

    assert_eq!(
        heard(&cohort.readers[0]),
        vec![
            "everybody hears this".to_owned(),
            "only bob hears this".to_owned()
        ]
    );
    // Carol's channel is untouched: she was not told, nothing was revoked, and
    // what she already had is still there. Being taken off a list is not an
    // event on the wire.
    assert_eq!(
        heard(&cohort.readers[1]),
        vec!["everybody hears this".to_owned()]
    );

    // An empty roster is how a group is taken out of use, and it is legal.
    let emptied = enrol(&cohort.alice, "team", &[]);
    assert_eq!(emptied["group"]["members"].as_array().unwrap().len(), 0);
    let nothing = fanout(&cohort.alice, "team", "nobody is listening");
    assert_eq!(nothing["delivered"].as_array().unwrap().len(), 0);

    std::fs::remove_dir_all(&cohort.ground).ok();
}

/// One unreachable host is one row of the report, not the end of the send.
#[test]
fn a_member_whose_host_is_gone_does_not_stop_the_others() {
    let cohort = cohort("group-partial", &["bob", "carol", "dave"]);
    enrol(&cohort.alice, "team", &["bob", "carol", "dave"]);

    // Carol's host becomes a file where a directory was, which no waypoint can
    // be opened on. Deleting the directory would not do it: a directory waypoint
    // creates what is missing, exactly as a fresh host would.
    let carols = cohort.ground.join("host-carol");
    std::fs::remove_dir_all(&carols).unwrap();
    std::fs::write(&carols, b"not a directory").unwrap();

    let report = fanout(&cohort.alice, "team", "half of you will hear this");
    let delivered = report["delivered"].as_array().unwrap();
    assert_eq!(delivered.len(), 3);

    let refused: Vec<&Value> = delivered
        .iter()
        .filter(|row| row["status"] == "refused")
        .collect();
    assert_eq!(refused.len(), 1, "exactly one member should have failed");
    assert_eq!(refused[0]["member"], "carol");
    assert!(
        refused[0]["code"]
            .as_str()
            .unwrap()
            .starts_with("waypoint."),
        "a host that cannot be reached is a waypoint failure: {}",
        refused[0]
    );

    assert_eq!(
        heard(&cohort.readers[0]),
        vec!["half of you will hear this".to_owned()]
    );
    assert_eq!(
        heard(&cohort.readers[2]),
        vec!["half of you will hear this".to_owned()]
    );

    std::fs::remove_dir_all(&cohort.ground).ok();
}

/// A roster that names somebody who is not here is refused where it is written.
#[test]
fn a_roster_cannot_name_a_channel_this_endpoint_does_not_have() {
    let cohort = cohort("group-stranger", &["bob"]);

    let refused = cohort
        .alice
        .run(&Request::Group {
            name: "team".to_owned(),
            members: vec!["bob".to_owned(), "nobody".to_owned()],
        })
        .expect_err("a roster named a channel that does not exist");
    assert_eq!(refused.code(), "kusanagi.unknown_channel");

    // And a group nobody made is its own failure, apart from a missing channel:
    // the way out of one is an invitation, the way out of the other is a roster.
    let missing = cohort
        .alice
        .run(&Request::Fanout {
            group: "team".to_owned(),
            payload: b"anybody there".to_vec(),
        })
        .expect_err("a fan-out found a group that was never written");
    assert_eq!(missing.code(), "kusanagi.unknown_group");
    assert!(missing.render(true).contains("kusanagi group"));

    std::fs::remove_dir_all(&cohort.ground).ok();
}

/// `channels` is where a person finds out what a group stands for.
#[test]
fn the_listing_says_which_channels_a_group_stands_for() {
    let cohort = cohort("group-listing", &["bob", "carol"]);
    enrol(&cohort.alice, "team", &["bob", "carol"]);

    let listing = json(&cohort.alice.run(&Request::Channels).unwrap());
    let groups = listing["groups"].as_array().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0]["name"], "team");
    assert_eq!(groups[0]["members"].as_array().unwrap().len(), 2);

    std::fs::remove_dir_all(&cohort.ground).ok();
}
