// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What an endpoint's own disk gives away when somebody else gets it.
//!
//! `unlinkable.rs` and `unwatched.rs` both take the host's side. This file takes
//! the side of whoever ends up holding the laptop: a seized machine, a backup, a
//! container image pushed to a registry, a support bundle.
//!
//! A site cannot stop being sensitive — it holds the identity seed and every
//! channel secret, and anything else it holds is reachable from those. So the
//! property is not that a site reveals nothing. It is narrower and checkable:
//! **a site holds no message.** Whoever takes the disk can decrypt the traffic
//! they can still reach on the host; they must not additionally be handed the
//! plaintext of conversations whose drops are long gone.
//!
//! This is the assertion that guards a decision rather than a line of code.
//! Making a read resume needs a verified position, and the obvious way to carry
//! one is to keep the last segment — it re-verifies its own signature, so no
//! invariant has to be relaxed to trust it. It was not done that way, precisely
//! because the cost is a copy of the most recent message of every channel
//! sitting in a file forever. The cheap version of this change is one commit
//! away at all times, and this test is what stops it arriving quietly.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use std::path::Path;

use common::{Endpoint, invite_line, scratch};
use kusanagi::{Request, Whose};

/// Words that appear in no protocol field, so finding them is unambiguous.
const FROM_ALICE: &str = "quails-vex-nimble-fjords";
const FROM_BOB: &str = "waltzing-cyborg-jinx-hymn";

#[test]
fn a_site_holds_no_message_it_has_sent_or_received() {
    let ground = scratch("at-rest");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();

    // Several segments each way, and then a read on both sides, because reading
    // is what writes a position down. A site that keeps a message keeps it after
    // the read rather than after the send.
    for round in 0..4 {
        alice.send("bob", &format!("{FROM_ALICE} {round}"));
        bob.send("alice", &format!("{FROM_BOB} {round}"));
    }
    for (endpoint, channel) in [(&alice, "bob"), (&bob, "alice")] {
        for whose in [Whose::Peer, Whose::Mine] {
            endpoint
                .run(&Request::Read {
                    name: channel.to_owned(),
                    after: None,
                    whose,
                })
                .unwrap();
        }
    }

    for (endpoint, name) in [(&alice, "alice"), (&bob, "bob")] {
        for (path, bytes) in files_under(endpoint.site_root()) {
            for secret in [FROM_ALICE, FROM_BOB] {
                assert!(
                    !contains(&bytes, secret.as_bytes()),
                    "{name}'s site carries a message in the clear at {}",
                    path.display()
                );
            }
        }
    }

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn what_a_site_keeps_is_a_closed_list() {
    let ground = scratch("at-rest-shape");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    alice.send("bob", FROM_ALICE);
    bob.run(&Request::Read {
        name: "alice".to_owned(),
        after: None,
        whose: Whose::Peer,
    })
    .unwrap();

    // Naming the kinds, so that a fourth one is a decision somebody made rather
    // than a file that appeared. Each kind is one line in `site.rs`'s own map of
    // the directory, and the two have to be changed together.
    let kinds: Vec<String> = files_under(bob.site_root())
        .iter()
        .filter_map(|(path, _)| {
            let relative = path.strip_prefix(bob.site_root()).ok()?;
            Some(match relative.iter().next()?.to_string_lossy().as_ref() {
                "identity" => "identity".to_owned(),
                "channels" => "channels".to_owned(),
                "cairns" => "cairns".to_owned(),
                "revoked" => "revoked".to_owned(),
                other => format!("unexpected: {other}"),
            })
        })
        .collect();

    for kind in &kinds {
        assert!(
            !kind.starts_with("unexpected"),
            "a site grew a kind of file nobody wrote down: {kind}"
        );
    }
    assert!(
        kinds.contains(&"identity".to_owned()) && kinds.contains(&"channels".to_owned()),
        "a joined endpoint keeps an identity and a channel: {kinds:?}"
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// Every file in a tree, with its bytes.
fn files_under(root: &Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else if let Ok(bytes) = std::fs::read(&path) {
            found.push((path, bytes));
        }
    }
    found
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// What is on the disk when somebody else has the disk.
///
/// File permissions stop another account on a running machine. They stop nothing
/// once the drive is somewhere else, and "somewhere else" is a backup, a folder a
/// sync client uploads, or a laptop that was taken. On Windows every record is a
/// DPAPI blob whose key comes from this account's logon credentials, so what
/// travels is noise.
#[cfg(windows)]
#[test]
fn nothing_a_site_writes_is_readable_without_this_windows_account() {
    let ground = scratch("at-rest-sealed");
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = invite_line(&alice, "bob", &host.display().to_string());
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
    })
    .unwrap();
    alice.send("bob", "something worth keeping");

    let files = files_under(alice.site_root());
    assert!(files.len() >= 2, "the site wrote nothing to check");
    for (path, bytes) in &files {
        // Every record says which store sealed it, and on this platform that is
        // DPAPI. A `0x00` here would be a record written in the clear.
        assert_eq!(
            bytes.first(),
            Some(&0x01),
            "{} is not sealed at rest",
            path.display()
        );
        // The local name of the channel is the thing a person would grep for.
        assert!(
            !contains(bytes, b"bob"),
            "{} holds a channel name in the clear",
            path.display()
        );
    }

    // And the endpoint itself reads all of it back, which is the other half:
    // sealing that cannot be opened is data loss with a good excuse.
    let heard = common::json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Mine,
            })
            .unwrap(),
    );
    assert_eq!(heard["height"], 0);

    std::fs::remove_dir_all(&ground).ok();
}

/// A record from a platform whose store this one does not have.
#[test]
fn a_record_this_platform_cannot_open_is_refused_by_name() {
    let ground = scratch("at-rest-foreign");
    let alice = Endpoint::new(ground.join("alice"));
    alice.run(&Request::Identity).unwrap();

    // Tag `0x02` is reserved for the next platform's store. Nothing here has
    // one, so the answer must name the situation rather than hand back a blob.
    let identity = alice.site_root().join("identity");
    let mut foreign = vec![0x02_u8];
    foreign.extend_from_slice(&[7; 32]);
    std::fs::write(&identity, &foreign).unwrap();

    let refused = alice
        .run(&Request::Identity)
        .expect_err("a record from another platform was read as though it were ours");
    assert_eq!(refused.code(), "site.foreign_record");

    std::fs::remove_dir_all(&ground).ok();
}
