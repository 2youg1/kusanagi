// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A waypoint reached over HTTP: somebody's own box, on their own machine.
//!
//! The protocol is deliberately smaller than S3, because everything this network
//! asks of a host is small. It is written out in `docs/box-protocol.md`, and the
//! server that answers it is `kusanagi host`.
//!
//! ```text
//! GET /d/<address>     200 + body + ETag | 304 (If-None-Match) | 404
//! PUT /d/<address>     201 | 412 when occupied | 428 without If-None-Match: *
//! ```
//!
//! An unconditional `PUT` is refused rather than accepted, so this host cannot
//! lose write-once semantics by accident: there is no request that overwrites.
//!
//! **Every header here is one that ordinary web traffic already carries.**
//! `If-None-Match` is how any cache asks a conditional question and how S3
//! spells a write-once write; `Cache-Control: max-age` is how anything asks for
//! a lifetime. A header named after this product would announce it to every
//! proxy and log between the two ends, including those inside TLS.

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};

use crate::access::Proxy;
use crate::conditional::{Conditional, Fetched, TtlOutcome, Validator};

/// The header that asks for a write only if nothing is there.
const IF_NONE_MATCH: &str = "If-None-Match";

/// The header carrying a requested lifetime.
///
/// `Cache-Control: max-age=<seconds>`, which is what a browser, a CDN and a
/// package manager all send.
pub const TTL_HEADER: &str = "Cache-Control";

/// A waypoint that is somebody's HTTP box.
#[derive(Debug, Clone)]
pub struct HttpWaypoint {
    base: String,
    agent: ureq::Agent,
}

impl HttpWaypoint {
    /// Points at `base`, which is the root URL of a box.
    ///
    /// Status codes are *not* treated as transport errors: 404, 412 and 304 are
    /// all normal answers in this protocol, and an adapter that could not tell
    /// them apart from a broken connection would have to guess.
    ///
    /// **No `User-Agent`.** An agent string puts a library name and version into
    /// every request, where a host, a proxy and any log on the path can read it,
    /// and a version narrows the set of people it could be. Sending none says
    /// less than sending anything, and is unremarkable: plenty of API traffic
    /// carries no agent string. A borrowed browser header above a handshake that
    /// is plainly not a browser's would be a worse tell than silence.
    #[must_use]
    pub fn new(base: &str, proxy: Option<&Proxy>) -> Self {
        let agent = Proxy::applied(
            proxy,
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .user_agent(ureq::config::AutoHeaderValue::None),
        )
        .build()
        .into();
        Self {
            base: base.trim_end_matches('/').to_owned(),
            agent,
        }
    }

    fn url(&self, addr: &DropAddr) -> String {
        format!("{}/d/{addr}", self.base)
    }

    fn put_inner(
        &self,
        addr: &DropAddr,
        bytes: &[u8],
        ttl: Option<u64>,
    ) -> Result<PutOutcome, WaypointError> {
        let mut request = self
            .agent
            .put(self.url(addr))
            .header(IF_NONE_MATCH, "*")
            .header("Content-Type", "application/octet-stream");
        if let Some(seconds) = ttl {
            request = request.header(TTL_HEADER, format!("max-age={seconds}"));
        }
        let response = request
            .send(bytes)
            .map_err(|source| transport("writing to a box", &source))?;

        match response.status().as_u16() {
            200 | 201 | 204 => Ok(PutOutcome::Stored),
            412 => Ok(PutOutcome::AlreadyPresent),
            428 => Err(WaypointError::OverwriteNotRefused),
            other => Err(WaypointError::UnusableAddress {
                reason: format!("the box answered {other} to a conditional write"),
            }),
        }
    }
}

/// Every transport failure arrives here, so that the action being attempted is
/// attached in one place rather than at a dozen call sites.
fn transport(action: &'static str, source: &dyn core::fmt::Display) -> WaypointError {
    WaypointError::Io {
        action,
        source: std::io::Error::other(source.to_string()),
    }
}

impl Waypoint for HttpWaypoint {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        self.put_inner(addr, bytes, None)
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        match self.get_if_changed(addr, None)? {
            Fetched::Absent | Fetched::Unchanged => Ok(None),
            Fetched::Fresh { bytes, .. } => Ok(Some(bytes)),
        }
    }
}

impl Conditional for HttpWaypoint {
    fn get_if_changed(
        &self,
        addr: &DropAddr,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError> {
        let mut request = self.agent.get(self.url(addr));
        if let Some(validator) = known {
            request = request.header(IF_NONE_MATCH, validator.as_str());
        }
        let mut response = request
            .call()
            .map_err(|source| transport("reading from a box", &source))?;

        let validator = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(Validator::new);

        match response.status().as_u16() {
            304 => Ok(Fetched::Unchanged),
            404 => Ok(Fetched::Absent),
            200 => {
                let bytes = response
                    .body_mut()
                    .read_to_vec()
                    .map_err(|source| transport("reading a segment from a box", &source))?;
                Ok(Fetched::Fresh { bytes, validator })
            }
            other => Err(WaypointError::UnusableAddress {
                reason: format!("the box answered {other} to a read"),
            }),
        }
    }

    fn put_with_ttl(
        &self,
        addr: &DropAddr,
        bytes: &[u8],
        seconds: u64,
    ) -> Result<TtlOutcome, WaypointError> {
        match self.put_inner(addr, bytes, Some(seconds))? {
            PutOutcome::Stored => Ok(TtlOutcome::Accepted),
            PutOutcome::AlreadyPresent => Err(WaypointError::UnusableAddress {
                reason: "the probe address was already occupied".to_owned(),
            }),
        }
    }
}
