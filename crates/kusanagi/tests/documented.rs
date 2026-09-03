// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The numbers in the documents, taken from the code that decides them.
//!
//! A drop was 4 096 bytes for most of this project's life. It is 131 072 now,
//! and four documents went on saying 4 096 for a whole release cycle because
//! nothing connected the sentence to the constant. **A number written in two
//! places has two authorities, and one of them is wrong as soon as anybody edits
//! the other.**
//!
//! So the sentences are checked rather than remembered: the two numbers a reader
//! is told about are formatted here from `seal::DROP` and `kernel::MAX_PAYLOAD`,
//! and the documents must contain those and not their predecessors.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    reason = "test code"
)]

use std::path::{Path, PathBuf};

/// The documents a reader is expected to believe.
const DOCUMENTS: [&str; 5] = [
    "README.md",
    "README.zh-CN.md",
    "ARCHITECTURE.md",
    "docs/box-protocol.md",
    "docs/joining.md",
];

/// The workspace root, from where this crate sits in it.
fn workspace() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("the workspace root is not where this crate says it is")
}

/// A number the way these documents write one: groups of three, thin spaces.
///
/// `131072` becomes `131 072`. The separator is what the prose already uses, so
/// this is the spelling the check is against rather than a second convention.
fn spelled(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::new();
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(' ');
        }
        out.push(digit);
    }
    out
}

/// Every document, read once.
fn documents() -> Vec<(&'static str, String)> {
    let root = workspace();
    DOCUMENTS
        .iter()
        .map(|name| {
            let text = std::fs::read_to_string(root.join(name))
                .unwrap_or_else(|_| panic!("{name} is missing"));
            (*name, text)
        })
        .collect()
}

#[test]
fn the_size_of_a_drop_is_the_size_the_code_gives_it() {
    let said = spelled(u64::try_from(kusanagi_seal::DROP).unwrap_or(u64::MAX));
    let mentions: Vec<&str> = documents()
        .into_iter()
        .filter(|(_, text)| text.contains(&said))
        .map(|(name, _)| name)
        .collect();
    assert!(
        mentions.len() >= 3,
        "only {mentions:?} say that a drop is {said} bytes"
    );
}

#[test]
fn no_document_still_says_what_a_drop_used_to_be() {
    // Every size this drop has ever had. A number that leaves the code has to
    // leave the prose in the same change, and the way to hold that is to name
    // the old value rather than to hope somebody greps for it.
    const RETIRED: [&str; 3] = ["4 096 bytes", "4096 bytes", "64 KiB"];
    for (name, text) in documents() {
        for stale in RETIRED {
            assert!(
                !text.contains(stale),
                "{name} still says `{stale}`, which stopped being true"
            );
        }
    }
}

#[test]
fn the_largest_message_is_the_one_the_kernel_allows() {
    let said = spelled(u64::from(kusanagi_kernel::MAX_PAYLOAD));
    let found = documents()
        .into_iter()
        .any(|(_, text)| text.contains(&said));
    assert!(
        found,
        "no document says a message is capped at {said} bytes"
    );
}

#[test]
fn every_port_in_the_documents_is_the_one_a_host_takes_by_default() {
    let port = kusanagi::HOST_ADDRESS
        .rsplit(':')
        .next()
        .expect("the default host address has no port");
    for (name, text) in documents() {
        // 8443 is `pcsync-https` in the IANA registry and is held in practice by
        // Tomcat, ingress controllers and development proxies. A document that
        // still shows it teaches the collision this default exists to avoid.
        assert!(
            !text.contains(":8443"),
            "{name} still shows port 8443, which this host stopped taking"
        );
        for line in text.lines().filter(|line| line.contains("kusanagi host")) {
            assert!(
                !line.contains(':') || line.contains(port),
                "{name} starts a host on a port that is not the default: {line}"
            );
        }
    }
}

#[test]
fn the_invitation_in_the_readme_carries_the_suite_this_build_speaks() {
    // Version 2, suite 1 — `0201`. Both bytes have moved once, and each move
    // left an example behind in prose for a release: `0100` was Ed25519, and
    // `kusanagi1:` was the format that carried a grant chain in the line.
    for (name, text) in documents() {
        for retired in ["kusanagi1:", "kusanagi2:0100", "kusanagi2:0101"] {
            assert!(
                !text.contains(retired),
                "{name} shows an invitation this build refuses: `{retired}`"
            );
        }
    }
}
