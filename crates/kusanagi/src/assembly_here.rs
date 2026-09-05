// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What this machine is doing with what this endpoint holds.
//!
//! Apart from `assembly.rs` because that router is at its line limit: the
//! `doctor --here` report answers questions about this side only — where the
//! site is, how its records are sealed, whether a proxy is set, and what this
//! binary hashes to — and needs nothing from the network.

use kusanagi_site::Site;

use kusanagi_door::{Complaint, Outcome};

pub(crate) fn here(site: &Site) -> Result<Outcome, Complaint> {
    let root = site.root();
    Ok(Outcome::Here {
        site: root.display().to_string(),
        under_profile: under_profile(root),
        at_rest: kusanagi_vault::store(),
        // Whether, never what. A proxy address says which network somebody
        // trusts, which is the kind of fact this report exists to protect.
        proxy: std::env::var("KUSANAGI_PROXY").is_ok_and(|set| !set.trim().is_empty()),
        binary: binary_hash()?,
    })
}

/// Whether the site sits under this user's profile directory.
///
/// `None` where the question has no meaning, which is every platform whose
/// default root is not chosen for the access control list it inherits.
#[cfg(windows)]
fn under_profile(root: &std::path::Path) -> Option<bool> {
    let profile = std::env::var("LOCALAPPDATA").ok()?;
    let here = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let profile = std::path::Path::new(&profile);
    let profile = profile
        .canonicalize()
        .unwrap_or_else(|_| profile.to_owned());
    Some(here.starts_with(profile))
}

#[cfg(not(windows))]
const fn under_profile(_root: &std::path::Path) -> Option<bool> {
    None
}

/// The BLAKE3 of the file this process was started from.
///
/// BLAKE3 rather than SHA-256 because this workspace already hashes with it
/// everywhere else, and a verification step that needs no second tool is worth
/// more than matching what `sha256sum` happens to print.
fn binary_hash() -> Result<String, Complaint> {
    let path = std::env::current_exe().map_err(|source| Complaint::Local {
        action: "find the running binary",
        source,
    })?;
    let bytes = std::fs::read(&path).map_err(|source| Complaint::Local {
        action: "read the running binary",
        source,
    })?;
    Ok(kusanagi_kernel::Hex(blake3::hash(&bytes).as_bytes()).to_string())
}
