// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a host learns from *what* it is asked for.
//!
//! `unlinkable.rs` takes the host's side and reads what the host is holding. It
//! is silent about the other thing a host has, which is an access log: every
//! request names what it wants, and a reader that named an address handed the
//! host the pair of that address's writer and its reader.
//!
//! Under D-20 a read names a **bin** — a period and a ward — and takes all of
//! it. So the properties asserted here are about the requests themselves:
//! **no fetch names an object the host did not list first**, **an idle poll is
//! a listing and nothing more**, and **two readers of one ward ask for the same
//! things**, whoever their peers are.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_in_result,
    reason = "test code"
)]

mod common;

use common::watching::Watching;
use common::{Endpoint, invite_line, scratch};
use kusanagi::{Lane, Reach, Request, Site, SystemClock, track};
use kusanagi_kernel::{Clock as _, Listing as _, Object, Sweep, VerifyingKey, Ward};
use kusanagi_waypoint::DirWaypoint;

/// How many segments the peer has already written before the poll being measured.
const HEIGHT: usize = 12;

/// Alice writes `HEIGHT` segments to a reader, who has joined and not yet read.
fn staged(tag: &str, reader_ward: Option<Ward>) -> (std::path::PathBuf, Endpoint, Endpoint) {
    let ground = scratch(tag);
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));
    if let Some(ward) = reader_ward {
        Site::at(bob.site_root())
            .adopt(&kusanagi::fresh_seed().unwrap(), ward)
            .unwrap();
    }
    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .unwrap();
    for round in 0..HEIGHT {
        alice.send("bob", &format!("round {round}"));
    }
    (ground, alice, bob)
}

/// The peer's lane as the reader opens it: filed in the reader's own ward.
fn peer_lane(site: &Site) -> Lane {
    let channel = site.channel("alice").unwrap();
    let peer = channel.peer.as_ref().expect("the reader has met alice");
    Lane::open(
        site,
        "alice",
        &channel,
        &peer.key,
        site.ward().unwrap().unwrap(),
        SystemClock.now(),
    )
    .expect("a lane")
}

/// Every fetch named something a listing had just handed back.
fn only_what_was_listed(watching: &Watching, host: &std::path::Path) {
    let listed: Vec<Object> = watching
        .lists()
        .iter()
        .flat_map(|prefix| {
            DirWaypoint::new(host)
                .list(&Sweep::from_prefix(prefix).unwrap())
                .unwrap()
        })
        .collect();
    for got in watching.gets() {
        assert!(
            listed.contains(&got),
            "the reader fetched {got}, which no listing it asked for contained: the host has \
             been handed an address the reader derived"
        );
    }
}

#[test]
fn a_read_fetches_nothing_the_host_did_not_list_first() {
    let (ground, _alice, bob) = staged("unwatched", None);
    let host = ground.join("host");
    let site = Site::at(bob.site_root());
    let lane = peer_lane(&site);
    let watching = Watching::new(&host);

    let caught_up = track(
        &site,
        "alice",
        &watching,
        &lane,
        Reach::Whole,
        SystemClock.now(),
    )
    .unwrap();
    assert_eq!(caught_up.held().len(), HEIGHT);

    // Every listing names the reader's own ward and a period, and nothing else.
    let ward = format!("/{}/", site.ward().unwrap().unwrap());
    for prefix in watching.lists() {
        assert!(
            prefix.ends_with(&ward),
            "a listing asked for {prefix}, which is not one period of this reader's ward"
        );
    }
    only_what_was_listed(&watching, &host);
    // And the bin was taken together rather than object after object, so the
    // access log shows a bin being read and not a sequence being followed.
    assert!(
        watching.busiest() > 1,
        "the bin was fetched one object at a time"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_poll_that_finds_the_bin_as_it_was_makes_one_request_and_grows_with_nothing() {
    let (ground, alice, bob) = staged("unwatched-poll", None);
    let host = ground.join("host");
    let site = Site::at(bob.site_root());
    let lane = peer_lane(&site);
    let watching = Watching::new(&host);
    track(
        &site,
        "alice",
        &watching,
        &lane,
        Reach::Whole,
        SystemClock.now(),
    )
    .unwrap();

    // The poll an agent runs in a loop. The bin lists exactly as it did, so
    // there is nothing new to take and nothing is taken.
    watching.forget();
    let polled = track(
        &site,
        "alice",
        &watching,
        &lane,
        Reach::Head,
        SystemClock.now(),
    )
    .unwrap();
    assert_eq!(
        polled.head().map(|head| head.index()),
        Some(u64::try_from(HEIGHT).unwrap() - 1)
    );
    assert!(
        watching.gets().is_empty(),
        "an idle poll fetched {} objects out of a bin it had already taken",
        watching.gets().len()
    );
    let idle = watching.lists().len();
    assert!((1..=2).contains(&idle), "an idle poll made {idle} listings");

    // The stream doubles; the poll that finds the change takes the bin, and the
    // one after it is idle again at exactly the same cost as before.
    for round in HEIGHT..HEIGHT * 2 {
        alice.send("bob", &format!("round {round}"));
    }
    watching.forget();
    let grown = track(
        &site,
        "alice",
        &watching,
        &lane,
        Reach::Head,
        SystemClock.now(),
    )
    .unwrap();
    assert_eq!(
        grown.head().map(|head| head.index()),
        Some(2 * u64::try_from(HEIGHT).unwrap() - 1)
    );
    assert!(!watching.gets().is_empty(), "a changed bin was not taken");
    only_what_was_listed(&watching, &host);
    watching.forget();
    track(
        &site,
        "alice",
        &watching,
        &lane,
        Reach::Head,
        SystemClock.now(),
    )
    .unwrap();
    assert!(
        watching.gets().is_empty(),
        "a poll after the catch-up fetched again"
    );
    assert_eq!(
        watching.lists().len(),
        idle,
        "a poll got more expensive as the stream grew"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn two_readers_of_one_ward_ask_the_host_for_the_same_things() {
    // Two readers who chose the same ward and talk to different people. The host
    // must not be able to tell from their requests which of them is whose.
    let ward = Ward::from_bits(0x0c0d);
    let (one, _, bob) = staged("unwatched-twins-a", Some(ward));
    let (two, _, carol) = staged("unwatched-twins-b", Some(ward));
    // Both readers' peers wrote into one host, into the same ward.
    merge(&two.join("host"), &one.join("host"));
    let host = one.join("host");

    let asked = |reader: &Endpoint| {
        let site = Site::at(reader.site_root());
        let lane = peer_lane(&site);
        let watching = Watching::new(&host);
        let walked = track(
            &site,
            "alice",
            &watching,
            &lane,
            Reach::Whole,
            SystemClock.now(),
        )
        .unwrap();
        assert_eq!(
            walked.held().len(),
            HEIGHT,
            "a reader missed its own segments"
        );
        let mut requests = watching.asked();
        requests.sort();
        requests
    };
    assert_eq!(
        asked(&bob),
        asked(&carol),
        "two readers of one ward made different requests, so the host can tell them apart"
    );

    std::fs::remove_dir_all(&one).ok();
    std::fs::remove_dir_all(&two).ok();
}

#[test]
fn sending_asks_for_the_peers_ward_and_names_no_address() {
    let (ground, alice, _bob) = staged("unwatched-send", None);
    let host = ground.join("host");
    let site = Site::at(alice.site_root());
    let channel = site.channel("bob").unwrap();
    let peer_ward = channel.peer.as_ref().expect("alice has met bob").ward;
    let lane = Lane::open(
        &site,
        "bob",
        &channel,
        &alice_key(&site),
        peer_ward,
        SystemClock.now(),
    )
    .expect("a lane");
    let cairn = site
        .cairn("bob", &alice_key(&site).handle())
        .unwrap()
        .expect("sending left no record of where it got to");
    assert_eq!(cairn.head().index(), u64::try_from(HEIGHT - 1).unwrap());

    // The walk that `send` performs finds its head by sweeping the ward it
    // writes into — the host already saw this endpoint write there — and takes
    // nothing, because the bin lists as it did after the last send.
    let watching = Watching::new(&host);
    track(
        &site,
        "bob",
        &watching,
        &lane,
        Reach::Head,
        SystemClock.now(),
    )
    .unwrap();
    let ward = format!("/{peer_ward}/");
    for prefix in watching.lists() {
        assert!(prefix.ends_with(&ward), "a send listed {prefix}");
    }
    assert!(
        watching.gets().is_empty(),
        "a send fetched from a bin it had already taken"
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// Copies every object of one host into another, key for key.
fn merge(from: &std::path::Path, into: &std::path::Path) {
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = into.join(entry.file_name());
        if entry.path().is_dir() {
            std::fs::create_dir_all(&target).unwrap();
            merge(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

fn alice_key(site: &Site) -> VerifyingKey {
    site.identity()
        .unwrap()
        .expect("an endpoint that has sent has an identity")
        .verifying_key()
}

#[test]
fn a_shorter_width_lists_fewer_digits_and_still_finds_every_segment() {
    let (ground, _alice, bob) = staged("unwatched-width", None);
    let host = ground.join("host");
    bob.run(&Request::Sweep {
        digits: Some(2),
        cap: None,
    })
    .unwrap();
    let site = Site::at(bob.site_root());
    let lane = peer_lane(&site);
    let watching = Watching::new(&host);

    let walked = track(
        &site,
        "alice",
        &watching,
        &lane,
        Reach::Whole,
        SystemClock.now(),
    )
    .unwrap();
    assert_eq!(walked.held().len(), HEIGHT);
    // Two digits of the ward and no separator: the prefix names 256 wards.
    let two: String = site
        .ward()
        .unwrap()
        .unwrap()
        .to_string()
        .chars()
        .take(2)
        .collect();
    for prefix in watching.lists() {
        assert!(
            prefix.ends_with(&format!("/{two}")),
            "a two-digit sweep listed {prefix}"
        );
    }
    only_what_was_listed(&watching, &host);

    std::fs::remove_dir_all(&ground).ok();
}
