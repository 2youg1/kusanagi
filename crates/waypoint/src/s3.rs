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

use hmac::{Hmac, Mac as _};
use kusanagi_kernel::{DropAddr, Hex, PutOutcome, Waypoint, WaypointError};
use sha2::{Digest as _, Sha256};

use crate::conditional::{Conditional, Fetched, TtlOutcome, Validator};

type HmacSha256 = Hmac<Sha256>;

/// The signing algorithm named in every request.
const ALGORITHM: &str = "AWS4-HMAC-SHA256";

/// The service name in the credential scope.
const SERVICE: &str = "s3";

/// Credentials for one bucket.
///
/// Held by value and never printed: `Debug` is written by hand so that a struct
/// containing one cannot leak it through a derived formatter.
#[derive(Clone)]
pub struct Credentials {
    access_key: String,
    secret_key: String,
}

impl Credentials {
    /// Wraps an access key and its secret.
    #[must_use]
    pub fn new(access_key: &str, secret_key: &str) -> Self {
        Self {
            access_key: access_key.to_owned(),
            secret_key: secret_key.to_owned(),
        }
    }
}

impl core::fmt::Debug for Credentials {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Credentials({}, redacted)", self.access_key)
    }
}

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
    agent: ureq::Agent,
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
            agent: ureq::Agent::config_builder()
                .http_status_as_error(false)
                .build()
                .into(),
        }
    }

    fn key(&self, addr: &DropAddr) -> String {
        format!("/{}/{}{addr}", self.bucket, self.prefix)
    }

    fn url(&self, addr: &DropAddr) -> String {
        format!("{}{}", self.endpoint, self.key(addr))
    }

    /// Signs one request and returns the headers it must carry.
    fn signed_headers(
        &self,
        method: &str,
        addr: &DropAddr,
        payload: &[u8],
        extra: &[(&str, String)],
    ) -> Result<Vec<(String, String)>, WaypointError> {
        let stamp = timestamp(self.now).ok_or_else(|| WaypointError::UnusableAddress {
            reason: format!("the clock reading {} is not a representable date", self.now),
        })?;
        let payload_hash = Hex(&Sha256::digest(payload)).to_string();

        let mut headers: Vec<(String, String)> = vec![
            ("host".to_owned(), self.host.clone()),
            ("x-amz-content-sha256".to_owned(), payload_hash.clone()),
            ("x-amz-date".to_owned(), stamp.full.clone()),
        ];
        for (name, value) in extra {
            headers.push((name.to_lowercase(), value.clone()));
        }
        headers.sort_by(|left, right| left.0.cmp(&right.0));

        let mut canonical_headers = String::new();
        let mut signed_names = Vec::with_capacity(headers.len());
        for (name, value) in &headers {
            canonical_headers.push_str(name);
            canonical_headers.push(':');
            canonical_headers.push_str(value.trim());
            canonical_headers.push('\n');
            signed_names.push(name.clone());
        }
        let signed_names = signed_names.join(";");

        let canonical_request = [
            method,
            &self.key(addr),
            "",
            &canonical_headers,
            &signed_names,
            &payload_hash,
        ]
        .join("\n");

        let scope = format!("{}/{}/{SERVICE}/aws4_request", stamp.date, self.region);
        let to_sign = [
            ALGORITHM,
            &stamp.full,
            &scope,
            &Hex(&Sha256::digest(canonical_request.as_bytes())).to_string(),
        ]
        .join("\n");

        let signature = self.signature(&stamp.date, &to_sign)?;
        let authorization = format!(
            "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_names}, Signature={signature}",
            self.credentials.access_key
        );

        headers.push(("authorization".to_owned(), authorization));
        Ok(headers)
    }

    fn signature(&self, date: &str, to_sign: &str) -> Result<String, WaypointError> {
        let mut key = mac(
            format!("AWS4{}", self.credentials.secret_key).as_bytes(),
            date.as_bytes(),
        );
        for step in [self.region.as_bytes(), SERVICE.as_bytes(), b"aws4_request"] {
            key = key.and_then(|derived| mac(&derived, step));
        }
        let signed = key
            .and_then(|derived| mac(&derived, to_sign.as_bytes()))
            .ok_or(WaypointError::OverwriteNotRefused)?;
        Ok(Hex(&signed).to_string())
    }

    fn send(
        &self,
        method: &str,
        addr: &DropAddr,
        payload: &[u8],
        extra: &[(&str, String)],
    ) -> Result<Answer, WaypointError> {
        let headers = self.signed_headers(method, addr, payload, extra)?;
        let url = self.url(addr);
        let sent = match method {
            "PUT" => carrying(self.agent.put(&url), &headers).send(payload),
            _ => carrying(self.agent.get(&url), &headers).call(),
        };
        let mut response =
            sent.map_err(|source| transport("talking to an object store", &source))?;
        let status = response.status().as_u16();
        let validator = response
            .headers()
            .get("etag")
            .and_then(|value| value.to_str().ok())
            .map(Validator::new);
        let body = response
            .body_mut()
            .read_to_vec()
            .map_err(|source| transport("reading an object", &source))?;
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

fn mac(key: &[u8], message: &[u8]) -> Option<[u8; 32]> {
    let mut mac = HmacSha256::new_from_slice(key).ok()?;
    mac.update(message);
    Some(mac.finalize().into_bytes().into())
}

fn transport(action: &'static str, source: &dyn core::fmt::Display) -> WaypointError {
    WaypointError::Io {
        action,
        source: std::io::Error::other(source.to_string()),
    }
}

/// A signing timestamp in the two shapes `SigV4` needs it.
struct Stamp {
    /// `YYYYMMDD`, for the credential scope.
    date: String,
    /// `YYYYMMDDTHHMMSSZ`, for `x-amz-date`.
    full: String,
}

/// Converts Unix seconds to a UTC calendar timestamp.
///
/// Hand-written rather than pulled from a date library: this is the only date
/// arithmetic in the workspace, it has one correct answer, and the algorithm —
/// Howard Hinnant's `civil_from_days` — is public and short. Every step is
/// checked, so an absurd clock reading returns `None` instead of wrapping into a
/// signature that would be rejected for a reason nobody could diagnose.
fn timestamp(seconds: u64) -> Option<Stamp> {
    let days = seconds.checked_div(86_400)?;
    let rest = seconds.checked_rem(86_400)?;
    let hour = rest.checked_div(3_600)?;
    let minute = rest.checked_rem(3_600)?.checked_div(60)?;
    let second = rest.checked_rem(60)?;

    let z = days.checked_add(719_468)?;
    let era = z.checked_div(146_097)?;
    let doe = z.checked_sub(era.checked_mul(146_097)?)?;
    let yoe = doe
        .checked_sub(doe.checked_div(1_460)?)?
        .checked_add(doe.checked_div(36_524)?)?
        .checked_sub(doe.checked_div(146_096)?)?
        .checked_div(365)?;
    let year = yoe.checked_add(era.checked_mul(400)?)?;
    let doy = doe.checked_sub(
        yoe.checked_mul(365)?
            .checked_add(yoe.checked_div(4)?)?
            .checked_sub(yoe.checked_div(100)?)?,
    )?;
    let mp = doy.checked_mul(5)?.checked_add(2)?.checked_div(153)?;
    let day = doy
        .checked_sub(mp.checked_mul(153)?.checked_add(2)?.checked_div(5)?)?
        .checked_add(1)?;
    let month = if mp < 10 {
        mp.checked_add(3)?
    } else {
        mp.checked_sub(9)?
    };
    let year = if month <= 2 {
        year.checked_add(1)?
    } else {
        year
    };

    Some(Stamp {
        date: format!("{year:04}{month:02}{day:02}"),
        full: format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z"),
    })
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
    use super::{Credentials, S3Waypoint, timestamp};
    use kusanagi_kernel::DropAddr;

    #[test]
    fn the_epoch_renders_as_the_epoch() {
        let stamp = timestamp(0).unwrap();
        assert_eq!(stamp.full, "19700101T000000Z");
        assert_eq!(stamp.date, "19700101");
    }

    /// The instant AWS uses in its own worked example of `SigV4` signing.
    #[test]
    fn the_documented_example_instant_renders_correctly() {
        // 2013-05-24T00:00:00Z
        let stamp = timestamp(1_369_353_600).unwrap();
        assert_eq!(stamp.full, "20130524T000000Z");
        assert_eq!(stamp.date, "20130524");
    }

    #[test]
    fn leap_days_and_century_years_land_correctly() {
        // 2000-02-29T12:34:56Z — a leap day in a century year that *is* a leap year
        assert_eq!(timestamp(951_827_696).unwrap().full, "20000229T123456Z");
        // 2100-03-01T00:00:00Z — the day after a century year that is *not*
        assert_eq!(timestamp(4_107_542_400).unwrap().full, "21000301T000000Z");
        // 2024-12-31T23:59:59Z
        assert_eq!(timestamp(1_735_689_599).unwrap().full, "20241231T235959Z");
    }

    /// The worked example published in the AWS documentation for signing a
    /// `GetObject` request. If this stops matching, the signing is wrong and no
    /// real host will accept anything this adapter sends.
    #[test]
    fn signing_reproduces_the_published_aws_vector() {
        let waypoint = S3Waypoint::new(
            "https://examplebucket.s3.amazonaws.com",
            "",
            "",
            "us-east-1",
            Credentials::new(
                "AKIAIOSFODNN7EXAMPLE",
                "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
            ),
            1_369_353_600,
        );
        // The published string-to-sign for that example, verbatim.
        let to_sign = "AWS4-HMAC-SHA256\n\
             20130524T000000Z\n\
             20130524/us-east-1/s3/aws4_request\n\
             7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972";
        assert_eq!(
            waypoint.signature("20130524", to_sign).unwrap(),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn a_key_is_the_bucket_the_prefix_and_the_address() {
        let waypoint = S3Waypoint::new(
            "https://account.r2.cloudflarestorage.com",
            "drops",
            "kusanagi/",
            "auto",
            Credentials::new("id", "secret"),
            0,
        );
        let addr = DropAddr::from_bytes([0xab; 20]);
        assert_eq!(
            waypoint.url(&addr),
            format!("https://account.r2.cloudflarestorage.com/drops/kusanagi/{addr}")
        );
        assert_eq!(waypoint.host, "account.r2.cloudflarestorage.com");
    }

    #[test]
    fn credentials_never_print_their_secret() {
        let printed = format!("{:?}", Credentials::new("public-id", "the-secret"));
        assert!(printed.contains("public-id"));
        assert!(!printed.contains("the-secret"));
    }
}
