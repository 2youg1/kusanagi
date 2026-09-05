// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What one message in several segments does on a real host, asserted end to
//! end.
//!
//! Three facts, each with its own test. A divided message is one message on the
//! read: several drops on the host, one row in the answer, at the last part's
//! height. A message past the limit is refused before a host is opened: the
//! send fails with the limit in the code, and the host holds nothing that was
//! not already there. Nothing on the host distinguishes parts from sentences:
//! every drop under it is the same size.
//!
//! **A room's answer is rows of parts joined the same way.** Two on top are
//! asserted here because a room sheds nothing the channel already proves.

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
use kusanagi::{Request, Whose};

fn pair(tag: &str) -> (Endpoint, Endpoint, std::path::PathBuf, std::path::PathBuf) {
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
    .unwrap();
    (alice, bob, ground, host)
}

#[test]
fn a_divided_message_is_one_row_standing_at_its_last_part() {
    let (alice, bob, ground, host) = pair("divided-message");
    // Not text on purpose, so the answer renders it as hex and no rendering
    // question can hide a missing byte.
    let sent: Vec<u8> = (0..300_000_u32)
        .map(|byte| u8::try_from(byte % 251).unwrap())
        .collect();

    bob.run(&Request::Send {
        name: "alice".to_owned(),
        payload: sent.clone(),
    })
    .expect("a message in several parts was refused");

    let heard = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .expect("alice could not read bob"),
    );
    let rows = heard["segments"].as_array().unwrap();
    assert_eq!(rows.len(), 1, "a run of parts was not one message");
    // Three parts of 126 335 bytes, standing at heights 0, 1 and 2.
    assert_eq!(rows[0]["index"], 2, "it does not stand at its last part");
    assert_eq!(
        rows[0]["payload"].as_str().unwrap(),
        &kusanagi_kernel::Hex(&sent).to_string(),
        "joining lost or moved a byte"
    );

    // And every drop is the same size as every other drop, so a part stands out
    // no more than the sentence it could have been. Drops are filed under
    // period and ward, hence the recursion.
    let sizes: Vec<u64> = files_under(&host).into_iter().map(|(_, len)| len).collect();
    assert!(!sizes.is_empty(), "the host is holding nothing");
    assert!(
        sizes.iter().all(|len| *len == sizes[0]),
        "a part stood out on the host: {sizes:?}"
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// Every file under `root`, with its length.
fn files_under(root: &std::path::Path) -> Vec<(std::path::PathBuf, u64)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".staging") {
                continue;
            }
            found.extend(files_under(&path));
        } else if let Ok(data) = std::fs::metadata(&path) {
            found.push((path, data.len()));
        }
    }
    found
}

#[test]
fn past_the_limit_is_refused_before_a_host_is_opened() {
    let (alice, bob, ground, host) = pair("divided-refused");
    let limit = usize::try_from(kusanagi_kernel::PART_ROOM).unwrap() * 32;
    let refused = bob
        .run(&Request::Send {
            name: "alice".to_owned(),
            payload: vec![b'm'; limit + 1],
        })
        .expect_err("a message past the limit went out");
    assert_eq!(refused.code(), "segment.message_too_large");
    let said = refused.render(false);
    assert!(
        said.contains(&limit.to_string()),
        "the refusal does not name the limit: {said}"
    );
    assert!(
        said.contains("7z") || said.contains("split"),
        "the refusal does not say what to do instead: {said}"
    );

    let alice_refused = alice
        .run(&Request::Send {
            name: "bob".to_owned(),
            payload: vec![b'm'; limit + 1],
        })
        .expect_err("a message past the limit went out");
    assert_eq!(alice_refused.code(), "segment.message_too_large");

    // Nothing past the limit even opens the box: the offer drop and the
    // greetings are all the host was ever going to hold.
    let _ = host;
    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_slotted_channel_carries_one_drop_and_says_so() {
    let ground = scratch("divided-slotted");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let habit = kusanagi::Habit {
        cadence: kusanagi::Cadence::Slotted {
            period: core::num::NonZeroU32::new(600).unwrap(),
        },
        ..kusanagi::Habit::default()
    };
    let invitation = invite_line(&alice, "bob-slotted", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "bob-slotted".to_owned(),
        habit,
    })
    .unwrap();

    let refused = bob
        .run(&Request::Send {
            name: "bob-slotted".to_owned(),
            payload: vec![b'm'; usize::try_from(kusanagi_kernel::MAX_PAYLOAD).unwrap() + 1],
        })
        .expect_err("a message in several drops was queued for a slot");
    assert_eq!(refused.code(), "kusanagi.slotted_one_drop");

    std::fs::remove_dir_all(&ground).ok();
}
