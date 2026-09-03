// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a host learns by watching *which* addresses are asked for.
//!
//! `unlinkable.rs` takes the host's side and reads what the host is holding. It
//! is the stronger assertion in one direction and silent in another: a real host
//! also serves the requests, and a request names an address out loud.
//!
//! A reader that starts at height zero every time asks for every address of one
//! stream, in ascending order, back to back, on one connection. Those addresses
//! are unlinkable to each other only until the moment somebody asks for them in
//! that order — at which point the host has been handed exactly the grouping the
//! derivation in `seal` exists to deny it.
//!
//! So the property asserted here is about cost, and it is a privacy property
//! rather than a performance one: **the number of addresses one read reveals
//! must not grow with the length of the stream.**

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

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::{Endpoint, invite_line, scratch};
use kusanagi::{Reach, Request, Site, track};
use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};
use kusanagi_seal::derive;
use kusanagi_waypoint::DirWaypoint;

/// How many segments the peer has already written before the poll being measured.
const HEIGHT: usize = 12;

/// A host that writes down every address it is asked for, in the order it was
/// asked. This is the cheapest thing a real host can do, and every one of them
/// does it: it is called an access log.
struct Watching {
    inner: DirWaypoint,
    asked: Mutex<Vec<DropAddr>>,
    /// How many reads were open at once, at the busiest moment.
    ///
    /// A serial walk never gets above one. That number is the whole of what a
    /// window changes from the host's side, so it is the number the test asserts
    /// on rather than the order the addresses arrived in — which is decided by a
    /// scheduler and would make this a test about the scheduler.
    open: AtomicUsize,
    busiest: AtomicUsize,
}

impl Watching {
    fn new(root: &std::path::Path) -> Self {
        Self {
            inner: DirWaypoint::new(root),
            asked: Mutex::new(Vec::new()),
            open: AtomicUsize::new(0),
            busiest: AtomicUsize::new(0),
        }
    }

    /// The most reads this host had in flight at one time.
    fn busiest(&self) -> usize {
        self.busiest.load(Ordering::SeqCst)
    }

    /// Everything asked for since the last [`Self::forget`].
    fn asked(&self) -> Vec<DropAddr> {
        self.asked.lock().unwrap().clone()
    }

    fn forget(&self) {
        self.asked.lock().unwrap().clear();
    }
}

impl Waypoint for Watching {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        self.inner.put_if_absent(addr, bytes)
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        self.asked.lock().unwrap().push(*addr);
        let now = self.open.fetch_add(1, Ordering::SeqCst) + 1;
        self.busiest.fetch_max(now, Ordering::SeqCst);
        // Long enough that a batch issued together is still together when the
        // last of it arrives. Without it this measures how fast a local
        // directory is, which is not what a host looks like.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let found = self.inner.get(addr);
        self.open.fetch_sub(1, Ordering::SeqCst);
        found
    }
}

#[test]
fn a_read_does_not_replay_the_whole_stream_to_the_host() {
    let ground = scratch("unwatched");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    for round in 0..HEIGHT {
        alice.send("bob", &format!("round {round}"));
    }

    // Bob's own view of the channel, which is all a reader has.
    let site = Site::at(bob.site_root());
    let channel = site.channel("alice").unwrap();
    let peer = channel.peer.as_ref().expect("bob has met alice");
    let stream = channel.secret.stream(&peer.handle());

    let watching = Watching::new(&host);

    // The first read shows the whole stream, so it fetches the whole stream. That
    // cost is what catching up costs, and it is not what this file is about.
    let caught_up = track(&site, "alice", &watching, &stream, &peer.key, Reach::Whole).unwrap();
    assert_eq!(caught_up.held().len(), HEIGHT);
    let catching_up = watching.asked().len();
    assert!(
        (HEIGHT..=HEIGHT + kusanagi::WINDOW).contains(&catching_up),
        "catching up asked for {catching_up} addresses of a stream of {HEIGHT}"
    );
    // **More than the stream holds, on purpose.** A catch-up asks in windows, so
    // the last window runs past the live edge and the host is shown addresses
    // above the highest segment there is. That is the difference between knowing
    // where a stream ends and knowing it to within a window.
    assert!(
        catching_up > HEIGHT,
        "the walk stopped exactly at the live edge, which tells the host where it is"
    );
    // And it asked for them together. A host that sees *N*, then *N+1* after it
    // answered *N*, is being handed the shape of a chain by the reading pattern;
    // one that sees eight arrive at once is not.
    assert!(
        watching.busiest() > 1,
        "a catch-up asked for one address at a time, so the access log still \
         reads as a chain"
    );

    // The poll an agent actually runs in a loop. Nothing has changed, and bob has
    // already verified all of it.
    watching.forget();
    let polled = track(&site, "alice", &watching, &stream, &peer.key, Reach::Head).unwrap();
    assert_eq!(
        polled.head(),
        caught_up.head(),
        "the resumed walk lost the stream's height"
    );

    let revealed = watching.asked();
    assert!(
        revealed.len() <= 2,
        "one poll named {} addresses of a stream of {HEIGHT}; a host with an \
         access log now holds the grouping that `seal` exists to deny it",
        revealed.len()
    );

    // And the cost is flat rather than merely smaller: a stream twice as long
    // must cost a poll exactly the same, or this is an optimisation rather than
    // the closing of a leak.
    for round in HEIGHT..HEIGHT * 2 {
        alice.send("bob", &format!("round {round}"));
    }
    track(&site, "alice", &watching, &stream, &peer.key, Reach::Head).unwrap();
    watching.forget();
    track(&site, "alice", &watching, &stream, &peer.key, Reach::Head).unwrap();
    assert_eq!(
        watching.asked().len(),
        revealed.len(),
        "a poll got more expensive as the stream grew"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_poll_names_the_one_address_it_is_waiting_on_and_no_other() {
    let ground = scratch("unwatched-exact");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    for round in 0..HEIGHT {
        alice.send("bob", &format!("round {round}"));
    }

    let site = Site::at(bob.site_root());
    let channel = site.channel("alice").unwrap();
    let peer = channel.peer.as_ref().expect("bob has met alice");
    let stream = channel.secret.stream(&peer.handle());

    let watching = Watching::new(&host);
    track(&site, "alice", &watching, &stream, &peer.key, Reach::Whole).unwrap();

    // "At most two" is a bound; this is the fact. A poll asks for the height
    // above the one it has verified, and asks for nothing else — so a host sees
    // one address it has never been shown before, carrying no relation to any
    // address it has seen.
    watching.forget();
    track(&site, "alice", &watching, &stream, &peer.key, Reach::Head).unwrap();
    let (expected, _) = derive(&stream, u64::try_from(HEIGHT).unwrap());
    assert_eq!(
        watching.asked(),
        vec![expected],
        "a poll named something other than exactly the height it waits on"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn sending_does_not_replay_your_own_stream_to_the_host() {
    let ground = scratch("unwatched-send");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    for round in 0..HEIGHT {
        alice.send("bob", &format!("round {round}"));
    }

    // Every one of those sends went through the ordinary door. If `send` had
    // walked from genesis it would have left no cairn, and if it had left one
    // without resuming from it the height would still be right — so the assertion
    // that it resumed is the next one, and this is the assertion that it marked.
    let site = Site::at(alice.site_root());
    let channel = site.channel("bob").unwrap();
    let stream = channel.secret.stream(&alice_key(&site).handle());
    let cairn = site
        .cairn("bob", &alice_key(&site).handle())
        .unwrap()
        .expect("sending left no record of where it got to");
    assert_eq!(cairn.head().index(), u64::try_from(HEIGHT - 1).unwrap());

    // And the walk that `send` performs, from that cairn, names one address: the
    // height it is about to claim. Writing the thousandth segment must cost what
    // writing the first cost, or a host learns how long a conversation has run
    // from the shape of a single send.
    let watching = Watching::new(&host);
    track(
        &site,
        "bob",
        &watching,
        &stream,
        &alice_key(&site),
        Reach::Head,
    )
    .unwrap();
    let (expected, _) = derive(&stream, u64::try_from(HEIGHT).unwrap());
    assert_eq!(
        watching.asked(),
        vec![expected],
        "a send named more than the height it was about to claim"
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// This endpoint's own key, which checks the author of its own stream.
fn alice_key(site: &Site) -> kusanagi_kernel::VerifyingKey {
    site.identity()
        .unwrap()
        .expect("an endpoint that has sent has an identity")
        .verifying_key()
}
