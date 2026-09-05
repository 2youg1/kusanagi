// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The contract every waypoint must satisfy.
//!
//! This is a function rather than a set of `#[test]`s for two reasons that both
//! matter. An adapter written outside this repository — a company's own object
//! store — must be able to run the same clauses, and `kusanagi doctor` must be
//! able to run them against a **live host** and print which clause failed. A
//! failing clause is therefore data, not a panic.
//!
//! Clause names are published identifiers: they will appear in `doctor` output,
//! so renaming one is a change to a public interface.

use core::fmt;

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};
use kusanagi_seal::{Fit, Stream, derive, seal};

/// A clause the waypoint under test did not satisfy.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Failure {
    /// A clause produced the wrong answer.
    #[error("clause `{clause}` failed: {detail}")]
    Clause {
        /// The published name of the clause.
        clause: &'static str,
        /// What was expected and what happened instead.
        detail: String,
    },
    /// The waypoint failed outright while a clause was running.
    #[error("clause `{clause}` could not run: {source}")]
    Unavailable {
        /// The published name of the clause.
        clause: &'static str,
        /// The failure the waypoint reported.
        #[source]
        source: WaypointError,
    },
}

impl Failure {
    /// The clause that failed.
    #[must_use]
    pub const fn clause(&self) -> &'static str {
        match self {
            Self::Clause { clause, .. } | Self::Unavailable { clause, .. } => clause,
        }
    }
}

/// One clause of the contract, mid-execution.
struct Clause<'a> {
    name: &'static str,
    waypoint: &'a dyn Waypoint,
}

impl Clause<'_> {
    fn put(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, Failure> {
        self.waypoint
            .put_if_absent(addr, bytes)
            .map_err(|source| Failure::Unavailable {
                clause: self.name,
                source,
            })
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, Failure> {
        self.waypoint
            .get(addr)
            .map_err(|source| Failure::Unavailable {
                clause: self.name,
                source,
            })
    }

    fn delete(&self, addr: &DropAddr) -> Result<(), Failure> {
        self.waypoint
            .delete(addr)
            .map_err(|source| Failure::Unavailable {
                clause: self.name,
                source,
            })
    }

    fn require(&self, held: bool, detail: impl fmt::Display) -> Result<(), Failure> {
        if held {
            return Ok(());
        }
        Err(Failure::Clause {
            clause: self.name,
            detail: detail.to_string(),
        })
    }
}

/// Runs every clause against `waypoint`.
///
/// Addresses are derived from `namespace` exactly as real traffic is, so a
/// caller may point this at a live host without colliding with anything — pass a
/// stream derived from a secret nobody else holds.
///
/// # Errors
///
/// The first [`Failure`], naming the clause that broke.
pub fn run(waypoint: &impl Waypoint, namespace: &Stream) -> Result<(), Failure> {
    let addr = |step: u64| derive(namespace, step).0;
    let clause = |name| Clause { name, waypoint };
    let first = body(namespace, 1, b"first")?;
    let second = body(namespace, 1, b"second")?;
    let other = body(namespace, 2, b"other")?;
    let released = body(namespace, 4, b"released")?;

    let empty = clause("empty-address-reads-nothing");
    empty.require(
        empty.get(&addr(0))?.is_none(),
        "an address never written to returned bytes",
    )?;

    let stored = clause("write-then-read");
    stored.require(
        stored.put(&addr(1), &first)? == PutOutcome::Stored,
        "writing to an empty address did not report Stored",
    )?;
    stored.require(
        stored.get(&addr(1))?.as_deref() == Some(first.as_slice()),
        "the bytes read back differ from the bytes written",
    )?;

    // The load-bearing clause: everything above this file assumes a drop receives
    // exactly one segment, and a host that quietly overwrites breaks that silently
    // rather than loudly.
    let once = clause("write-once");
    once.require(
        once.put(&addr(1), &second)? == PutOutcome::AlreadyPresent,
        "a second write to an occupied address was not refused",
    )?;
    once.require(
        once.get(&addr(1))?.as_deref() == Some(first.as_slice()),
        "a second write replaced the bytes already at the address",
    )?;

    let independent = clause("addresses-are-independent");
    independent.require(
        independent.put(&addr(2), &other)? == PutOutcome::Stored,
        "a write to a fresh address was refused",
    )?;
    independent.require(
        independent.get(&addr(1))?.as_deref() == Some(first.as_slice()),
        "writing one address disturbed another",
    )?;

    // A channel that releases stakes its history on this clause: once the peer
    // has acknowledged a drop, the drop is removed and the reader's own site is
    // the only copy left. A host that quietly kept the bytes would leave that
    // channel believing in a deletion that never happened.
    let removal = clause("release-removes-and-stays-removed");
    removal.put(&addr(4), &released)?;
    removal.delete(&addr(4))?;
    removal.require(
        removal.get(&addr(4))?.is_none(),
        "a released drop was still readable afterwards",
    )?;
    removal.delete(&addr(4))?;
    removal.require(
        removal.get(&addr(4))?.is_none(),
        "releasing an address twice did not leave it empty",
    )?;

    Ok(())
}

/// A body shaped like every real drop: sealed under the step's own key and
/// padded to the veil, so a host running this contract sees traffic and never
/// a test. A probe of twenty-two plain bytes would tell a host, and anybody on
/// the path who measures sizes, exactly which program was talking to it.
fn body(namespace: &Stream, step: u64, text: &[u8]) -> Result<Vec<u8>, Failure> {
    let (_, key) = derive(namespace, step);
    seal(&key, Fit::Veil, text).map_err(|error| Failure::Clause {
        clause: "sealed-body",
        detail: error.to_string(),
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::run;
    use crate::{DirWaypoint, MemoryWaypoint};
    use kusanagi_kernel::Signer;
    use kusanagi_seal::{Secret, Stream};

    fn namespace() -> Stream {
        Secret::from_bytes([0x5a; 32]).stream(&Signer::from_seed(&[0x5a; 32]).handle())
    }

    #[test]
    fn the_memory_adapter_satisfies_the_contract() {
        run(&MemoryWaypoint::new(), &namespace()).expect("memory waypoint broke the contract");
    }

    #[test]
    fn the_directory_adapter_satisfies_the_contract() {
        // A root nobody else can be using, so the test needs no pre-cleanup and
        // can run beside its neighbours.
        let root = std::env::temp_dir().join(format!(
            "kusanagi-conformance-{}-{}",
            std::process::id(),
            line!()
        ));
        run(&DirWaypoint::new(&root), &namespace()).expect("directory waypoint broke the contract");
        std::fs::remove_dir_all(&root).expect("could not clean up the test root");
    }
}
