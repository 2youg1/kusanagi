// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A trace the adversary found, kept here so this repository remembers it.
//!
//! Written by `adversary/src/Kusanagi/Regression.hs` and compared against it
//! byte for byte. Change the trace there; changing it here turns the adversary
//! red, which is exactly what should happen when the two disagree.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, json, scratch};
use kusanagi::{Request, Whose};
use kusanagi_grant::Abilities;

#[test]
fn an_endpoint_cannot_accept_its_own_invitation() {
    let ground = scratch("an_endpoint_cannot_accept_its_own_invitation");
    let host = ground.join("host").display().to_string();
    let alice = Endpoint::new(ground.join("alice"));

    let invitation1 = json(
        &alice
            .run(&Request::Invite {
                name: "one".to_owned(),
                waypoint: host.clone(),
                lifetime: 3600,
                abilities: Abilities::ALL,
                habit: kusanagi::Habit::default(),
            })
            .expect("the invitation was refused"),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned();

    let refused2 = alice
        .run(&Request::Join {
            invite: invitation1.clone(),
            name: "two".to_owned(),
            habit: kusanagi::Habit::default(),
        })
        .unwrap_err();
    assert_eq!(refused2.code(), "kusanagi.own_invitation");

    alice
        .run(&Request::Send {
            name: "one".to_owned(),
            payload: b"beta".to_vec(),
        })
        .expect("the segment was refused");

    let refused4 = alice
        .run(&Request::Read {
            name: "one".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap_err();
    assert_eq!(refused4.code(), "kusanagi.no_peer_yet");

    std::fs::remove_dir_all(&ground).ok();
}
