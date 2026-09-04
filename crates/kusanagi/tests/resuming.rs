// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a cairn may and may not change about a read.
//!
//! `unwatched.rs` asserts what a poll costs the reader in privacy: how many
//! addresses it names out loud. These two assert the other side of the same
//! mechanism — that buying that saving changed nothing about the answer.
//!
//! They live apart from it because they are a different claim with a different
//! failure. A cairn that is read wrongly makes a poll expensive, which is the
//! subject of the other file; a cairn that is *trusted* wrongly makes a read
//! quietly short, and no assertion about a message arriving would notice.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

mod common;

use common::{Endpoint, invite_line, scratch};
use kusanagi::Request;

/// How many segments the peer has written before the read being measured.
const HEIGHT: usize = 12;

#[test]
fn losing_every_cairn_changes_what_a_read_costs_and_nothing_else() {
    let ground = scratch("unwatched-kill");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

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

    let read = |endpoint: &Endpoint| {
        common::json(
            &endpoint
                .run(&Request::Read {
                    name: "alice".to_owned(),
                    after: Some(3),
                    whose: kusanagi::Whose::Peer,
                })
                .unwrap(),
        )
    };

    // A cairn is the only thing on a site that can be recomputed, so it is the
    // only thing whose loss must be invisible in a result. Deleting the lot is
    // the kill that `ARCHITECTURE.md` §7 law 1 is about, in its strongest form:
    // not a new process over the same files, but the files themselves gone.
    let before = read(&bob);
    std::fs::remove_dir_all(bob.site_root().join("cairns")).unwrap();
    let after = read(&bob);
    assert_eq!(before, after, "a lost cairn changed what a read reported");

    // And alice's own sending is unaffected by losing hers, which is the half
    // that would fork a chain rather than merely misreport one.
    std::fs::remove_dir_all(alice.site_root().join("cairns")).unwrap();
    alice.send("bob", "after the cairns were lost");
    let heard = common::json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: kusanagi::Whose::Peer,
        })
        .unwrap(),
    );
    assert_eq!(heard["height"], u64::try_from(HEIGHT).unwrap());
    assert_eq!(
        heard["segments"][HEIGHT]["text"],
        "after the cairns were lost"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_read_that_shows_segments_shows_every_one_it_was_asked_for() {
    let ground = scratch("unwatched-floor");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

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

    // Read once so that a cairn exists at the top of the stream, then ask for a
    // floor far below it. Resuming from the cairn would fetch nothing and report
    // nothing, which is the way this change could quietly lose messages.
    for after in [None, Some(0), Some(2)] {
        let seen = common::json(
            &bob.run(&Request::Read {
                name: "alice".to_owned(),
                after,
                whose: kusanagi::Whose::Peer,
            })
            .unwrap(),
        );
        let expected = HEIGHT - after.map_or(0, |floor| usize::try_from(floor).unwrap() + 1);
        assert_eq!(
            seen["segments"].as_array().unwrap().len(),
            expected,
            "reading with --after {after:?} after a cairn was written lost segments"
        );
        assert_eq!(seen["height"], u64::try_from(HEIGHT - 1).unwrap());
    }

    std::fs::remove_dir_all(&ground).ok();
}
