// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Where this endpoint keeps its things when nobody says.
//!
//! Apart from `assembly.rs` because it answers one question the operating system
//! owns, and it answers it once per platform. Everything here is a `#[cfg]`
//! branch over one environment variable; the branch that is not this machine's
//! is compiled by CI on a machine that is, and asserted by nothing here — a claim
//! about a platform nobody ran is not evidence.
//!
//! **This is the only file besides `assembly.rs` allowed to read the
//! environment**, and it reads exactly the variables that name a home
//! directory.

use std::path::PathBuf;

use kusanagi_door::Complaint;

/// Where an endpoint keeps its site when nobody names a directory.
///
/// **A relative path was the wrong default.** It put the identity, the channel
/// keys and every cairn in whatever directory the program was started from —
/// which for an agent is the repository it is working in, a folder a sync client
/// uploads, or a directory somebody shares. The profile directory is where the
/// operating system already keeps per-user state, and on Windows it is also the
/// cheapest half of the file permissions this design owes: what `%LOCALAPPDATA%`
/// inherits admits the owner, `SYSTEM` and administrators and nobody else, so a
/// second standard account on the machine cannot read a site today.
///
/// One environment variable per platform, read here because this module is the
/// only one allowed to read any. The branch that is not this machine's is
/// compiled by CI on a machine that is, and asserted by nothing here — a claim
/// about a platform nobody ran is not evidence.
///
/// # Errors
///
/// [`Complaint::NoRoot`] when the variable that names the profile is absent,
/// which is the one case where this program cannot guess and must be told.
#[cfg(windows)]
pub fn default_root() -> Result<PathBuf, Complaint> {
    beneath("LOCALAPPDATA", "kusanagi")
}

/// Where an endpoint keeps its site when nobody names a directory.
///
/// # Errors
///
/// [`Complaint::NoRoot`] when `HOME` is absent.
#[cfg(target_os = "macos")]
pub fn default_root() -> Result<PathBuf, Complaint> {
    beneath("HOME", "Library/Application Support/kusanagi")
}

/// Where an endpoint keeps its site when nobody names a directory.
///
/// # Errors
///
/// [`Complaint::NoRoot`] when neither `XDG_DATA_HOME` nor `HOME` is set.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn default_root() -> Result<PathBuf, Complaint> {
    beneath("XDG_DATA_HOME", "kusanagi").or_else(|_| beneath("HOME", ".local/share/kusanagi"))
}

/// `$<variable>/<tail>`, or a complaint naming the variable that was missing.
fn beneath(variable: &'static str, tail: &str) -> Result<PathBuf, Complaint> {
    let base = std::env::var(variable).map_err(|_| Complaint::NoRoot { variable })?;
    if base.trim().is_empty() {
        return Err(Complaint::NoRoot { variable });
    }
    Ok(PathBuf::from(base).join(tail))
}

/// Where `kusanagi host` keeps other people's drops when nobody says.
///
/// Beside the site rather than inside it: what a host holds is not this
/// endpoint's state, and a `forget` or a backup must not sweep up somebody
/// else's bytes.
///
/// # Errors
///
/// Whatever [`default_root`] reports.
pub fn default_host_dir() -> Result<PathBuf, Complaint> {
    let root = default_root()?;
    let mut named = root.clone().into_os_string();
    named.push("-host");
    Ok(PathBuf::from(named))
}
