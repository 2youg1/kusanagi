// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A directory on this machine, used as a waypoint.
//!
//! This is the adapter the walking skeleton runs on, and it is also the one that
//! pins down what write-once has to mean everywhere else: a drop is claimed by an
//! operation that is atomic against a concurrent claimant and against a crash.

use std::fs::{self, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::str::FromStr as _;
use std::sync::atomic::{AtomicU64, Ordering};

use kusanagi_kernel::{
    Bin, DropAddr, Listing, Object, PutOutcome, Sweep, Ward, Waypoint, WaypointError,
};

use crate::client::MAX_LISTED;

/// Distinguishes staged files written by concurrent callers in one process.
static STAGING_COUNTER: AtomicU64 = AtomicU64::new(0);

/// The subdirectory holding half-written segments. Everything in it is scratch.
const STAGING_DIR: &str = ".staging";

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

    /// `root/period/ward/address`, which is [`Object`]'s own spelling as a path.
    ///
    /// The bin **is** the shard. An earlier tree split on two characters of the
    /// address to keep directories small; a period and a ward do that at least as
    /// well and are the layout every other adapter already has to use, so the one
    /// tree serves both jobs and `read_dir` answers a sweep with no index.
    fn path_of(&self, at: &Object) -> PathBuf {
        self.root
            .join(at.bin().period().to_string())
            .join(at.bin().ward().to_string())
            .join(at.addr().to_string())
    }

    /// The bin this directory name is, when the sweep asked for it.
    fn bin_of(sweep: &Sweep, name: &str) -> Option<Bin> {
        let ward = Ward::from_bits(u16::from_str_radix(name, 16).ok()?);
        let bin = Bin::new(sweep.period(), ward);
        sweep.covers(bin).then_some(bin)
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
    fn put_if_absent(&self, at: &Object, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        let path = self.path_of(at);
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

    /// Removes the drop, treating an address that holds nothing as done.
    ///
    /// The write side claims an address by hard-linking onto it, so the file
    /// here is the only link and removing it removes the drop.
    fn delete(&self, at: &Object) -> Result<(), WaypointError> {
        match fs::remove_file(self.path_of(at)) {
            Ok(()) => Ok(()),
            Err(source) if source.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(WaypointError::Io {
                action: "releasing a drop",
                source,
            }),
        }
    }

    fn get(&self, at: &Object) -> Result<Option<Vec<u8>>, WaypointError> {
        // Reading creates nothing: a read with a side effect would make polling an
        // empty address change the world.
        //
        // A path that is missing because no bin was ever filed reads as empty,
        // which is what an empty bin is. But a root that exists and is not a
        // directory is not an empty host, it is a broken one: the bin layout
        // puts intermediate components between the root and the drop, so their
        // absence can no longer tell the two apart and the root itself is asked.
        // Without this a host replaced by a file would read as silence, and a
        // reader would report "nothing arrived" about a host it cannot reach.
        let missing = |source| WaypointError::Io {
            action: "reading a drop",
            source,
        };
        match fs::read(self.path_of(at)) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(source) if source.kind() == io::ErrorKind::NotFound => {
                match fs::metadata(&self.root) {
                    Ok(seen) if !seen.is_dir() => Err(missing(source)),
                    _ => Ok(None),
                }
            }
            Err(source) => Err(missing(source)),
        }
    }
}

impl Listing for DirWaypoint {
    /// Walks the ward directories the sweep names and reports what is in them.
    ///
    /// A name that does not parse is skipped rather than reported: this tree may
    /// be a directory somebody else also uses, and a file that is not a drop is
    /// not this adapter's business. What *is* reported is a directory that
    /// cannot be read, because a bin a reader cannot see is a message a reader
    /// silently never gets.
    fn list(&self, sweep: &Sweep) -> Result<Vec<Object>, WaypointError> {
        let listing = |source| WaypointError::Io {
            action: "listing a bin",
            source,
        };
        let period = self.root.join(sweep.period().to_string());
        let wards = match fs::read_dir(&period) {
            Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => return Err(listing(source)),
            Ok(entries) => entries,
        };
        let mut found = Vec::new();
        for ward in wards {
            let ward = ward.map_err(listing)?;
            let Some(bin) = Self::bin_of(sweep, &ward.file_name().to_string_lossy()) else {
                continue;
            };
            for drop in fs::read_dir(ward.path()).map_err(listing)? {
                let name = drop.map_err(listing)?.file_name();
                let Ok(addr) = DropAddr::from_str(&name.to_string_lossy()) else {
                    continue;
                };
                found.push(Object::new(bin, addr));
                if found.len() >= MAX_LISTED {
                    return Ok(found);
                }
            }
        }
        Ok(found)
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
    use kusanagi_kernel::{Bin, DropAddr, Object, Period, PutOutcome, Ward, Waypoint};

    /// One object in a bin, for a test that cares about the address alone.
    fn at(byte: u8) -> Object {
        Object::new(
            Bin::new(Period::from_count(7), Ward::from_bits(0x00ab)),
            DropAddr::from_bytes([byte; 20]),
        )
    }

    fn fresh_root(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kusanagi-dir-{}-{tag}", std::process::id()))
    }

    #[test]
    fn the_tree_is_created_on_demand() {
        let root = fresh_root("on-demand");
        let waypoint = DirWaypoint::new(&root);
        let addr = at(0xa1);

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
        let addr = at(0xa1);

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
        let addr = at(0xa1);

        assert_eq!(
            waypoint.put_if_absent(&addr, b"x").unwrap_err().code(),
            "waypoint.io"
        );

        std::fs::remove_file(&root).unwrap();
    }

    #[test]
    fn exactly_one_of_many_racing_writers_stores() {
        let root = fresh_root("race");
        let addr = at(0xa1);

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
