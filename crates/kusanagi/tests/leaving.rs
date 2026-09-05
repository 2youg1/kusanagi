// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Three questions an endpoint has to be able to answer about itself.
//!
//! *What did I write?* — after a process is killed, the height of one's own
//! stream is a fact on the host, and asking for it must not require writing
//! another segment to find out.
//!
//! *What may I still do here?* — expiry and revocation are local facts, so a
//! listing that reports them costs nothing and saves a caller from learning the
//! answer as a failure.
//!
//! *How do I leave?* — a channel whose peer is the root authority cannot be
//! revoked, and until `forget` existed the recovery text for that failure named
//! an action this program did not have.

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

/// Opens `host`, admits bob, and returns both endpoints.
fn pair(tag: &str) -> (std::path::PathBuf, Endpoint, Endpoint) {
    let ground = scratch(tag);
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .expect("bob could not join");
    (ground, alice, bob)
}

#[test]
fn an_endpoint_can_read_its_own_stream_back_without_writing_to_it() {
    let (ground, alice, _bob) = pair("mine");

    for line in ["one", "two", "three"] {
        alice.send("bob", line);
    }

    // This is the recovery an interrupted program performs: it asks the host
    // how far it got, rather than appending a segment to discover the height.
    let mine = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Mine,
            })
            .expect("alice could not read her own stream"),
    );
    assert_eq!(mine["height"], 2);
    assert_eq!(mine["author"], alice.handle());
    assert_eq!(mine["segments"][2]["text"], "three");

    // The same request as a poller makes it: only what follows a known height,
    // with the head still reported.
    let tail = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: Some(1),
                whose: Whose::Mine,
            })
            .unwrap(),
    );
    assert_eq!(tail["height"], 2);
    assert_eq!(tail["segments"].as_array().unwrap().len(), 1);
    assert_eq!(tail["segments"][0]["index"], 2);

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn reading_ones_own_stream_still_works_after_being_cut_off() {
    let (ground, alice, bob) = pair("mine-revoked");
    bob.send("alice", "before the axe");
    alice
        .run(&Request::Read {
            name: "bob".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .expect("alice could not meet bob");
    alice
        .run(&Request::Revoke {
            name: "bob".to_owned(),
        })
        .expect("alice could not revoke bob");

    // Bob's own writes keep succeeding, and that is the design rather than a
    // hole in it: nobody can tell him he was cut off, because telling him would
    // need a channel he is no longer on. Enforcement is on the reader.
    bob.run(&Request::Send {
        name: "alice".to_owned(),
        payload: b"after the axe".to_vec(),
    })
    .expect("a revoked endpoint could not write into a stream nobody reads");
    let refused = alice
        .run(&Request::Read {
            name: "bob".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .expect_err("a revoked peer's stream was still accepted");
    assert_eq!(refused.code(), "grant.revoked");

    // What bob wrote is still bob's, and his own key still opens it. A check
    // here would refuse nothing: the bytes are his, at addresses he derives.
    let own = json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Mine,
        })
        .expect("a revoked endpoint could not read its own stream"),
    );
    assert_eq!(own["height"], 1);
    assert_eq!(own["author"], bob.handle());
    assert_eq!(own["segments"][0]["text"], "before the axe");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn the_listing_says_what_each_channel_still_permits() {
    let (ground, alice, bob) = pair("listing");
    bob.send("alice", "hello");
    alice
        .run(&Request::Read {
            name: "bob".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap();

    // Alice holds the root authority: nothing issued it, so nothing expires it.
    let hers = json(&alice.run(&Request::Channels).unwrap());
    assert_eq!(hers["channels"][0]["standing"], "root");
    assert_eq!(hers["channels"][0]["can"][0], "send");
    assert!(hers["channels"][0]["expires_at"].is_null());
    assert!(hers["channels"][0]["refused"].is_null());

    // Bob holds a grant: it says what he may do, and until when.
    let his = json(&bob.run(&Request::Channels).unwrap());
    assert_eq!(his["channels"][0]["standing"], "granted");
    assert_eq!(his["channels"][0]["can"][1], "read");
    assert!(his["channels"][0]["expires_at"].is_number());
    assert!(his["channels"][0]["refused"].is_null());

    // Before the axe, alice's listing says nothing is wrong with her peer.
    assert!(hers["channels"][0]["peer_refused"].is_null());

    // After it, her own listing says the peer is cut off — no request leaves
    // the machine to find that out, and no command has to fail first.
    alice
        .run(&Request::Revoke {
            name: "bob".to_owned(),
        })
        .unwrap();
    let after = json(&alice.run(&Request::Channels).unwrap());
    assert_eq!(after["channels"][0]["peer_refused"], "grant.revoked");

    // Bob's side cannot know, and says so by saying nothing: his grant still
    // verifies against his own revocation list, which is empty.
    let his_after = json(&bob.run(&Request::Channels).unwrap());
    assert_eq!(his_after["channels"][0]["can"][0], "send");
    assert!(his_after["channels"][0]["refused"].is_null());

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn forgetting_a_channel_frees_its_name_and_keeps_the_revocation() {
    let (ground, alice, bob) = pair("forget");
    bob.send("alice", "something alice will stop reading");
    alice
        .run(&Request::Read {
            name: "bob".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap();
    alice
        .run(&Request::Revoke {
            name: "bob".to_owned(),
        })
        .unwrap();

    let gone = json(
        &alice
            .run(&Request::Forget {
                name: "bob".to_owned(),
            })
            .expect("alice could not forget the channel"),
    );
    assert_eq!(gone["command"], "forgotten");
    assert_eq!(gone["name"], "bob");

    // It is not listed, and asking for it again fails like anything absent.
    let listed = json(&alice.run(&Request::Channels).unwrap());
    assert!(listed["channels"].as_array().unwrap().is_empty());
    assert_eq!(
        alice
            .run(&Request::Forget {
                name: "bob".to_owned(),
            })
            .unwrap_err()
            .code(),
        "kusanagi.unknown_channel"
    );

    // The name is free again — but the revocation outlives the record, or a
    // second invitation under the same name would bring a dead grant back.
    // Read through the site rather than off the disk: on Windows the file is a
    // DPAPI blob, which is what `at_rest.rs` is for and what this must not
    // depend on.
    let revoked = kusanagi::Site::at(alice.site_root()).revocations().unwrap();
    assert_eq!(revoked.len(), 1);
    let second = invite_line(&alice, "bob", &ground.join("host").display().to_string());
    assert!(second.starts_with("kusanagi2:"));

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn the_way_out_of_an_unrevokable_channel_is_a_command_that_exists() {
    let (ground, _alice, bob) = pair("root-peer");

    // Bob's peer is the authority that invited him; there is nothing above it
    // to revoke, and the failure says so.
    let refused = bob
        .run(&Request::Revoke {
            name: "alice".to_owned(),
        })
        .expect_err("bob revoked a root authority");
    assert_eq!(refused.code(), "kusanagi.cannot_revoke_root");
    assert!(refused.render(false).contains("kusanagi forget"));

    // And the command it names does what it says.
    bob.run(&Request::Forget {
        name: "alice".to_owned(),
    })
    .expect("the recovery command did not work");
    assert!(
        json(&bob.run(&Request::Channels).unwrap())["channels"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// Found by `adversary/` (Model, "what one endpoint says is what the other
/// hears"): the check that the peer may still read ran before the greeting
/// that discovers the peer, so the first send after a join skipped it.
#[test]
fn the_first_send_to_a_peer_who_may_not_read_is_refused() {
    let ground = scratch("first-send-checks-the-peer");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let line = json(
        &alice
            .run(&Request::Invite {
                name: "bob".to_owned(),
                waypoint: host.display().to_string(),
                lifetime: 3_600,
                abilities: kusanagi_grant::Abilities::NONE,
                habit: kusanagi::Habit::default(),
            })
            .unwrap(),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned();
    bob.run(&Request::Join {
        invite: line,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();

    let refused = alice
        .run(&Request::Send {
            name: "bob".to_owned(),
            payload: b"for nobody".to_vec(),
        })
        .unwrap_err();
    assert_eq!(refused.code(), "grant.forbidden");
    assert_eq!(
        common::drops(&host),
        2,
        "a drop was written for a peer who may not read it"
    );

    std::fs::remove_dir_all(&ground).ok();
}
