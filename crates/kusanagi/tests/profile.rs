// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where a site lands when nobody says where.
//!
//! A relative default put an identity, every channel key and every cairn in
//! whatever directory the program happened to be started from — for an agent,
//! the repository it is editing, a folder a sync client uploads, or a directory
//! shared with somebody else. Nothing about that is visible at the moment it
//! happens, which is why it is asserted here rather than remembered.
//!
//! Driven through the binary rather than the library, because the default is a
//! property of the command line and a library caller passes a path.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use std::path::Path;
use std::process::{Command, Output};

use common::scratch;

/// The variable that names the profile directory on this platform.
///
/// One name per platform, and only this machine's is asserted: `assembly.rs`
/// compiles one branch per platform and CI compiles the others, which is as far
/// as a claim about a machine nobody ran can honestly go.
#[cfg(windows)]
const PROFILE: &str = "LOCALAPPDATA";

/// What the site directory is called under the profile directory.
#[cfg(windows)]
const UNDER: &str = "kusanagi";

/// Runs the binary with no `--root` at all, under a profile of our choosing.
#[cfg(windows)]
fn without_root(profile: Option<&Path>, arguments: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_kusanagi"));
    command.arg("--json").args(arguments);
    match profile {
        Some(path) => command.env(PROFILE, path),
        None => command.env_remove(PROFILE),
    };
    command.output().expect("the binary would not start")
}

#[cfg(windows)]
#[test]
fn a_site_nobody_placed_lands_under_this_user_s_profile_and_not_in_the_current_directory() {
    let profile = scratch("profile-default");
    std::fs::create_dir_all(&profile).unwrap();

    let output = without_root(Some(&profile), &["id"]);
    assert!(
        output.status.success(),
        "id failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let expected = profile.join(UNDER).join("identity");
    assert!(
        expected.exists(),
        "no identity at {}; the default is not the profile directory",
        expected.display()
    );
    // The other half of the claim: nothing was written beside the process.
    assert!(
        !Path::new(".kusanagi").exists(),
        "a site appeared in the working directory"
    );

    std::fs::remove_dir_all(&profile).ok();
}

#[cfg(windows)]
#[test]
fn a_machine_that_will_not_say_where_data_lives_is_asked_rather_than_guessed() {
    let output = without_root(None, &["id"]);

    assert!(
        !output.status.success(),
        "the program invented a place to keep an identity"
    );
    let said = String::from_utf8_lossy(&output.stderr);
    let answer: serde_json::Value =
        serde_json::from_str(said.trim()).expect("a failure is still JSON when --json was asked");
    assert_eq!(answer["code"], "kusanagi.no_root");
    assert!(
        answer["recover"]
            .as_str()
            .unwrap_or_default()
            .contains("--root"),
        "the way out does not name the flag that is the way out: {answer}"
    );
}
