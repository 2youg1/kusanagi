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

use kusanagi_kernel::{Listing, Object, PutOutcome, Sweep, Waypoint, WaypointError};

use crate::access::Access;
use crate::client::{Client, MAX_LISTED};
use crate::conditional::{Conditional, Fetched, TtlOutcome, Validator};
use crate::sigv4::{Credentials, Signing, encoded};

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

    fn key(&self, at: &Object) -> String {
        format!("/{}/{}{at}", self.bucket, self.prefix)
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
        at: &Object,
        payload: &[u8],
        extra: &[(&str, String)],
    ) -> Result<Answer, WaypointError> {
        self.sign_and_send(method, &self.key(at), "", payload, extra)
    }

    /// One signed request against `key`, with `query` already canonical.
    ///
    /// Split out of [`S3Waypoint::send`] when listing arrived, because a listing
    /// signs the bucket rather than an object and carries a query that must be
    /// signed exactly as it is sent.
    fn sign_and_send(
        &self,
        method: &str,
        key: &str,
        query: &str,
        payload: &[u8],
        extra: &[(&str, String)],
    ) -> Result<Answer, WaypointError> {
        let headers = self.signing().headers(method, key, query, payload, extra)?;
        let url = if query.is_empty() {
            format!("{}{key}", self.endpoint)
        } else {
            format!("{}{key}?{query}", self.endpoint)
        };
        let sent = match method {
            "PUT" => carrying(self.client.agent().put(&url), &headers).send(payload),
            "DELETE" => carrying(self.client.agent().delete(&url), &headers).call(),
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

/// How many pages of a listing this adapter will follow.
///
/// One page is a thousand keys, so four is well past any honest bin and short
/// enough that a bucket cannot bill a reader indefinitely for one sweep.
const MAX_PAGES: u8 = 4;

/// The text inside every `<name>…</name>` of `xml`, in order.
///
/// Nine lines instead of an XML parser, and the reason is the same one that
/// keeps the dependency count low: this reads exactly two element names out of
/// one response whose producer is not trusted anyway, and every value it
/// extracts is parsed again as a key and checked against the sweep that asked
/// for it. Nothing here decides anything; it only narrows what is looked at.
fn tagged<'a>(xml: &'a str, name: &str) -> Vec<&'a str> {
    let open = format!("<{name}>");
    let close = format!("</{name}>");
    xml.split(open.as_str())
        .skip(1)
        .filter_map(|rest| rest.split(close.as_str()).next())
        .collect()
}

/// What an object store answered.
struct Answer {
    status: u16,
    validator: Option<Validator>,
    body: Vec<u8>,
}

impl Waypoint for S3Waypoint {
    fn put_if_absent(&self, at: &Object, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        let answer = self.send("PUT", at, bytes, &[("if-none-match", "*".to_owned())])?;
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

    fn get(&self, at: &Object) -> Result<Option<Vec<u8>>, WaypointError> {
        match self.get_if_changed(at, None)? {
            Fetched::Absent | Fetched::Unchanged => Ok(None),
            Fetched::Fresh { bytes, .. } => Ok(Some(bytes)),
        }
    }

    /// `DeleteObject`, whose success and whose "there was nothing there" are the
    /// same `204` in every S3 implementation worth using.
    fn delete(&self, at: &Object) -> Result<(), WaypointError> {
        let answer = self.send("DELETE", at, &[], &[])?;
        match answer.status {
            200 | 202 | 204 | 404 => Ok(()),
            other => Err(WaypointError::UnusableAddress {
                reason: format!(
                    "the object store answered {other} to a delete: {}",
                    String::from_utf8_lossy(&answer.body)
                ),
            }),
        }
    }
}

impl Listing for S3Waypoint {
    /// `ListObjectsV2` under the sweep's prefix, following pages to a cap.
    ///
    /// The cap is the point at which this stops believing a bucket: four pages
    /// of a thousand keys is thirty times what a reader expects to find, and a
    /// bucket that keeps saying "there is more" after that is wasting a reader's
    /// money rather than delivering messages. What it costs to stop early is
    /// bounded by the same rule that bounds the bin, and `kusanagi` reports it.
    fn list(&self, sweep: &Sweep) -> Result<Vec<Object>, WaypointError> {
        let bucket = format!("/{}", self.bucket);
        let under = format!("{}{}", self.prefix, sweep.prefix());
        let mut found = Vec::new();
        let mut carry: Option<String> = None;
        for _ in 0..MAX_PAGES {
            let query = match &carry {
                Some(token) => format!(
                    "continuation-token={}&list-type=2&prefix={}",
                    encoded(token),
                    encoded(&under)
                ),
                None => format!("list-type=2&prefix={}", encoded(&under)),
            };
            let answer = self.sign_and_send("GET", &bucket, &query, &[], &[])?;
            if answer.status != 200 {
                return Err(WaypointError::UnusableAddress {
                    reason: format!(
                        "the object store answered {} to a listing: {}",
                        answer.status,
                        String::from_utf8_lossy(&answer.body)
                    ),
                });
            }
            let page = String::from_utf8_lossy(&answer.body);
            for key in tagged(&page, "Key") {
                if let Some(rest) = key.strip_prefix(self.prefix.as_str())
                    && let Ok(at) = rest.parse::<Object>()
                    && sweep.holds(&at)
                {
                    found.push(at);
                }
            }
            carry = tagged(&page, "NextContinuationToken")
                .first()
                .map(|token| (*token).to_owned());
            if carry.is_none() || found.len() >= MAX_LISTED {
                break;
            }
        }
        found.truncate(MAX_LISTED);
        Ok(found)
    }
}

impl Conditional for S3Waypoint {
    fn get_if_changed(
        &self,
        at: &Object,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError> {
        let extra = known.map_or_else(Vec::new, |validator| {
            vec![("if-none-match", validator.as_str().to_owned())]
        });
        let answer = self.send("GET", at, &[], &extra)?;
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
        at: &Object,
        bytes: &[u8],
        _seconds: u64,
    ) -> Result<TtlOutcome, WaypointError> {
        // S3 expires objects through bucket lifecycle rules, which are a property
        // of the bucket rather than of a request. Reporting that plainly is what
        // lets `doctor` record a named degradation instead of pretending.
        self.put_if_absent(at, bytes)?;
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
    use super::{Access, Credentials, S3Waypoint, encoded, tagged};
    use kusanagi_kernel::{Bin, DropAddr, Object, Period, Ward};

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
        let at = Object::new(
            Bin::new(Period::from_count(7), Ward::from_bits(0x00ab)),
            DropAddr::from_bytes([0xab; 20]),
        );
        assert_eq!(waypoint.key(&at), format!("/drops/kusanagi/{at}"));
        assert_eq!(waypoint.host, "account.r2.cloudflarestorage.com");
    }

    #[test]
    fn a_listing_reads_the_keys_out_of_one_page_and_drops_the_rest() {
        let at = Object::new(
            Bin::new(Period::from_count(7), Ward::from_bits(0x00ab)),
            DropAddr::from_bytes([0xab; 20]),
        );
        let page = format!(
            "<ListBucketResult><Contents><Key>kusanagi/{at}</Key></Contents>             <Contents><Key>kusanagi/not-a-key</Key></Contents>             <NextContinuationToken>1/x=</NextContinuationToken></ListBucketResult>"
        );
        assert_eq!(
            tagged(&page, "Key"),
            vec![format!("kusanagi/{at}"), "kusanagi/not-a-key".to_owned()]
        );
        assert_eq!(tagged(&page, "NextContinuationToken"), vec!["1/x="]);
        assert_eq!(encoded("1/x="), "1%2Fx%3D");
    }
}
