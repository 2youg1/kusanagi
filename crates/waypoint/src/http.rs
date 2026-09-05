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
//! GET /d/<period>/<ward>/<address>   200 + body + ETag | 304 (If-None-Match) | 404
//! PUT /d/<period>/<ward>/<address>   404, always, whatever happened
//! GET /bin/<period>/<ward prefix>    200 + one key per line
//! ```
//!
//! The third route is what makes a read stop naming an address. It answers with
//! keys and never with bodies, so a host learns which bin was swept and never
//! which object of it was wanted.
//!
//! An unconditional `PUT` is ignored rather than accepted, so this host cannot
//! lose write-once semantics by accident: there is no request that overwrites.
//!
//! **A write is confirmed by reading it back, never by a status code.** A host
//! that answered `201` would be announcing that it is a box to anybody who sent
//! one request to `/d/<40 hex>`, and a scan of the internet would return the list
//! of boxes. So the box says nothing, and this client finds out what happened the
//! way it finds out everything else about a host it does not trust: it looks.
//!
//! **Every header here is one that ordinary web traffic already carries.**
//! `If-None-Match` is how any cache asks a conditional question and how S3
//! spells a write-once write; `Cache-Control: max-age` is how anything asks for
//! a lifetime. A header named after this product would announce it to every
//! proxy and log between the two ends, including those inside TLS.

use kusanagi_kernel::{Listing, Object, PutOutcome, Sweep, Waypoint, WaypointError};

use crate::access::Access;
use crate::client::Client;
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
    client: Client,
}

impl HttpWaypoint {
    /// Points at `base`, which is the root URL of a box.
    ///
    /// What this endpoint will let the host do to it — redirect it, keep it
    /// waiting, hand it an unbounded body — is decided once, in
    /// [`Client`](crate::client::Client), and not restated here.
    #[must_use]
    pub fn new(base: &str, access: &Access) -> Self {
        Self {
            base: base.trim_end_matches('/').to_owned(),
            client: Client::new(access),
        }
    }

    fn url(&self, at: &Object) -> String {
        format!("{}/d/{at}", self.base)
    }

    /// Sends one conditional write, and reports only that it was sent.
    ///
    /// **What the host answers is not evidence and is not read.** What it did is
    /// found out by looking, which is the caller's job because only the caller
    /// knows whether looking would even be meaningful.
    fn put_inner(&self, at: &Object, bytes: &[u8], ttl: Option<u64>) -> Result<(), WaypointError> {
        let mut request = self
            .client
            .agent()
            .put(self.url(at))
            .header(IF_NONE_MATCH, "*")
            .header("Content-Type", "application/octet-stream");
        if let Some(seconds) = ttl {
            request = request.header(TTL_HEADER, format!("max-age={seconds}"));
        }
        let response = request
            .send(bytes)
            .map_err(|source| self.client.failed("writing to a box", &source))?;
        Client::actionable("writing to a box", &response)?;
        Ok(())
    }
}

impl Waypoint for HttpWaypoint {
    /// Writes, then looks.
    ///
    /// What is at the address afterwards is the whole answer: equal to what was
    /// sent means this write landed; different means somebody else's did and this
    /// address is spent; nothing there means the write did not happen — the host
    /// is full, or it is not a box.
    ///
    /// The extra request is a protocol constant rather than a function of what is
    /// being said, so it changes what a host counts and not what a host can
    /// distinguish.
    fn put_if_absent(&self, at: &Object, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        self.put_inner(at, bytes, None)?;
        match self.get(at)? {
            Some(found) if found == bytes => Ok(PutOutcome::Stored),
            Some(_) => Ok(PutOutcome::AlreadyPresent),
            None => Err(WaypointError::Unwritten {
                action: "writing to a box",
            }),
        }
    }

    fn get(&self, at: &Object) -> Result<Option<Vec<u8>>, WaypointError> {
        match self.get_if_changed(at, None)? {
            Fetched::Absent | Fetched::Unchanged => Ok(None),
            Fetched::Fresh { bytes, .. } => Ok(Some(bytes)),
        }
    }

    /// Asks the box to forget a drop, and does not read the answer.
    ///
    /// A box answers every request the same empty `404`, so the status says
    /// nothing here either. Whoever needs to know the drop is gone reads the
    /// address back — which is what `released` in `kusanagi` does once, for the
    /// whole batch, rather than once per drop.
    fn delete(&self, at: &Object) -> Result<(), WaypointError> {
        let response = self
            .client
            .agent()
            .delete(self.url(at))
            .call()
            .map_err(|source| self.client.failed("releasing a drop", &source))?;
        Client::actionable("releasing a drop", &response)?;
        Ok(())
    }
}

impl Listing for HttpWaypoint {
    /// Asks the box for the keys under one prefix, one per line.
    ///
    /// Lines the reader cannot parse are dropped rather than reported. A box
    /// that pads its answer with rubbish wastes this reader's bandwidth, which
    /// is the most any host can do to a sweep, and every line that does parse is
    /// still checked against the sweep that asked for it.
    fn list(&self, sweep: &Sweep) -> Result<Vec<Object>, WaypointError> {
        let url = format!("{}/bin/{}", self.base, sweep.prefix());
        let mut response = self
            .client
            .agent()
            .get(&url)
            .call()
            .map_err(|source| self.client.failed("listing a bin", &source))?;
        if Client::actionable("listing a bin", &response)? == 404 {
            return Ok(Vec::new());
        }
        let body = Client::body("listing a bin", &mut response)?;
        Ok(crate::client::listed(
            &String::from_utf8_lossy(&body),
            sweep,
        ))
    }
}

impl Conditional for HttpWaypoint {
    fn get_if_changed(
        &self,
        at: &Object,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError> {
        let mut request = self.client.agent().get(self.url(at));
        if let Some(validator) = known {
            request = request.header(IF_NONE_MATCH, validator.as_str());
        }
        let mut response = request
            .call()
            .map_err(|source| self.client.failed("reading from a box", &source))?;

        let validator = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(Validator::new);

        match Client::actionable("reading from a box", &response)? {
            304 => Ok(Fetched::Unchanged),
            404 => Ok(Fetched::Absent),
            200 => {
                let bytes = Client::body("reading a segment from a box", &mut response)?;
                Ok(Fetched::Fresh { bytes, validator })
            }
            other => Err(WaypointError::UnusableAddress {
                reason: format!("the box answered {other} to a read"),
            }),
        }
    }

    /// Writes with a lifetime, and does **not** look afterwards.
    ///
    /// A lifetime cannot be confirmed by reading back, because a lifetime that
    /// has already elapsed and a write that never happened are the same empty
    /// address. Telling those two apart is exactly what `probe::examine` is for,
    /// so this reports what it sent and leaves the finding to the prober.
    fn put_with_ttl(
        &self,
        at: &Object,
        bytes: &[u8],
        seconds: u64,
    ) -> Result<TtlOutcome, WaypointError> {
        self.put_inner(at, bytes, Some(seconds))?;
        Ok(TtlOutcome::Accepted)
    }
}
