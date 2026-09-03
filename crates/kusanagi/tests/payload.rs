// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a program puts in, a program gets back.
//!
//! The reader on the other side of this door is usually an agent, and an agent
//! that cannot recover the exact bytes it sent has no channel — it has a
//! rumour. These two tests are the difference.

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

#[test]
fn a_payload_that_is_not_text_survives_the_round_trip() {
    let ground = scratch("payload");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();

    // A lone 0xff is not valid UTF-8, and a NUL is not something a shell will
    // carry. Both are ordinary bytes to a segment.
    let sent = vec![0xff, 0x00, 0xfe, b'h', b'i'];
    bob.run(&Request::Send {
        name: "alice".to_owned(),
        payload: sent.clone(),
    })
    .expect("bytes that are not text were refused");

    let heard = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .expect("alice could not read bob"),
    );

    // Bytes that are not text arrive as hexadecimal, exactly.
    assert_eq!(heard["segments"][0]["payload"], "ff00fe6869");
    // And the readable field is absent rather than wrong. It used to be here
    // carrying replacement characters, which nothing downstream could tell from
    // the real thing — a lossy rendering that looks like a lossless one is worse
    // than no rendering at all.
    assert!(heard["segments"][0]["text"].is_null());
}

#[test]
fn reading_after_a_height_reports_only_what_follows() {
    let ground = scratch("after");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    for line in ["one", "two", "three"] {
        bob.send("alice", line);
    }

    let everything = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    assert_eq!(everything["segments"].as_array().unwrap().len(), 3);

    let rest = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: Some(0),
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    // Two segments follow height 0 …
    assert_eq!(rest["segments"].as_array().unwrap().len(), 2);
    assert_eq!(rest["segments"][0]["text"], "two");
    // … and the verified head is still reported in full, which is what makes
    // one call enough to answer "is there anything new".
    assert_eq!(rest["height"], 2);

    let nothing = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: Some(2),
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    assert_eq!(nothing["segments"].as_array().unwrap().len(), 0);
    assert_eq!(nothing["height"], 2);

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn the_largest_payload_that_fits_a_drop_goes_through_and_one_more_does_not() {
    // The limit is not a number somebody picked. Every sealed drop is one fixed
    // size, and `MAX_PAYLOAD` is what is left of that size after the segment's
    // own fields and the authentication tag — so this is the boundary between a
    // message that fits in one envelope and one that would need chunking, which
    // does not exist yet.
    let ground = scratch("payload-limit");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();

    let brim = usize::try_from(kusanagi_kernel::MAX_PAYLOAD).unwrap();
    alice
        .run(&Request::Send {
            name: "bob".to_owned(),
            payload: vec![b'z'; brim],
        })
        .expect("the largest payload that fits was refused");

    let refused = alice
        .run(&Request::Send {
            name: "bob".to_owned(),
            payload: vec![b'z'; brim + 1],
        })
        .expect_err("one byte over the limit was accepted");
    assert_eq!(refused.code(), "segment.payload_too_large");

    // And the drop that did go through is the same size as every other one, so
    // the largest message this network carries is not visible as the largest.
    let sizes: Vec<u64> = files_under(&host).into_iter().map(|(_, len)| len).collect();
    assert!(!sizes.is_empty(), "the host is holding nothing");
    assert!(
        sizes.iter().all(|len| *len == sizes[0]),
        "a full-sized message stood out on the host: {sizes:?}"
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
