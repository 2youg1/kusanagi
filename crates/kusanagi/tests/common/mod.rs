// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What the acceptance tests share: an endpoint, a scratch directory, and the
//! ability to look at a host the way a host operator would.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::string_slice,
    dead_code,
    reason = "test code"
)]

use std::path::{Path, PathBuf};

use kusanagi::{Complaint, Fence, Outcome, Request, Site};

/// A fence with a value nobody has to look up.
///
/// JSON ignores it, and the tests that assert on prose want a fence they can
/// spell. What must be random in production is asserted by `payload.rs`.
pub const FENCE: Fence = Fence::from_bytes([0; 8]);

/// A directory nothing else is using.
pub fn scratch(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!("kusanagi-it-{}-{tag}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    root
}

/// One endpoint: a site directory and the verbs that act on it.
pub struct Endpoint {
    site: Site,
}

impl Endpoint {
    pub fn new(root: PathBuf) -> Self {
        Self {
            site: Site::at(root),
        }
    }

    pub fn run(&self, request: &Request) -> Result<Outcome, Complaint> {
        kusanagi::run(&self.site, request)
    }

    pub fn site_root(&self) -> &Path {
        self.site.root()
    }

    pub fn handle(&self) -> String {
        json(&self.run(&Request::Identity).unwrap())["handle"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    pub fn send(&self, channel: &str, text: &str) {
        self.send_reporting(channel, text);
    }

    /// Sends, and reports the address the segment was left at.
    pub fn send_reporting(&self, channel: &str, text: &str) -> String {
        let outcome = self
            .run(&Request::Send {
                name: channel.to_owned(),
                payload: text.as_bytes().to_vec(),
            })
            .unwrap_or_else(|error| panic!("send failed: {}", error.render(false)));
        json(&outcome)["address"].as_str().unwrap().to_owned()
    }
}

/// Renders an outcome as JSON so that a test asserts on fields rather than prose.
pub fn json(outcome: &Outcome) -> serde_json::Value {
    serde_json::from_str(&outcome.render(true, FENCE)).expect("an outcome did not render as JSON")
}

/// Opens a channel and returns the invitation line.
pub fn invite_line(endpoint: &Endpoint, name: &str, waypoint: &str) -> String {
    json(
        &endpoint
            .run(&Request::Invite {
                name: name.to_owned(),
                waypoint: waypoint.to_owned(),
                lifetime: 3_600,
                abilities: kusanagi_grant::Abilities::ALL,
                habit: kusanagi::Habit::default(),
            })
            .unwrap(),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned()
}

/// Two endpoints introduced to each other on one host, both with `habit`.
///
/// Both ends are given the same habit because a test that set only one would be
/// testing two things at once. Nothing in the protocol requires them to agree:
/// how often an endpoint writes and what it keeps are its own decisions, and
/// `channel.rs` says why.
pub fn pair_with(tag: &str, habit: kusanagi::Habit) -> (Endpoint, Endpoint, PathBuf) {
    let ground = scratch(tag);
    let host = ground.join("host");
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    let invitation = json(
        &alice
            .run(&Request::Invite {
                name: "bob".to_owned(),
                waypoint: host.display().to_string(),
                lifetime: 3_600,
                abilities: kusanagi_grant::Abilities::ALL,
                habit,
            })
            .unwrap(),
    )["invite"]
        .as_str()
        .unwrap()
        .to_owned();
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit,
    })
    .unwrap();
    (alice, bob, host)
}

/// The same, writing when asked and keeping everything.
pub fn pair(tag: &str) -> (Endpoint, Endpoint, PathBuf) {
    pair_with(tag, kusanagi::Habit::default())
}

/// How many objects a host holds, which is what an observer counts.
pub fn drops(host: &Path) -> usize {
    stored(host).len()
}

/// Everything a host holds: the address it is filed under, and the bytes.
pub fn stored(host: &Path) -> Vec<(String, Vec<u8>)> {
    let mut found = Vec::new();
    collect(host, &mut found);
    found.sort();
    found
}

fn collect(dir: &Path, into: &mut Vec<(String, Vec<u8>)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) == Some(".staging") {
                continue;
            }
            collect(&path, into);
        } else if let Ok(bytes) = std::fs::read(&path) {
            // The file is named by the address; the directories above it are the
            // bin it is filed in, which `object_path` is the one reader of.
            let address = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default()
                .to_owned();
            into.push((address, bytes));
        }
    }
}

/// Where the object at `address` sits under `host`, whatever bin it is in.
///
/// Found by name rather than computed, because the bin an object is filed in is
/// the endpoint's business and a test that recomputed it would be a second
/// authority for the key layout — one that goes stale silently.
///
/// # Panics
///
/// When nothing under `host` is named `address`, which for every caller here
/// means the write under test never landed.
pub fn object_path(host: &Path, address: &str) -> std::path::PathBuf {
    fn look(dir: &Path, address: &str) -> Option<std::path::PathBuf> {
        for entry in std::fs::read_dir(dir).ok()?.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = look(&path, address) {
                    return Some(found);
                }
            } else if path.file_name().and_then(|name| name.to_str()) == Some(address) {
                return Some(path);
            }
        }
        None
    }
    look(host, address).unwrap_or_else(|| panic!("the host holds nothing at {address}"))
}

/// Flips one bit of the object stored at `address`, the way a hostile host would.
pub fn flip_one_byte(host: &Path, address: &str) {
    let path = object_path(host, address);
    let mut bytes =
        std::fs::read(&path).unwrap_or_else(|error| panic!("nothing stored at {address}: {error}"));
    let byte = bytes.first_mut().expect("an empty object was stored");
    *byte ^= 0x01;
    std::fs::write(&path, &bytes).unwrap();
}
