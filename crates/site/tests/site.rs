// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a site must do with the disk, asserted from outside the crate.
//!
//! These lived beside the code until `site.rs` reached the `.rs` limit in
//! `ARCHITECTURE.md` §5. They were already written against the public interface,
//! so the move costs nothing: an identity is still written once, a name that
//! could escape the directory is still refused, and a forgotten channel still
//! frees its name while leaving the revocation list alone.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use kusanagi_grant::StepId;
use kusanagi_kernel::Signer;
use kusanagi_kernel::Ward;
use kusanagi_seal::Secret;
use kusanagi_site::{Channel, Site, SiteError, Standing};

fn scratch(tag: &str) -> Site {
    let root = std::env::temp_dir().join(format!("kusanagi-site-{}-{tag}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    Site::at(root)
}

fn channel(name: &str) -> Channel {
    let root = Signer::from_seed(&[1; 32]);
    Channel {
        cadence: kusanagi_site::Cadence::OnDemand,
        retention: kusanagi_site::Retention::Keep,
        opened: kusanagi_kernel::Period::from_count(0),
        name: name.to_owned(),
        secret: Secret::from_bytes([7; 32]),
        root: root.handle(),
        introduction: Signer::from_seed(&[2; 32]).verifying_key(),
        locator: "./drops".to_owned(),
        standing: Standing::Root,
        peer: None,
    }
}

#[test]
fn an_identity_is_written_once_and_read_back() {
    let site = scratch("identity");
    assert!(site.identity().unwrap().is_none());

    let first = site
        .adopt(&[5; 32], Ward::from_bits(0x00ab))
        .unwrap()
        .handle();
    assert_eq!(site.identity().unwrap().unwrap().handle(), first);

    // a second adoption must not silently replace the first
    assert_eq!(
        site.adopt(&[6; 32], Ward::from_bits(0x00ab))
            .unwrap()
            .handle(),
        first
    );
    std::fs::remove_dir_all(site.root()).unwrap();
}

#[test]
fn channels_are_kept_and_listed() {
    let site = scratch("channels");
    assert!(site.names().unwrap().is_empty());
    assert!(!site.holds("alice").unwrap());
    site.adopt(&[5; 32], Ward::from_bits(0x00ab)).unwrap();

    site.keep(&channel("alice")).unwrap();
    site.keep(&channel("bob")).unwrap();
    assert_eq!(site.names().unwrap(), vec!["alice", "bob"]);
    assert!(site.holds("alice").unwrap());
    assert_eq!(site.channel("alice").unwrap().locator, "./drops");
    std::fs::remove_dir_all(site.root()).unwrap();
}

/// A listing of the directory says how many channels there are, and no more.
///
/// The file used to be called `alice`, so any account that could read the
/// directory read the relationship graph off it without opening anything. That
/// is a different size of harm from a count, and it is the one this network's
/// derived addresses exist to prevent.
#[test]
fn nothing_in_the_directory_is_readable_as_a_peer_name() {
    let site = scratch("filed");
    site.adopt(&[5; 32], Ward::from_bits(0x00ab)).unwrap();
    site.keep(&channel("alice")).unwrap();
    site.keep(&channel("bob")).unwrap();

    let filed: Vec<String> = std::fs::read_dir(site.root().join("channels"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(filed.len(), 2, "a channel is one file: {filed:?}");
    for name in &filed {
        assert!(!name.contains("alice") && !name.contains("bob"), "{name}");
        assert!(
            name.len() == 64 && name.chars().all(|c| c.is_ascii_hexdigit()),
            "a filed name is a hash and nothing else: {name}"
        );
    }

    // A second site with a different identity files the same name elsewhere,
    // so the hash cannot be looked up in a table somebody built once.
    let other = scratch("filed-other");
    other.adopt(&[6; 32], Ward::from_bits(0x00ab)).unwrap();
    other.keep(&channel("alice")).unwrap();
    let elsewhere: Vec<String> = std::fs::read_dir(other.root().join("channels"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    let one = elsewhere.first().expect("a channel was kept and not filed");
    assert!(!filed.contains(one), "two sites filed one name alike");

    std::fs::remove_dir_all(site.root()).unwrap();
    std::fs::remove_dir_all(other.root()).unwrap();
}

/// A channel cannot be written before there is an identity to file it under.
#[test]
fn a_channel_needs_an_identity_to_be_filed_under() {
    let site = scratch("no-identity");
    assert!(matches!(
        site.keep(&channel("alice")),
        Err(SiteError::NoIdentity)
    ));
    assert!(site.names().unwrap().is_empty());
    assert!(!site.holds("alice").unwrap());
}

#[test]
fn an_unknown_channel_is_named_as_such() {
    let site = scratch("unknown");
    assert!(matches!(
        site.channel("nobody"),
        Err(SiteError::UnknownChannel { .. })
    ));
}

#[test]
fn a_name_that_could_escape_the_directory_is_refused() {
    let site = scratch("names");
    // `-` and anything starting with it are refused for a second reason: every
    // command line reads a leading hyphen as a flag, and this program reads a
    // bare `-` as "the name arrives on stdin". A name nobody can type is a name
    // nobody should be allowed to take.
    for bad in [
        "../escape",
        "with/slash",
        "Upper",
        "",
        "with space",
        "-",
        "-lead",
    ] {
        assert!(
            matches!(site.channel(bad), Err(SiteError::BadName { .. })),
            "`{bad}` was accepted as a channel name"
        );
    }
}

#[test]
fn a_forgotten_channel_leaves_no_trace_and_frees_its_name() {
    let site = scratch("forget");
    site.adopt(&[5; 32], Ward::from_bits(0x00ab)).unwrap();
    site.keep(&channel("alice")).unwrap();
    site.forget("alice").unwrap();

    assert!(!site.holds("alice").unwrap());
    assert!(site.names().unwrap().is_empty());
    assert!(matches!(
        site.forget("alice"),
        Err(SiteError::UnknownChannel { .. })
    ));
    // the name is free again, which is what makes forgetting a way out
    site.keep(&channel("alice")).unwrap();
    assert!(site.holds("alice").unwrap());
    std::fs::remove_dir_all(site.root()).unwrap();
}

#[test]
fn revocations_accumulate_and_survive() {
    let site = scratch("revoked");
    assert!(site.revocations().unwrap().is_empty());

    site.revoke(StepId::from_bytes([1; 32])).unwrap();
    site.revoke(StepId::from_bytes([2; 32])).unwrap();
    site.revoke(StepId::from_bytes([1; 32])).unwrap();

    let revoked = site.revocations().unwrap();
    assert_eq!(revoked.len(), 2);
    assert!(revoked.contains(&StepId::from_bytes([1; 32])));
    std::fs::remove_dir_all(site.root()).unwrap();
}
