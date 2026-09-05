// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A room: one ward swept once, every member's stream verified.
//!
//! Three endpoints share one host and one room. Each writes once; each reads
//! once and hears the other two. Nothing here names an address: the ward is
//! shared, the sweep takes the whole bin, and matching happens on this machine
//! — so a read of three members lists the host exactly as often as a read of
//! one.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use std::collections::BTreeMap;

use common::watching::Watching;
use common::{Endpoint, json, scratch};
use kusanagi::{Lane, Reach, Request, Site, SystemClock, track_all};
use kusanagi_kernel::{Bin, Clock as _, VerifyingKey};
use kusanagi_seal::{Keyring, period};

fn trio(tag: &str) -> (Endpoint, Endpoint, Endpoint, std::path::PathBuf) {
    let ground = scratch(tag);
    let host = ground.join("host");
    std::fs::create_dir_all(&host).unwrap();
    (
        Endpoint::new(ground.join("alice")),
        Endpoint::new(ground.join("bob")),
        Endpoint::new(ground.join("carol")),
        host,
    )
}

fn found(endpoint: &Endpoint, room: &str, host: &str) {
    endpoint
        .run(&Request::Room {
            name: room.to_owned(),
            waypoint: host.to_owned(),
        })
        .unwrap_or_else(|error| panic!("a room was not founded: {}", error.render(false)));
}

fn admit(founder: &Endpoint, newcomer: &Endpoint, room: &str) {
    let invitation = json(
        &founder
            .run(&Request::RoomInvite {
                name: room.to_owned(),
                lifetime: 3_600,
            })
            .unwrap(),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned();
    newcomer
        .run(&Request::RoomJoin {
            invite: invitation,
            name: room.to_owned(),
        })
        .unwrap_or_else(|error| panic!("could not join the room: {}", error.render(false)));
}

fn say(endpoint: &Endpoint, room: &str, text: &str) {
    endpoint
        .run(&Request::RoomSend {
            name: room.to_owned(),
            payload: text.as_bytes().to_vec(),
        })
        .unwrap_or_else(|error| panic!("a room send failed: {}", error.render(false)));
}

fn read(endpoint: &Endpoint, room: &str, after: BTreeMap<String, u64>) -> serde_json::Value {
    json(
        &endpoint
            .run(&Request::RoomRead {
                name: room.to_owned(),
                after,
            })
            .unwrap_or_else(|error| panic!("a room read failed: {}", error.render(false))),
    )
}

fn texts(read: &serde_json::Value) -> Vec<String> {
    let mut said: Vec<String> = read["threads"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|thread| thread["segments"].as_array().unwrap().iter())
        .map(|entry| entry["text"].as_str().unwrap().to_owned())
        .collect();
    said.sort();
    said
}

#[test]
fn three_members_each_write_once_and_each_read_hears_the_other_two() {
    let (alice, bob, carol, host) = trio("room-trio");
    let waypoint = host.display().to_string();
    found(&alice, "team", &waypoint);
    admit(&alice, &bob, "team");
    admit(&alice, &carol, "team");
    // The founder's read admits both and carries the roster on her stream, so
    // bob and carol learn of each other from it rather than from her disk.
    read(&alice, "team", BTreeMap::new());

    say(&alice, "team", "alice here");
    say(&bob, "team", "bob here");
    say(&carol, "team", "carol here");

    for endpoint in [&alice, &bob, &carol] {
        let heard = read(endpoint, "team", BTreeMap::new());
        assert_eq!(
            heard["threads"].as_array().unwrap().len(),
            3,
            "a room reads every member, not one stream"
        );
        assert_eq!(texts(&heard), vec!["alice here", "bob here", "carol here"]);
    }

    // `after` is per author: holding bob's height hides bob's line and nobody
    // else's, because one height means nothing across three streams.
    let floors = BTreeMap::from([(bob.handle(), 0)]);
    assert_eq!(
        texts(&read(&carol, "team", floors)),
        vec!["alice here", "carol here"]
    );
}

#[test]
fn a_read_of_three_members_lists_the_host_as_often_as_a_read_of_one() {
    let (alice, bob, carol, host) = trio("room-cost");
    let waypoint = host.display().to_string();
    found(&alice, "team", &waypoint);
    admit(&alice, &bob, "team");
    admit(&alice, &carol, "team");
    read(&alice, "team", BTreeMap::new());
    say(&alice, "team", "one");
    say(&bob, "team", "two");
    say(&carol, "team", "three");

    // Carol's record holds the roster her invitation carried; her first read
    // replaces it with the one alice signed after admitting everybody.
    read(&carol, "team", BTreeMap::new());
    let site = Site::at(carol.site_root());
    let room = site.room("team").unwrap();
    assert_eq!(room.roster.members().len(), 3);
    let now = SystemClock.now();
    let lanes: Vec<Lane> = room
        .roster
        .members()
        .iter()
        .map(|member: &VerifyingKey| Lane {
            keys: Keyring::Standing(room.secret.stream(&member.handle())),
            author: *member,
            bin: Bin::new(period(now.as_unix_seconds()), room.ward),
            opened: room.opened,
        })
        .collect();
    let watching = Watching::new(&host);

    let one: Vec<(&Lane, Reach)> = vec![(&lanes[0], Reach::Whole)];
    track_all(&site, "team", &watching, &one, now).unwrap();
    let alone = watching.lists().len();
    assert!(alone >= 1);

    watching.forget();
    let all: Vec<(&Lane, Reach)> = lanes.iter().map(|lane| (lane, Reach::Whole)).collect();
    let walked = track_all(&site, "team", &watching, &all, now).unwrap();
    assert_eq!(walked.len(), 3);
    assert!(walked.iter().all(|done| done.head().is_some()));
    assert_eq!(
        watching.lists().len(),
        alone,
        "three lanes of one ward cost more listings than one lane"
    );
}

#[test]
fn only_the_founder_invites_and_a_room_name_is_a_channel_name() {
    let (alice, bob, _, host) = trio("room-founder");
    let waypoint = host.display().to_string();
    found(&alice, "team", &waypoint);
    admit(&alice, &bob, "team");
    let refused = bob
        .run(&Request::RoomInvite {
            name: "team".to_owned(),
            lifetime: 3_600,
        })
        .expect_err("a member who cannot sign the roster minted an invitation");
    assert_eq!(refused.code(), "kusanagi.not_the_founder");

    let taken = alice
        .run(&Request::Invite {
            name: "team".to_owned(),
            waypoint,
            lifetime: 3_600,
            abilities: kusanagi_grant::Abilities::ALL,
            habit: kusanagi::Habit::default(),
        })
        .expect_err("a channel took a room's name");
    assert_eq!(taken.code(), "kusanagi.channel_exists");
}
