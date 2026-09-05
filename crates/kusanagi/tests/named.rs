// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A name signed on a key: exchanged once at introduction, shown on this
//! program's lines and never inside the peer's (D-10, L1).

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, FENCE, invite_line, json, scratch, stored};
use kusanagi::{Naming, Request, Whose};

fn named(endpoint: &Endpoint, alias: &str) {
    endpoint
        .run(&Request::Name {
            naming: Naming::Set(alias.to_owned()),
        })
        .unwrap();
}

fn read(endpoint: &Endpoint, channel: &str) -> kusanagi::Outcome {
    endpoint
        .run(&Request::Read {
            name: channel.to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap()
}

/// Every line between an opening and a closing fence, joined.
fn fenced(prose: &str) -> String {
    let mut inside = false;
    let mut kept = Vec::new();
    for line in prose.lines() {
        if line == FENCE.opens() {
            inside = true;
        } else if line == FENCE.closes() {
            inside = false;
        } else if inside {
            kept.push(line);
        }
    }
    kept.join("\n")
}

#[test]
fn a_name_set_before_the_introduction_reaches_the_peer_and_stays_outside_the_fence() {
    let ground = scratch("named");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));
    named(&alice, "Alice");
    named(&bob, "Bob");

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();

    // Bob learned Alice's name from the offer; Alice learns Bob's from the
    // greeting the first time she reads.
    let bobs = json(&bob.run(&Request::Channels).unwrap());
    assert_eq!(bobs["channels"][0]["alias"], "Alice");
    assert_eq!(bobs["channels"][0]["peer"], "Alice");
    bob.send("alice", "Hello from Bob");
    let heard = read(&alice, "bob");
    let alices = json(&alice.run(&Request::Channels).unwrap());
    assert_eq!(alices["channels"][0]["alias"], "Bob");

    // Three fields, three facts: the channel's local name, the peer's signed
    // alias, and the handle every segment was verified against.
    let shape = json(&heard);
    assert_eq!(shape["name"], "bob");
    assert_eq!(shape["alias"], "Bob");
    assert_eq!(shape["author"], bob.handle());
    assert_eq!(shape["segments"][0]["text"], "Hello from Bob");

    // The prose says who spoke on its own header line, and the fence holds
    // only what Bob wrote: the alias never enters the peer's half.
    let prose = heard.render(false, FENCE);
    assert!(prose.lines().next().unwrap().contains("Bob"), "{prose}");
    assert_eq!(fenced(&prose), "Hello from Bob");

    // Nothing the host holds spells either name: the declaration rides sealed.
    for (key, bytes) in stored(&host) {
        for name in [b"Alice".as_slice(), b"Bob"] {
            assert!(
                !bytes.windows(name.len()).any(|window| window == name),
                "the host object {key} spells a name in the clear"
            );
        }
    }
    std::fs::remove_dir_all(&ground).ok();
}

/// A rename reaches channels opened afterwards and no earlier one: the name is
/// exchanged at introduction, which is the written limit of L1.
#[test]
fn a_later_rename_reaches_only_channels_opened_afterwards() {
    let ground = scratch("renamed");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));
    named(&alice, "Alice");
    let first = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: first,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();

    named(&alice, "Alicia");
    let second = invite_line(&alice, "bob-again", &host.display().to_string());
    bob.run(&Request::Join {
        invite: second,
        name: "alicia".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();

    let listed = json(&bob.run(&Request::Channels).unwrap());
    let alias_of = |channel: &str| {
        listed["channels"]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["name"] == channel)
            .map(|row| row["alias"].clone())
            .unwrap()
    };
    assert_eq!(alias_of("alice"), "Alice");
    assert_eq!(alias_of("alicia"), "Alicia");
    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_unfit_name_is_refused_where_it_was_typed_and_a_peer_without_one_is_abbreviated() {
    let ground = scratch("unfit");
    let alice = Endpoint::new(ground.join("alice"));
    for bad in ["", "Bob\nHello", "Bob\u{202E}", &"x".repeat(33)] {
        let refused = alice
            .run(&Request::Name {
                naming: Naming::Set(bad.to_owned()),
            })
            .unwrap_err();
        assert_eq!(refused.code(), "kusanagi.argument", "{bad:?}");
    }
    let asked = json(
        &alice
            .run(&Request::Name {
                naming: Naming::Ask,
            })
            .unwrap(),
    );
    assert!(asked["alias"].is_null());

    // Without a name, the one naming rule falls back to twelve characters of
    // the handle, on the listing and on the stream header alike.
    let (alice, bob, _host) = common::pair("unnamed");
    bob.send("alice", "hi");
    let heard = read(&alice, "bob");
    assert!(json(&heard)["alias"].is_null());
    let header = heard.render(false, FENCE);
    let short: String = bob.handle().chars().take(12).collect();
    assert!(header.lines().next().unwrap().contains(&short));
    let listed = json(&alice.run(&Request::Channels).unwrap());
    assert_eq!(listed["channels"][0]["peer"], short);
    std::fs::remove_dir_all(&ground).ok();
}
