// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Drops carried by somebody else's client, invoked rather than imitated.
//!
//! Every other adapter here speaks a protocol: `http.rs` writes HTTP, `s3.rs`
//! signs `SigV4`. A path observer therefore sees a TLS handshake with this
//! program's cipher list, this program's ALPN, this program's header order —
//! and `ARCHITECTURE.md` §3 admits property 0, *is this kusanagi at all*, as the
//! one property already lost to such an observer.
//!
//! **The way to stop looking like a program is to stop being the program that
//! talks.** This adapter runs the real client of whatever service holds the
//! drops — `rclone`, `aws`, `git`, a vendor's own tool — and lets it make the
//! connection. What crosses the network is then that client's traffic because it
//! *is* that client's traffic, down to the handshake, and the credentials belong
//! to it rather than to us: nothing here reads, holds or transmits them.
//!
//! ## The contract a carrier program satisfies
//!
//! ```text
//! PROGRAM get    OBJECT   bytes to stdout   0 = found, 3 = nothing there
//! PROGRAM put    OBJECT   bytes on stdin    0 = sent
//! PROGRAM delete OBJECT                     0 = gone, 3 = nothing there
//! ```
//!
//! **Three exit codes, not two.** A carrier that reported failure and absence
//! the same way would turn a broken network into "the stream ends here", and a
//! walk would stop early and report a short chain as a complete one — a silent
//! wrong answer, which is the one kind of failure this workspace refuses to
//! produce. So absence is `3` and everything unexpected is a failure.
//!
//! ## Where the program name comes from, and why it is not the locator
//!
//! The program is named by `KUSANAGI_CARRIER` in this endpoint's own
//! environment. **It is never taken from the locator**, and that is a security
//! boundary rather than a configuration style: a locator arrives inside an
//! invitation, an invitation arrives from somebody else, and a locator that
//! could name a program would be a remote code execution vector wearing the
//! word "waypoint". The locator says which prefix on the carrier to use; this
//! machine's owner says what a carrier is.

use std::io::Write as _;
use std::process::{Command, Stdio};

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};

use crate::client::MAX_OBJECT;

/// The exit status a carrier uses for "there is nothing at that object".
const ABSENT: i32 = 3;

/// A carrier program, as this machine's owner configured it.
///
/// The words are split on whitespace and there is no quoting, which is one rule
/// instead of a shell parser this crate would then own. A path with a space in
/// it goes in a wrapper script; a wrapper is what most carriers need anyway,
/// because the real client's flags are the operator's business and not ours.
#[derive(Debug, Clone)]
pub struct Carrier {
    program: String,
    leading: Vec<String>,
}

impl Carrier {
    /// Reads `PROGRAM [ARG…]` as it was configured.
    ///
    /// # Errors
    ///
    /// [`WaypointError::UnusableAddress`] when the text names no program.
    pub fn parse(text: &str) -> Result<Self, WaypointError> {
        let mut words = text.split_whitespace().map(str::to_owned);
        let program = words.next().ok_or_else(|| WaypointError::UnusableAddress {
            reason: "KUSANAGI_CARRIER is set to nothing; it names the program that moves bytes"
                .to_owned(),
        })?;
        Ok(Self {
            program,
            leading: words.collect(),
        })
    }
}

/// A place whose bytes travel inside somebody else's client.
#[derive(Debug)]
pub struct CarrierWaypoint {
    carrier: Carrier,
    prefix: String,
}

/// What one run of the carrier produced.
struct Ran {
    status: Option<i32>,
    out: Vec<u8>,
    said: String,
}

impl CarrierWaypoint {
    /// A carrier rooted at `prefix`.
    #[must_use]
    pub fn new(carrier: Carrier, prefix: &str) -> Self {
        Self {
            carrier,
            prefix: prefix.trim_end_matches('/').to_owned(),
        }
    }

    /// What the carrier is asked to move: the prefix and the drop address.
    ///
    /// A single token so that the carrier can be a one-line script: a path
    /// under a mount, a key under a bucket, a name under a remote.
    fn object(&self, addr: &DropAddr) -> String {
        if self.prefix.is_empty() {
            addr.to_string()
        } else {
            format!("{}/{addr}", self.prefix)
        }
    }

    /// Runs the carrier once, feeding it `input` and collecting what it wrote.
    ///
    /// The child's stdout is capped at [`MAX_OBJECT`] for the reason every
    /// response body is: what comes back is allocated before it is checked, so
    /// the cap is what stops a carrier from choosing this endpoint's memory.
    fn run(&self, verb: &str, addr: &DropAddr, input: &[u8]) -> Result<Ran, WaypointError> {
        let failed = |source: std::io::Error| WaypointError::Io {
            action: "running the carrier",
            source,
        };
        let mut child = Command::new(&self.carrier.program)
            .args(&self.carrier.leading)
            .arg(verb)
            .arg(self.object(addr))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(failed)?;

        if let Some(mut sink) = child.stdin.take() {
            // A carrier that does not read its input closes the pipe, and a
            // broken pipe here is that carrier's answer rather than a failure of
            // this endpoint. What it did is decided by the exit status below.
            sink.write_all(input).ok();
        }
        let done = child.wait_with_output().map_err(failed)?;
        let mut out = done.stdout;
        out.truncate(usize::try_from(MAX_OBJECT).unwrap_or(usize::MAX));
        Ok(Ran {
            status: done.status.code(),
            out,
            said: String::from_utf8_lossy(&done.stderr)
                .lines()
                .next()
                .unwrap_or_default()
                .to_owned(),
        })
    }

    /// Turns an exit status this contract does not define into a failure.
    fn refused(&self, verb: &'static str, ran: &Ran) -> WaypointError {
        WaypointError::Io {
            action: verb,
            source: std::io::Error::other(format!(
                "the carrier `{}` exited {} : {}",
                self.carrier.program,
                ran.status
                    .map_or_else(|| "on a signal".to_owned(), |code| code.to_string()),
                ran.said
            )),
        }
    }
}

impl Waypoint for CarrierWaypoint {
    /// Sends, then looks, exactly as the HTTP adapter does.
    ///
    /// A carrier cannot be asked for a conditional write: the real clients this
    /// exists to borrow do not all offer one, and one that did would be
    /// answering about its own store rather than about this address. So the
    /// write-once decision is made here, from what is at the address afterwards,
    /// which is the same evidence and needs nothing of the carrier.
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        if let Some(found) = self.get(addr)? {
            return Ok(if found == bytes {
                PutOutcome::Stored
            } else {
                PutOutcome::AlreadyPresent
            });
        }
        let ran = self.run("put", addr, bytes)?;
        if ran.status != Some(0) {
            return Err(self.refused("writing through a carrier", &ran));
        }
        match self.get(addr)? {
            Some(found) if found == bytes => Ok(PutOutcome::Stored),
            Some(_) => Ok(PutOutcome::AlreadyPresent),
            None => Err(WaypointError::Unwritten {
                action: "writing through a carrier",
            }),
        }
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        let ran = self.run("get", addr, &[])?;
        match ran.status {
            Some(0) => Ok(Some(ran.out)),
            Some(ABSENT) => Ok(None),
            _ => Err(self.refused("reading through a carrier", &ran)),
        }
    }

    fn delete(&self, addr: &DropAddr) -> Result<(), WaypointError> {
        let ran = self.run("delete", addr, &[])?;
        match ran.status {
            Some(0 | ABSENT) => Ok(()),
            _ => Err(self.refused("releasing through a carrier", &ran)),
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod tests {
    use super::{Carrier, CarrierWaypoint};
    use kusanagi_kernel::DropAddr;

    #[test]
    fn a_carrier_is_a_program_and_the_words_after_it() {
        let carrier = Carrier::parse("  rclone --config  c.conf ").unwrap();
        assert_eq!(carrier.program, "rclone");
        assert_eq!(carrier.leading, vec!["--config", "c.conf"]);
        assert!(Carrier::parse("   ").is_err());
    }

    #[test]
    fn the_object_is_the_prefix_and_the_address() {
        let addr = DropAddr::from_bytes([0xab; 20]);
        let carrier = Carrier::parse("x").unwrap();
        assert_eq!(
            CarrierWaypoint::new(carrier.clone(), "remote:drops/").object(&addr),
            format!("remote:drops/{addr}")
        );
        assert_eq!(
            CarrierWaypoint::new(carrier, "").object(&addr),
            addr.to_string()
        );
    }
}
