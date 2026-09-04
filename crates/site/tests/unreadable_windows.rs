// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who else on this machine can read a channel secret, asked on Windows.
//!
//! `unreadable.rs` asks the same question in mode bits, which do not exist here.
//! The answer here is an access control list, and the two properties below are
//! separate tests because **they go green at different times**:
//!
//! 1. `nobody_else_is_named` is about where a site lands. Under
//!    `%LOCALAPPDATA%` the inherited list admits the owner, `SYSTEM` and
//!    administrators and nobody else, so this passes today and is what makes the
//!    default root a security property rather than tidiness.
//! 2. `the_protection_is_the_site_s_own` is about what a site asks for. Until
//!    `site::permissions::windows` exists, a site inherits its parent's list
//!    instead of carrying its own — so a site under a directory somebody opened
//!    up is open too. **This one is expected to fail until then**, which is why
//!    it is written now: a gap nobody can run is a gap nobody fixes.
//!
//! Identities are read as SIDs rather than as names. `icacls` prints group names
//! in the language the system was installed in, and a test that greps for
//! "Everyone" passes on a German machine for the wrong reason.

#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use kusanagi_grant::StepId;
use kusanagi_kernel::Signer;
use kusanagi_seal::Secret;
use kusanagi_site::{Channel, Site, Standing};

/// Identities that must never appear on anything a site writes.
///
/// Everyone, Users, Authenticated Users, Interactive, Guests. Well-known SIDs,
/// so this list means the same thing on every installation in every language.
const OUTSIDERS: [&str; 5] = [
    "S-1-1-0",
    "S-1-5-32-545",
    "S-1-5-11",
    "S-1-5-4",
    "S-1-5-32-546",
];

/// SYSTEM and OWNER RIGHTS, which a site's own list is allowed to carry.
const SYSTEM: &str = "S-1-5-18";
const OWNER_RIGHTS: &str = "S-1-3-4";

/// One access control entry, as this test needs to read it.
struct Ace {
    identity: String,
    inherited: bool,
}

/// Separates one path's answer from the next inside one PowerShell run.
const NEXT: &str = "--- next ---";

/// What the access control list on each path says, plus whether it is protected.
///
/// PowerShell rather than a Win32 call, because a test may spawn anything and
/// the crate under test is not allowed to contain the code that answers this.
///
/// **Every path in one run.** Starting PowerShell costs about a fifth of a
/// second and the whole suite runs its tests at once, so a call per file used to
/// fail intermittently with an empty error stream — an interpreter that would
/// not start, reported as an access control list that was wrong. One run per
/// test cannot say that.
fn acls(paths: &[PathBuf]) -> Vec<(bool, Vec<Ace>)> {
    let listed = paths
        .iter()
        .map(|path| format!("'{}'", path.display().to_string().replace('\'', "''")))
        .collect::<Vec<String>>()
        .join(",");
    let script = format!(
        "$ErrorActionPreference='Stop'; foreach ($p in @({listed})) {{ \
           Write-Output '{NEXT}'; \
           $a = Get-Acl -LiteralPath $p; \
           Write-Output $a.AreAccessRulesProtected; \
           $a.Access | ForEach-Object {{ \
             Write-Output ($_.IdentityReference.Translate(\
               [System.Security.Principal.SecurityIdentifier]).Value \
               + ' ' + $_.IsInherited) }} }}"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .expect("powershell would not start");
    assert!(
        output.status.success(),
        "reading the access control lists failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let said = String::from_utf8_lossy(&output.stdout);
    let answers: Vec<(bool, Vec<Ace>)> = said
        .split(NEXT)
        .skip(1)
        .map(|block| {
            let mut lines = block.lines().map(str::trim).filter(|line| !line.is_empty());
            let protected = lines.next().unwrap_or("False").eq_ignore_ascii_case("true");
            let entries = lines
                .filter_map(|line| {
                    let (identity, inherited) = line.rsplit_once(' ')?;
                    Some(Ace {
                        identity: identity.trim().to_owned(),
                        inherited: inherited.trim().eq_ignore_ascii_case("true"),
                    })
                })
                .collect();
            (protected, entries)
        })
        .collect();
    assert_eq!(
        answers.len(),
        paths.len(),
        "powershell answered for {} of {} paths",
        answers.len(),
        paths.len()
    );
    answers
}

/// This account's own SID.
fn me() -> String {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "[System.Security.Principal.WindowsIdentity]::GetCurrent().User.Value",
        ])
        .output()
        .expect("powershell would not start");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// A site with one identity, one channel record and one revocation in it.
///
/// Under `%TEMP%`, which is inside `%LOCALAPPDATA%` — the same place the default
/// root now goes, so what is measured here is what a real site gets.
fn written(tag: &str) -> Site {
    let root = std::env::temp_dir().join(format!("kusanagi-acl-{}-{tag}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let site = Site::at(root);
    site.adopt(&[5; 32]).unwrap();
    site.keep(&Channel {
        cadence: kusanagi_site::Cadence::OnDemand,
        retention: kusanagi_site::Retention::Keep,
        name: "peer".to_owned(),
        secret: Secret::from_bytes([7; 32]),
        root: Signer::from_seed(&[3; 32]).handle(),
        introduction: Signer::from_seed(&[2; 32]).verifying_key(),
        locator: "./drops".to_owned(),
        standing: Standing::Root,
        peer: None,
    })
    .unwrap();
    site.revoke(StepId::from_bytes([1; 32])).unwrap();
    site
}

/// Every file and directory under `root`, and `root` itself.
fn everything(root: &Path) -> Vec<PathBuf> {
    let mut found = vec![root.to_path_buf()];
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(everything(&path));
        } else {
            found.push(path);
        }
    }
    found
}

#[test]
fn nobody_else_is_named() {
    let site = written("outsiders");
    let paths = everything(site.root());
    assert!(paths.len() > 3, "the site wrote nothing to check");

    for (path, (_, entries)) in paths.iter().zip(acls(&paths)) {
        for ace in &entries {
            assert!(
                !OUTSIDERS.contains(&ace.identity.as_str()),
                "{} names {}, so another account on this machine can read a \
                 channel secret from it",
                path.display(),
                ace.identity
            );
        }
    }

    std::fs::remove_dir_all(site.root()).ok();
}

#[test]
fn the_protection_is_the_site_s_own() {
    let site = written("protected");
    let mine = me();
    assert!(
        mine.starts_with("S-1-"),
        "could not read this account's sid"
    );

    let paths = everything(site.root());
    for (path, (protected, entries)) in paths.iter().zip(acls(&paths)) {
        assert!(
            protected,
            "{} inherits its access control list, so it is as open as whatever \
             directory it happens to sit in",
            path.display()
        );
        for ace in &entries {
            assert!(
                !ace.inherited,
                "{} carries an inherited entry for {}",
                path.display(),
                ace.identity
            );
            assert!(
                ace.identity == mine || ace.identity == SYSTEM || ace.identity == OWNER_RIGHTS,
                "{} names {}, which is neither this account, SYSTEM, nor OWNER RIGHTS",
                path.display(),
                ace.identity
            );
        }
    }

    std::fs::remove_dir_all(site.root()).ok();
}
