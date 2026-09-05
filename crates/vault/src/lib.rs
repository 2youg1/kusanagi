// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How a file is asked to belong to one account, on each platform that answers.
//!
//! A site holds an identity seed and every channel secret. Everything the
//! network's privacy rests on — which addresses exist, what is written at them,
//! who may write — follows from those bytes, so a site that any local account can
//! read is a site that any local account has joined. This crate is the one way
//! those bytes reach a disk and the one way they come back.
//!
//! The default is not good enough. A file created by `fs::write` on a typical
//! Unix system is `0644`; on Windows it inherits whatever list the parent
//! directory carries, which is the list of a directory this program did not
//! choose. Every user on the machine, every process in the container, every
//! layer of the image, and every backup that preserves permissions gets the
//! channel secret. Nothing in the threat model requires an attacker with root, a
//! stolen laptop or a seized disk — a second account on a shared build machine is
//! enough.
//!
//! **The permission is established at creation and never adjusted afterwards,
//! and that rule is a security property rather than an implementation detail.**
//! `set_permissions` and `SetNamedSecurityInfoW` both take a *path* and resolve
//! it, so adjusting a thing this build did not create hands anybody who can write
//! into a vault directory a way to aim that adjustment at a file its owner cares
//! about — through a symbolic link on Unix, through a junction on Windows.
//!
//! So a write that replaces a record replaces the **inode**: it is staged beside
//! the target and renamed over it, which acts on the name, so a link sitting
//! there is replaced rather than followed and a reader never sees half a record.
//! `waypoint::dir` makes a drop appear whole in the same shape.
//!
//! A directory this build did not create keeps the permissions it has. Every
//! file inside it is closed regardless, so such a vault exposes the set of file
//! names and nothing in them.
//!
//! **The platform difference is a file, not a branch.** [`files`] holds the part
//! that is the same everywhere — staging, renaming, and refusing to touch what it
//! did not create — and `unix.rs` and `windows.rs` each hold one answer to the
//! questions the platform actually decides: how a directory is created, how a
//! file is created, how a page is pinned, and which store seals bytes at rest. A
//! third platform is a third file and one line here.
//!
//! **Everything read off this disk comes back in a [`Locked`] buffer.** A record
//! kept here is a secret, and a secret in a page the operating system may evict
//! is a secret in `pagefile.sys` and in every backup that reached it. One funnel
//! means "which records need pinning" is not a list anybody has to remember: it
//! is every record, because there is one way in.
//!
//! **Why a crate rather than a module of `site`.** What lives here is not "what
//! an endpoint keeps on its disk" but "how the operating system is asked to keep
//! it", and it holds the whole of the workspace's platform matrix: the only
//! `unsafe`, the only `windows-sys` dependency, the only `cfg`-selected file
//! pair. Behind a crate boundary the suppression allowlist in the root
//! `Cargo.toml` names a crate instead of a module, which is the harder statement
//! of the same rule.

mod at_rest;
mod error;
mod files;
mod locked;

#[cfg(unix)]
#[path = "unix.rs"]
mod platform;
#[cfg(windows)]
#[path = "windows.rs"]
mod platform;

pub use at_rest::store;
pub use error::VaultError;
pub use files::{create_dir, read, write, write_new};
pub use locked::Locked;
