// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What `doctor --here` says, and what it must never say.
//!
//! The README tells a reader several things they *should* have arranged. Each
//! one is a field here, so that checking is a command rather than a belief. The
//! second property is the one that makes this safe to run and paste: a report
//! about a site must not contain anything out of the site.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, invite_line, json, scratch};
use kusanagi::{Habit, Request};

/// Every field agrees with what the rest of the suite asserts separately.
#[test]
fn the_report_says_what_this_machine_is_actually_doing() {
    let ground = scratch("here");
    let endpoint = Endpoint::new(ground.join("who"));
    invite_line(&endpoint, "bob", &ground.join("host").display().to_string());

    let report = json(&endpoint.run(&Request::Here).unwrap());

    assert_eq!(report["command"], "here");
    assert_eq!(report["contract"], 1);
    assert_eq!(report["site"], ground.join("who").display().to_string());

    // `at_rest.rs` decides this, and `at_rest.rs` is what the test for it reads.
    // Two names for one store would be two authorities.
    assert_eq!(report["at_rest"], kusanagi_vault::store());

    // `%TEMP%` is `%LOCALAPPDATA%\Temp`, so a scratch site is under the profile
    // — which is the same fact `site/tests/unreadable_windows.rs` leans on when
    // it finds no outsider on any file. Off Windows the question has no meaning
    // and the field is null rather than a guess.
    if cfg!(windows) {
        assert_eq!(report["under_profile"], true);
    } else {
        assert!(report["under_profile"].is_null());
    }

    // And a root that is plainly outside it answers the other way, or the field
    // is a constant rather than a measurement.
    let outside = Endpoint::new(std::path::PathBuf::from(".kusanagi-outside"));
    let elsewhere = json(&outside.run(&Request::Here).unwrap());
    if cfg!(windows) {
        assert_eq!(elsewhere["under_profile"], false);
    }
    std::fs::remove_dir_all(".kusanagi-outside").ok();

    // The binary hashes itself, so verifying a build needs no second tool.
    let binary = report["binary"].as_str().unwrap();
    assert_eq!(binary.len(), 64, "a blake3 is 64 hexadecimal digits");
    assert!(binary.chars().all(|digit| digit.is_ascii_hexdigit()));

    // Run twice, same answer: nothing here is sampled.
    let again = json(&endpoint.run(&Request::Here).unwrap());
    assert_eq!(again["binary"], report["binary"]);

    std::fs::remove_dir_all(&ground).ok();
}

/// A report an operator pastes into an issue must carry nothing out of the site.
#[test]
fn the_report_carries_nothing_that_was_meant_to_stay_on_this_disk() {
    let ground = scratch("here-secrets");
    let endpoint = Endpoint::new(ground.join("who"));
    let invitation = invite_line(&endpoint, "bob", &ground.join("host").display().to_string());
    // Somebody has to be on the other end before anything can be written to
    // them: a segment is filed where its reader looks, and there is nowhere to
    // file one for a reader who never arrived.
    let guest = Endpoint::new(ground.join("guest"));
    guest
        .run(&Request::Join {
            invite: invitation.clone(),
            name: "who".to_owned(),
            habit: Habit::default(),
        })
        .unwrap();
    endpoint.send("bob", "something nobody else should read");

    let rendered = endpoint
        .run(&Request::Here)
        .unwrap()
        .render(true, common::FENCE);

    // The channel name, the invitation and the handle are each a different kind
    // of thing this endpoint holds; none of them is a fact about this machine.
    assert!(!rendered.contains("bob"));
    assert!(!rendered.contains(&invitation));
    assert!(!rendered.contains(&endpoint.handle()));

    // And nothing that is on the disk appears in it either. Every file under the
    // site is checked rather than a chosen one, so a field added later that
    // echoes a record fails this without anybody remembering to come back.
    for stored in walk(&ground.join("who")) {
        let hex = kusanagi_kernel::Hex(&stored).to_string();
        for window in hex.as_bytes().chunks(32).filter(|part| part.len() == 32) {
            let needle = String::from_utf8_lossy(window).into_owned();
            assert!(
                !rendered.contains(&needle),
                "the report echoes 16 bytes of a file in the site"
            );
        }
    }

    std::fs::remove_dir_all(&ground).ok();
}

/// The bytes of every file under a directory.
fn walk(root: &std::path::Path) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(walk(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            found.push(bytes);
        }
    }
    found
}
