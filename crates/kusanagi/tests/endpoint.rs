// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Two endpoints, one untrusted host, and everything that has to hold between
//! them.
//!
//! These drive the library exactly as the binary does — same requests, same
//! outcomes — so what is checked here is the program, not a rehearsal of it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

mod common;

use common::{Endpoint, invite_line, json, scratch};
use kusanagi::Request;

#[test]
fn two_endpoints_exchange_messages_through_a_host_neither_of_them_runs() {
    let ground = scratch("exchange");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    // Alice opens a channel and hands over one line. That line is the entire
    // onboarding procedure: no account, no configuration file, no directory.
    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .expect("bob could not join");

    for line in ["the first thing alice says", "the second", "the third"] {
        alice.send("bob", line);
    }
    bob.send("alice", "bob heard you");

    // Each side reads the other's stream, verified end to end from genesis.
    let heard = json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
        })
        .expect("bob could not read alice"),
    );
    assert_eq!(heard["height"], 2);
    assert_eq!(heard["segments"][0]["text"], "the first thing alice says");
    assert_eq!(heard["segments"][2]["text"], "the third");

    let answered = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
            })
            .expect("alice could not read bob"),
    );
    assert_eq!(answered["height"], 0);
    assert_eq!(answered["segments"][0]["text"], "bob heard you");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn one_flipped_byte_on_the_host_is_caught() {
    let ground = scratch("tamper");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    let address = alice.send_reporting("bob", "a message that must arrive intact");

    // Bob can read it before the host interferes.
    assert!(
        bob.run(&Request::Read {
            name: "alice".to_owned()
        })
        .is_ok()
    );

    // The host flips one bit of the object it is holding. It cannot know what it
    // just broke, which is the point — but the reader finds out immediately.
    common::flip_one_byte(&host, &address);

    let complaint = bob
        .run(&Request::Read {
            name: "alice".to_owned(),
        })
        .expect_err("a tampered drop was accepted");
    assert_eq!(complaint.code(), "seal.rejected");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn revoking_a_peer_stops_their_messages_from_that_moment() {
    let ground = scratch("revoke");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    bob.send("alice", "something bob wrote while he was welcome");

    // Alice reads once, which is what teaches her who accepted the invitation.
    let heard = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
            })
            .unwrap(),
    );
    assert_eq!(
        heard["segments"][0]["text"],
        "something bob wrote while he was welcome"
    );

    let revoked = json(
        &alice
            .run(&Request::Revoke {
                name: "bob".to_owned(),
            })
            .expect("alice could not revoke bob"),
    );
    assert_eq!(revoked["command"], "revoked");

    // Everything below the revoked step is void immediately, including what was
    // already written and already read.
    let refused = alice
        .run(&Request::Read {
            name: "bob".to_owned(),
        })
        .expect_err("a revoked peer was still readable");
    assert_eq!(refused.code(), "grant.revoked");

    // Bob himself is unaffected locally — revocation is not a message — but
    // nothing he writes will be accepted again.
    bob.send("alice", "and something he wrote afterwards");
    assert_eq!(
        alice
            .run(&Request::Read {
                name: "bob".to_owned()
            })
            .unwrap_err()
            .code(),
        "grant.revoked"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_invitation_admits_exactly_one_endpoint() {
    let ground = scratch("one-shot");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));
    let mallory = Endpoint::new(ground.join("mallory"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation.clone(),
        name: "alice".to_owned(),
    })
    .unwrap();

    // The second acceptance is refused by the *host's* write-once rule, not by
    // any bookkeeping this program does.
    let refused = mallory
        .run(&Request::Join {
            invite: invitation,
            name: "alice".to_owned(),
        })
        .expect_err("an invitation was accepted twice");
    assert_eq!(refused.code(), "kusanagi.invite_spent");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_endpoint_with_only_read_cannot_send() {
    let ground = scratch("read-only");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let line = json(
        &alice
            .run(&Request::Invite {
                name: "bob".to_owned(),
                waypoint: host.display().to_string(),
                lifetime: 3_600,
                abilities: kusanagi_grant::Abilities::NONE.with(kusanagi_grant::Ability::Read),
            })
            .unwrap(),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned();

    bob.run(&Request::Join {
        invite: line,
        name: "alice".to_owned(),
    })
    .unwrap();

    let refused = bob
        .run(&Request::Send {
            name: "alice".to_owned(),
            text: "may I?".to_owned(),
        })
        .expect_err("an endpoint without `send` sent something");
    assert_eq!(refused.code(), "grant.forbidden");

    // and reading, which it was granted, still works
    alice.send("bob", "you may listen");
    assert!(
        bob.run(&Request::Read {
            name: "alice".to_owned()
        })
        .is_ok()
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_expired_invitation_is_refused() {
    let ground = scratch("expired");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = json(
        &alice
            .run(&Request::Invite {
                name: "bob".to_owned(),
                waypoint: host.display().to_string(),
                lifetime: 0,
                abilities: kusanagi_grant::Abilities::ALL,
            })
            .unwrap(),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned();

    let refused = bob
        .run(&Request::Join {
            invite: invitation,
            name: "alice".to_owned(),
        })
        .expect_err("an expired invitation was accepted");
    assert_eq!(refused.code(), "grant.expired");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_command_keeps_no_state_that_a_kill_could_lose() {
    let ground = scratch("stateless");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    alice.send("bob", "one");
    alice.send("bob", "two");

    // Everything alice knows about her own height came from the host, so an
    // endpoint rebuilt from nothing but its identity and channel files continues
    // the same chain rather than forking it.
    let rebuilt = Endpoint::new(alice.site_root().to_path_buf());
    rebuilt.send("bob", "three");

    let heard = json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
        })
        .unwrap(),
    );
    assert_eq!(heard["height"], 2);
    assert_eq!(heard["segments"][2]["text"], "three");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_channel_lists_itself_before_and_after_somebody_joins() {
    let ground = scratch("channels");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    let listed = json(&alice.run(&Request::Channels).unwrap());
    assert_eq!(listed["channels"][0]["name"], "bob");
    assert_eq!(listed["channels"][0]["standing"], "root");
    assert!(listed["channels"][0]["peer"].is_null());

    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    bob.send("alice", "here I am");
    alice
        .run(&Request::Read {
            name: "bob".to_owned(),
        })
        .unwrap();

    let listed = json(&alice.run(&Request::Channels).unwrap());
    assert!(listed["channels"][0]["peer"].is_string());

    let bobs = json(&bob.run(&Request::Channels).unwrap());
    assert_eq!(bobs["channels"][0]["standing"], "granted");

    std::fs::remove_dir_all(&ground).ok();
}
