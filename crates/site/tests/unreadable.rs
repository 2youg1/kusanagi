// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who else on this machine can read a channel secret.
//!
//! `crates/kusanagi/tests/at_rest.rs` asserts that a site holds no message. It
//! says nothing about the things a site does hold and cannot avoid holding — the
//! identity seed and every channel secret — and those are enough to compute every
//! address on every channel and open everything still sitting on the host.
//!
//! A default `fs::write` leaves them `0644`. The attacker that needs is not a
//! nation state: it is a second account on a shared build machine, a sidecar
//! container, or an image layer pushed to a registry.
//!
//! The whole file is `#[cfg(unix)]` because the mode bits it checks exist only
//! there. What Windows does instead is written down in
//! `crates/site/src/permissions.rs`, and it is a gap rather than a solution.

#![cfg(unix)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]

use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
use std::path::Path;

use kusanagi_grant::StepId;
use kusanagi_kernel::Signer;
use kusanagi_seal::Secret;
use kusanagi_site::{Channel, Site, Standing};

fn scratch(tag: &str) -> Site {
    let root =
        std::env::temp_dir().join(format!("kusanagi-unreadable-{}-{tag}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    Site::at(root)
}

/// Every file and directory under `root`, with its mode.
fn modes(root: &Path) -> Vec<(std::path::PathBuf, u32)> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(data) = std::fs::metadata(&path) else {
            continue;
        };
        found.push((path.clone(), data.permissions().mode()));
        if path.is_dir() {
            found.extend(modes(&path));
        }
    }
    found
}

#[test]
fn nothing_a_site_writes_is_readable_by_anybody_else() {
    let site = scratch("modes");
    let author = Signer::from_seed(&[3; 32]);

    site.adopt(&[5; 32]).unwrap();
    site.keep(
        "peer",
        &Channel {
            secret: Secret::from_bytes([7; 32]),
            root: author.handle(),
            introduction: Signer::from_seed(&[2; 32]).handle(),
            locator: "./drops".to_owned(),
            standing: Standing::Root,
            peer: None,
        },
    )
    .unwrap();
    site.revoke(StepId::from_bytes([1; 32])).unwrap();

    let seen = modes(site.root());
    assert!(!seen.is_empty(), "the site wrote nothing to check");
    for (path, mode) in &seen {
        assert_eq!(
            mode & 0o077,
            0,
            "{} is mode {:o}; every other account on this machine can read a \
             channel secret from it",
            path.display(),
            mode
        );
    }

    // The root itself, which is what stops somebody listing the channel names
    // even where the files inside are already closed.
    let root = std::fs::metadata(site.root()).unwrap().permissions().mode();
    assert_eq!(root & 0o077, 0, "the site directory is mode {root:o}");

    std::fs::remove_dir_all(site.root()).unwrap();
}

#[test]
fn a_file_an_older_build_left_open_is_replaced_rather_than_chmodded() {
    let site = scratch("upgrade");
    site.adopt(&[5; 32]).unwrap();
    let revoked = site.root().join("revoked");
    site.revoke(StepId::from_bytes([1; 32])).unwrap();

    // What upgrading from a build that had no rule about this looks like.
    std::fs::set_permissions(&revoked, std::fs::Permissions::from_mode(0o644)).unwrap();
    let before = std::fs::metadata(&revoked).unwrap().ino();
    site.revoke(StepId::from_bytes([2; 32])).unwrap();

    let after = std::fs::metadata(&revoked).unwrap();
    assert_eq!(
        after.permissions().mode() & 0o077,
        0,
        "a file left world-readable by an older build stayed that way at {:o}",
        after.permissions().mode()
    );
    // It is closed because it is a *different file*, not because anything
    // chmodded the old one: changing the mode of a file this build did not
    // create is an operation that can be aimed elsewhere.
    assert_ne!(
        before,
        after.ino(),
        "the old file was modified in place instead of being replaced"
    );

    std::fs::remove_dir_all(site.root()).unwrap();
}

#[test]
fn a_symbolic_link_planted_at_a_record_is_replaced_and_never_followed() {
    // The attack: somebody who can write into a site directory replaces a
    // channel record with a link to a file its owner cares about. Opening the
    // path with `create` and `truncate` would empty that file, and
    // `set_permissions` on the path would chmod it. A write stages beside the
    // target and renames over it instead, and `rename` replaces the name rather
    // than following where it points.
    let site = scratch("symlink");
    site.adopt(&[5; 32]).unwrap();

    let bait = site
        .root()
        .join("..")
        .join(format!("kusanagi-unreadable-bait-{}", std::process::id()));
    std::fs::write(&bait, b"something its owner cares about").unwrap();
    std::fs::set_permissions(&bait, std::fs::Permissions::from_mode(0o644)).unwrap();

    let channels = site.root().join("channels");
    std::fs::create_dir_all(&channels).unwrap();
    std::os::unix::fs::symlink(&bait, channels.join("peer")).unwrap();

    site.keep(
        "peer",
        &Channel {
            secret: Secret::from_bytes([7; 32]),
            root: Signer::from_seed(&[3; 32]).handle(),
            introduction: Signer::from_seed(&[2; 32]).handle(),
            locator: "./drops".to_owned(),
            standing: Standing::Root,
            peer: None,
        },
    )
    .unwrap();

    let after = std::fs::metadata(&bait).unwrap();
    assert_eq!(
        std::fs::read(&bait).unwrap(),
        b"something its owner cares about",
        "the write followed a link and overwrote a file outside the site"
    );
    assert_eq!(
        after.permissions().mode() & 0o777,
        0o644,
        "the mode of a file outside the site was changed to {:o}",
        after.permissions().mode()
    );

    // And the record itself is now a real file of this site's own.
    let record = std::fs::symlink_metadata(channels.join("peer")).unwrap();
    assert!(!record.file_type().is_symlink(), "the link is still there");
    assert_eq!(record.permissions().mode() & 0o077, 0);
    assert!(site.channel("peer").is_ok());

    std::fs::remove_file(&bait).ok();
    std::fs::remove_dir_all(site.root()).unwrap();
}
