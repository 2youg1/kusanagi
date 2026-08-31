// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2youg1 and the kusanagi contributors.

//! A directory on this machine, used as a waypoint.
//!
//! This is the adapter the walking skeleton runs on, and it is also the one that
//! pins down what write-once has to mean everywhere else: a drop is claimed by an
//! operation that is atomic against a concurrent claimant and against a crash.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};

/// Distinguishes staged files written by concurrent callers in one process.
static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The subdirectory holding half-written segments. Everything in it is scratch.
const STAGING_DIR: &str = ".staging";

/// How many leading characters of an address name its shard directory.
///
/// Addresses are already uniformly distributed, so the prefix is taken directly;
/// hashing an address to place it would be extra work, not extra safety.
const SHARD_WIDTH: usize = 2;

/// A directory tree used as a waypoint.
#[derive(Debug, Clone)]
pub struct DirWaypoint {
    root: PathBuf,
}

impl DirWaypoint {
    /// Uses `root` as the tree. The directory is created on first write.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn path_of(&self, addr: &DropAddr) -> Result<PathBuf, WaypointError> {
        let text = addr.to_string();
        let (shard, rest) =
            text.split_at_checked(SHARD_WIDTH)
                .ok_or_else(|| WaypointError::UnusableAddress {
                    reason: format!("address is shorter than its {SHARD_WIDTH}-character shard"),
                })?;
        Ok(self.root.join(shard).join(rest))
    }

    /// Writes `bytes` into a complete, uniquely named file and returns its path.
    ///
    /// Staging first is what makes the claim atomic: the file that eventually
    /// appears at the drop address is linked into place already whole, so a crash
    /// can leave rubbish in the staging directory but never half a segment at an
    /// address that can never be rewritten.
    fn stage(&self, bytes: &[u8]) -> Result<PathBuf, WaypointError> {
        let dir = self.root.join(STAGING_DIR);
        create_dir(&dir)?;
        let ticket = STAGING_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!("{}-{ticket}", std::process::id()));

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .map_err(|source| WaypointError::Io {
                action: "opening a staged segment",
                source,
            })?;
        file.write_all(bytes).map_err(|source| WaypointError::Io {
            action: "writing a staged segment",
            source,
        })?;
        file.sync_all().map_err(|source| WaypointError::Io {
            action: "flushing a staged segment",
            source,
        })?;
        Ok(path)
    }
}

fn create_dir(path: &Path) -> Result<(), WaypointError> {
    fs::create_dir_all(path).map_err(|source| WaypointError::Io {
        action: "creating a waypoint directory",
        source,
    })
}

fn discard(path: &Path) -> Result<(), WaypointError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(source) => Err(WaypointError::Io {
            action: "removing a staged segment",
            source,
        }),
    }
}

impl Waypoint for DirWaypoint {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        let path = self.path_of(addr)?;
        if let Some(parent) = path.parent() {
            create_dir(parent)?;
        }
        let staged = self.stage(bytes)?;

        // Hard-linking fails when the destination exists, so the claim and the
        // check are one operation rather than two with a race between them. Both
        // results are inspected below; neither is dropped.
        let linked = fs::hard_link(&staged, &path);
        let swept = discard(&staged);

        let outcome = match linked {
            Ok(()) => PutOutcome::Stored,
            Err(source) if source.kind() == io::ErrorKind::AlreadyExists => {
                PutOutcome::AlreadyPresent
            }
            Err(source) => {
                return Err(WaypointError::Io {
                    action: "claiming a drop",
                    source,
                });
            }
        };
        swept?;
        Ok(outcome)
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        // Reading creates nothing: a read with a side effect would make polling an
        // empty address change the world.
        match fs::read(self.path_of(addr)?) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(WaypointError::Io {
                action: "reading a drop",
                source,
            }),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::DirWaypoint;
    use kusanagi_kernel::{Handle, PutOutcome, Waypoint, public_v0};

    fn fresh_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kusanagi-dir-{}-{tag}", std::process::id()))
    }

    #[test]
    fn the_tree_is_created_on_demand() {
        let root = fresh_root("on-demand");
        let waypoint = DirWaypoint::new(&root);
        let addr = public_v0(&Handle::from_name("alice"), 0);

        assert!(
            !root.exists(),
            "the root existed before anything was written"
        );
        assert_eq!(
            waypoint.put_if_absent(&addr, b"x").unwrap(),
            PutOutcome::Stored
        );
        assert!(root.exists());

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reading_creates_nothing() {
        let root = fresh_root("read-only");
        let waypoint = DirWaypoint::new(&root);
        let addr = public_v0(&Handle::from_name("alice"), 0);

        assert!(waypoint.get(&addr).unwrap().is_none());
        assert!(
            !root.exists(),
            "a read created the tree; polling an empty address must not change the world"
        );
    }

    #[test]
    fn a_root_that_is_a_file_fails_loudly() {
        let root = fresh_root("root-is-a-file");
        std::fs::write(&root, b"not a directory").unwrap();
        let waypoint = DirWaypoint::new(&root);
        let addr = public_v0(&Handle::from_name("alice"), 0);

        assert_eq!(
            waypoint.put_if_absent(&addr, b"x").unwrap_err().code(),
            "waypoint.io"
        );

        std::fs::remove_file(&root).unwrap();
    }

    #[test]
    fn exactly_one_of_many_racing_writers_stores() {
        let root = fresh_root("race");
        let addr = public_v0(&Handle::from_name("alice"), 0);

        let stored = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8_u8)
                .map(|writer| {
                    let root = root.clone();
                    scope.spawn(move || {
                        DirWaypoint::new(root)
                            .put_if_absent(&addr, &[writer])
                            .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().expect("a racing writer panicked"))
                .filter(|outcome| *outcome == PutOutcome::Stored)
                .count()
        });

        assert_eq!(stored, 1, "a drop must be claimed exactly once");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
