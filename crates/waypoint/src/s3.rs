// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! An object store that speaks S3, reached with nothing but HTTP and a hash.
//!
//! Write-once here is not something this code enforces — it is `If-None-Match: *`
//! on `PutObject`, which S3 has supported since August 2024 and which Cloudflare
//! R2 answers with `412 PreconditionFailed`. That is the whole reason a network
//! of dead drops can run on a rented bucket: **the storage refuses the second
//! write, so there is no conflict resolution to get wrong.**
//!
//! Not every `S3`-compatible host means the same thing by that header, and the
//! divergences fail *open* — a condition quietly ignored, a write quietly
//! succeeding. That is why `kusanagi doctor` measures a host instead of reading
//! its documentation, and why this adapter never assumes it succeeded.
//!
//! Requests are signed with `SigV4` by hand. The alternative is an SDK that brings
//! an async runtime and several hundred crates to send four kinds of request;
//! the signing is ninety lines and is checked against the vector AWS publishes.

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};

use crate::access::Access;
use crate::client::Client;
use crate::conditional::{Conditional, Fetched, TtlOutcome, Validator};
use crate::sigv4::{Credentials, Signing};

/// A bucket on an S3-compatible host.
#[derive(Debug, Clone)]
pub struct S3Waypoint {
    endpoint: String,
    host: String,
    bucket: String,
    prefix: String,
    region: String,
    credentials: Credentials,
    now: u64,
    client: Client,
}

impl S3Waypoint {
    /// Points at one bucket.
    ///
    /// `now` is the time this adapter signs with, taken as a parameter like every
    /// other clock reading in this workspace. A signature is only valid within a
    /// window of about fifteen minutes, so an adapter built once and used for
    /// hours must be rebuilt — which is exactly what a one-shot command does.
    #[must_use]
    pub fn new(
        endpoint: &str,
        bucket: &str,
        prefix: &str,
        region: &str,
        credentials: Credentials,
        access: &Access,
        now: u64,
    ) -> Self {
        let endpoint = endpoint.trim_end_matches('/').to_owned();
        let host = endpoint
            .split_once("://")
            .map_or(endpoint.as_str(), |(_, rest)| rest)
            .trim_end_matches('/')
            .to_owned();
        Self {
            endpoint,
            host,
            bucket: bucket.to_owned(),
            prefix: prefix.to_owned(),
            region: region.to_owned(),
            credentials,
            now,
            client: Client::new(access),
        }
    }

    fn key(&self, addr: &DropAddr) -> String {
        format!("/{}/{}{addr}", self.bucket, self.prefix)
    }

    fn url(&self, addr: &DropAddr) -> String {
        format!("{}{}", self.endpoint, self.key(addr))
    }

    /// What signs this bucket's requests at the instant this adapter holds.
    fn signing(&self) -> Signing<'_> {
        Signing {
            credentials: &self.credentials,
            region: &self.region,
            host: &self.host,
            now: self.now,
        }
    }

    fn send(
        &self,
        method: &str,
        addr: &DropAddr,
        payload: &[u8],
        extra: &[(&str, String)],
    ) -> Result<Answer, WaypointError> {
        let headers = self
            .signing()
            .headers(method, &self.key(addr), payload, extra)?;
        let url = self.url(addr);
        let sent = match method {
            "PUT" => carrying(self.client.agent().put(&url), &headers).send(payload),
            _ => carrying(self.client.agent().get(&url), &headers).call(),
        };
        let mut response =
            sent.map_err(|source| self.client.failed("talking to an object store", &source))?;
        let status = Client::actionable("talking to an object store", &response)?;
        let validator = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(Validator::new);
        let body = Client::body("reading an object", &mut response)?;
        Ok(Answer {
            status,
            validator,
            body,
        })
    }
}

/// Attaches every signed header except `host`, which the transport sets itself:
/// sending it twice is what makes a signature disagree with its own request.
fn carrying<B>(
    mut request: ureq::RequestBuilder<B>,
    headers: &[(String, String)],
) -> ureq::RequestBuilder<B> {
    for (name, value) in headers {
        if name != "host" {
            request = request.header(name, value);
        }
    }
    request
}

/// What an object store answered.
struct Answer {
    status: u16,
    validator: Option<Validator>,
    body: Vec<u8>,
}

impl Waypoint for S3Waypoint {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        let answer = self.send("PUT", addr, bytes, &[("if-none-match", "*".to_owned())])?;
        match answer.status {
            200 | 201 | 204 => Ok(PutOutcome::Stored),
            412 => Ok(PutOutcome::AlreadyPresent),
            other => Err(WaypointError::UnusableAddress {
                reason: format!(
                    "the object store answered {other} to a conditional write: {}",
                    String::from_utf8_lossy(&answer.body)
                ),
            }),
        }
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        match self.get_if_changed(addr, None)? {
            Fetched::Absent | Fetched::Unchanged => Ok(None),
            Fetched::Fresh { bytes, .. } => Ok(Some(bytes)),
        }
    }
}

impl Conditional for S3Waypoint {
    fn get_if_changed(
        &self,
        addr: &DropAddr,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError> {
        let extra = known.map_or_else(Vec::new, |validator| {
            vec![("if-none-match", validator.as_str().to_owned())]
        });
        let answer = self.send("GET", addr, &[], &extra)?;
        match answer.status {
            304 => Ok(Fetched::Unchanged),
            403 | 404 => Ok(Fetched::Absent),
            200 => Ok(Fetched::Fresh {
                bytes: answer.body,
                validator: answer.validator,
            }),
            other => Err(WaypointError::UnusableAddress {
                reason: format!("the object store answered {other} to a read"),
            }),
        }
    }

    fn put_with_ttl(
        &self,
        addr: &DropAddr,
        bytes: &[u8],
        _seconds: u64,
    ) -> Result<TtlOutcome, WaypointError> {
        // S3 expires objects through bucket lifecycle rules, which are a property
        // of the bucket rather than of a request. Reporting that plainly is what
        // lets `doctor` record a named degradation instead of pretending.
        self.put_if_absent(addr, bytes)?;
        Ok(TtlOutcome::NotOffered)
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
    use super::{Access, Credentials, S3Waypoint};
    use kusanagi_kernel::DropAddr;

    #[test]
    fn a_key_is_the_bucket_the_prefix_and_the_address() {
        let waypoint = S3Waypoint::new(
            "https://account.r2.cloudflarestorage.com",
            "drops",
            "kusanagi/",
            "auto",
            Credentials::new("id", "secret"),
            &Access::default(),
            0,
        );
        let addr = DropAddr::from_bytes([0xab; 20]);
        assert_eq!(
            waypoint.url(&addr),
            format!("https://account.r2.cloudflarestorage.com/drops/kusanagi/{addr}")
        );
        assert_eq!(waypoint.host, "account.r2.cloudflarestorage.com");
    }
}
