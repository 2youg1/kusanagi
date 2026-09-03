// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Losing a site, and getting it back.
//!
//! A site is about to become the only copy of things nothing else can rebuild:
//! release deletes acknowledged drops, a ratchet burns the key that reopened
//! them, and encryption at rest ties the directory to one account. Every one of
//! those turns a lost site from an inconvenience into a lost conversation.
//!
//! An agent has no GUI to be reminded by, so the backup is a verb. These tests
//! are what says the verb works: not that it writes a file, but that a site
//! deleted and restored still reads its peer from where it had got to.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, FENCE, invite_line, json, scratch};
use kusanagi::{Outcome, Request, Site, Whose};

/// Exports `endpoint` and returns the recovery key and the archive.
fn exported(endpoint: &Endpoint) -> ([u8; 32], Vec<u8>) {
    let outcome = endpoint.run(&Request::Export).expect("export failed");
    let Outcome::Exported { recovery, archive } = outcome else {
        panic!("export did not report an archive");
    };
    let bytes = kusanagi_kernel::unhex(&recovery).expect("a recovery key is hexadecimal");
    (bytes.as_slice().try_into().unwrap(), archive)
}

/// A pair of endpoints with a channel between them and three messages on it.
fn talking(tag: &str) -> (std::path::PathBuf, Endpoint, Endpoint) {
    let ground = scratch(tag);
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    for line in ["the first", "the second", "the third"] {
        alice.send("bob", line);
    }
    // Alice reads bob so that she has a cairn on his lane as well as her own.
    alice
        .run(&Request::Read {
            name: "bob".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap();
    (ground, alice, bob)
}

#[test]
fn a_site_deleted_and_restored_is_the_site_it_was() {
    let (ground, alice, _bob) = talking("backup-round-trip");
    let before = json(&alice.run(&Request::Channels).unwrap());
    let identity = alice.handle();
    let (recovery, archive) = exported(&alice);

    // The whole of what alice had, gone.
    let root = alice.site_root().to_path_buf();
    drop(alice);
    std::fs::remove_dir_all(&root).unwrap();

    let restored = Endpoint::new(root.clone());
    let report = restored
        .run(&Request::Import {
            recovery,
            archive: archive.clone(),
        })
        .expect("import refused a key it produced");
    assert_eq!(json(&report)["channels"], 1);

    assert_eq!(
        restored.handle(),
        identity,
        "the identity did not come back"
    );
    assert_eq!(
        json(&restored.run(&Request::Channels).unwrap()),
        before,
        "the listing after a restore is not the listing before it"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_restored_site_resumes_where_it_had_got_to() {
    let (ground, _alice, bob) = talking("backup-cairn");
    // Bob reads alice to height three, so his cairn says so.
    let heard = json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .unwrap(),
    );
    assert_eq!(heard["height"], 2);

    let (recovery, archive) = exported(&bob);
    let root = bob.site_root().to_path_buf();
    drop(bob);
    std::fs::remove_dir_all(&root).unwrap();

    let restored = Endpoint::new(root);
    restored
        .run(&Request::Import { recovery, archive })
        .unwrap();

    // The evidence a cairn came back: the height is still known, and the read
    // that reports it did not have to walk from genesis to find out.
    let again = json(
        &restored
            .run(&Request::Read {
                name: "alice".to_owned(),
                after: Some(2),
                whose: Whose::Peer,
            })
            .unwrap(),
    );
    assert_eq!(again["height"], 2);
    assert_eq!(again["segments"].as_array().unwrap().len(), 0);

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_wrong_key_opens_nothing_and_says_only_that() {
    let (ground, alice, _bob) = talking("backup-wrong-key");
    let (_recovery, archive) = exported(&alice);

    let elsewhere = Endpoint::new(ground.join("elsewhere"));
    let refused = elsewhere
        .run(&Request::Import {
            recovery: [0; 32],
            archive,
        })
        .expect_err("an archive opened under a key that did not seal it");
    assert_eq!(refused.code(), "kusanagi.bad_recovery_key");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_archive_names_nobody_and_carries_no_seed_in_the_clear() {
    let (ground, alice, _bob) = talking("backup-opaque");
    let handle = alice.handle();
    let (_recovery, archive) = exported(&alice);

    // The channel is called `bob` here, and that name is a local fact this
    // endpoint keeps. It must not be readable in a file somebody backs up to a
    // cloud drive.
    let found = archive.windows(3).any(|window| window == b"bob".as_slice());
    assert!(!found, "a channel name is in the clear in an archive");

    // Nor is the handle, which is derived from the seed the archive carries.
    let handle_bytes = kusanagi_kernel::unhex(&handle).unwrap();
    let leaked = archive
        .windows(handle_bytes.len())
        .any(|window| window == handle_bytes.as_slice());
    assert!(!leaked, "an identity is in the clear in an archive");

    // What is in the clear is exactly the four bytes that say what this file is.
    assert_eq!(&archive[..4], b"KSNB");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_archive_does_not_land_on_top_of_a_site_that_is_already_there() {
    let (ground, alice, _bob) = talking("backup-occupied");
    let (recovery, archive) = exported(&alice);

    let occupied = Endpoint::new(ground.join("occupied"));
    occupied.run(&Request::Identity).unwrap();
    let refused = occupied
        .run(&Request::Import { recovery, archive })
        .expect_err("an import merged into a site that already had an identity");
    assert_eq!(refused.code(), "kusanagi.malformed");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn the_prose_shows_the_recovery_key_and_the_json_carries_it_too() {
    let ground = scratch("backup-said");
    let alice = Endpoint::new(ground.join("alice"));
    alice.run(&Request::Identity).unwrap();
    let outcome = alice.run(&Request::Export).unwrap();

    let said = outcome.render(false, FENCE);
    assert!(said.contains("shown once"), "{said}");
    let machine = json(&outcome);
    assert_eq!(machine["recovery"].as_str().unwrap().len(), 64);
    // The archive itself is never in JSON: it is bytes, and it goes to stdout.
    assert!(machine.get("archive").is_none());

    std::fs::remove_dir_all(&ground).ok();
}

/// A site with no identity has nothing to export, and says so.
#[test]
fn an_empty_site_exports_nothing_rather_than_an_empty_archive() {
    let ground = scratch("backup-empty");
    let site = Site::at(ground.join("nothing"));
    let refused = kusanagi::run(&site, &Request::Export)
        .expect_err("an endpoint with no identity produced an archive");
    assert_eq!(refused.code(), "kusanagi.no_identity");

    std::fs::remove_dir_all(&ground).ok();
}
