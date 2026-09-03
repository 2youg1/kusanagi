// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The catalogue of error codes, kept honest by a machine.
//!
//! A stable code is a promise to whoever wrote a script against it, and a
//! promise nobody can read is not one. `docs/codes.md` is where a caller looks;
//! this is what stops it from drifting away from the code that produces it.
//!
//! **The code is the authority and the document is the mirror.** Adding a code
//! without a row, or deleting a row without deleting the code, turns the build
//! red and prints the difference. It is a documentation test in the only form
//! that survives: one that fails.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The layers that publish codes. A tenth prefix is a decision, not a typo.
const PREFIXES: [&str; 10] = [
    "kusanagi", "locator", "waypoint", "grant", "segment", "seal", "site", "chain", "cairn", "box",
];

/// The workspace root, from where this test's crate sits in it.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is not where this crate says it is")
}

/// Whether `text` is spelled the way a code is spelled.
fn is_code(text: &str) -> bool {
    let Some((prefix, rest)) = text.split_once('.') else {
        return false;
    };
    PREFIXES.contains(&prefix)
        && !rest.is_empty()
        && rest
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte == b'_')
}

/// Every code literal in one file.
///
/// String literals are the odd pieces when a Rust file is split on its quotes.
/// That is crude, and it does not matter: a piece only counts when it is spelled
/// exactly like a code, and no other construct in this workspace is.
fn codes_in(source: &str) -> BTreeSet<String> {
    source
        .split('"')
        .skip(1)
        .step_by(2)
        .filter(|piece| is_code(piece))
        .map(ToOwned::to_owned)
        .collect()
}

/// Every `.rs` file under `crates/*/src/`.
fn sources(root: &Path) -> Vec<PathBuf> {
    fn walk(path: &Path, found: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, found);
            } else if path.extension().is_some_and(|kind| kind == "rs") {
                found.push(path);
            }
        }
    }
    let mut found = Vec::new();
    for crate_dir in std::fs::read_dir(root.join("crates"))
        .expect("there is no crates directory")
        .flatten()
    {
        walk(&crate_dir.path().join("src"), &mut found);
    }
    found
}

/// Every code the catalogue lists, read out of its first column.
fn catalogued(document: &str) -> BTreeSet<String> {
    document
        .lines()
        .filter_map(|line| line.strip_prefix("| `"))
        .filter_map(|line| line.split_once('`'))
        .map(|(code, _)| code.to_owned())
        .filter(|code| is_code(code))
        .collect()
}

#[test]
fn the_catalogue_lists_exactly_the_codes_this_workspace_publishes() {
    let root = workspace();
    let files = sources(&root);
    assert!(
        files.len() > 20,
        "only {} source files were found",
        files.len()
    );

    let mut published = BTreeSet::new();
    for file in &files {
        let source = std::fs::read_to_string(file).expect("a source file could not be read");
        published.extend(codes_in(&source));
    }
    assert!(!published.is_empty(), "no codes were found in the source");

    let document =
        std::fs::read_to_string(root.join("docs/codes.md")).expect("docs/codes.md is missing");
    let listed = catalogued(&document);

    let missing: Vec<&String> = published.difference(&listed).collect();
    let stale: Vec<&String> = listed.difference(&published).collect();
    assert!(
        missing.is_empty() && stale.is_empty(),
        "docs/codes.md disagrees with the source.\n  \
         in the code and not in the document: {missing:?}\n  \
         in the document and not in the code: {stale:?}"
    );
}

#[test]
fn every_row_says_when_and_how_to_recover() {
    let document = std::fs::read_to_string(workspace().join("docs/codes.md"))
        .expect("docs/codes.md is missing");
    for line in document.lines().filter(|line| line.starts_with("| `")) {
        let columns: Vec<&str> = line.trim_matches('|').split('|').collect();
        assert_eq!(columns.len(), 3, "a row has the wrong shape: {line}");
        for column in &columns {
            assert!(column.trim().len() > 3, "a column is empty in: {line}");
        }
    }
}
