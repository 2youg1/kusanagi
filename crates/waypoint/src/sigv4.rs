// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Signature Version 4, and the two values it needs: credentials and a date.
//!
//! Apart from `s3.rs` because it answers a different question. The adapter next
//! door decides which request to make; this decides how a request is proved to
//! have come from somebody holding a secret — and it is checked against the
//! worked example AWS publishes rather than against our own belief about it.
//!
//! The clock is a parameter here as everywhere else. A signature is valid for
//! about fifteen minutes, so a one-shot command signs with the instant it
//! sampled at the top and nothing further down reads a clock again.

use hmac::{Hmac, Mac as _};
use sha2::{Digest as _, Sha256};

use kusanagi_kernel::{Hex, WaypointError};

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

/// What signs one request to one bucket.
///
/// Borrowed rather than owned: it is assembled for one call out of what the
/// adapter already holds, so a secret has no second copy to be kept in step with
/// the first.
pub(crate) struct Signing<'a> {
    pub(crate) credentials: &'a Credentials,
    pub(crate) region: &'a str,
    pub(crate) host: &'a str,
    pub(crate) now: u64,
}

impl Signing<'_> {
    /// Signs one request and returns the headers it must carry.
    pub(crate) fn headers(
        &self,
        method: &str,
        key: &str,
        query: &str,
        payload: &[u8],
        extra: &[(&str, String)],
    ) -> Result<Vec<(String, String)>, WaypointError> {
        let stamp = timestamp(self.now).ok_or_else(|| WaypointError::UnusableAddress {
            reason: format!("the clock reading {} is not a representable date", self.now),
        })?;
        let payload_hash = Hex(&Sha256::digest(payload)).to_string();

        let mut headers: Vec<(String, String)> = vec![
            ("host".to_owned(), self.host.to_owned()),
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
            key,
            query,
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

    pub(crate) fn signature(&self, date: &str, to_sign: &str) -> Result<String, WaypointError> {
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
}
/// One query parameter value, encoded as a canonical request spells it.
///
/// AWS signs the query it sent, so this must agree with what goes on the wire
/// byte for byte. Unreserved characters pass; everything else — `/` inside a key
/// prefix, `+ / =` inside a continuation token — becomes `%XX` in upper case.
/// Written here rather than taken from a crate because it is ten lines, and a
/// crate is somebody who can write code into this binary.
pub(crate) fn encoded(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            out.push(char::from(byte));
        } else {
            // `Hex` is this workspace's one spelling of a byte in base sixteen;
            // AWS wants it in upper case, and that is the whole difference.
            out.push('%');
            out.push_str(&Hex(&[byte]).to_string().to_ascii_uppercase());
        }
    }
    out
}

fn mac(key: &[u8], message: &[u8]) -> Option<[u8; 32]> {
    let mut mac = HmacSha256::new_from_slice(key).ok()?;
    mac.update(message);
    Some(mac.finalize().into_bytes().into())
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

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]
mod tests {
    use super::{Credentials, Signing, timestamp};

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
        let credentials = Credentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        );
        let signing = Signing {
            credentials: &credentials,
            region: "us-east-1",
            host: "examplebucket.s3.amazonaws.com",
            now: 1_369_353_600,
        };
        // The published string-to-sign for that example, verbatim.
        let to_sign = "AWS4-HMAC-SHA256\n\
             20130524T000000Z\n\
             20130524/us-east-1/s3/aws4_request\n\
             7344ae5b7ee6c3e7e6b0fe0640412a37625d1fbfff95c48bbb2dc43964946972";
        assert_eq!(
            signing.signature("20130524", to_sign).unwrap(),
            "f0e8bdb87c964420e857bd35b5d6ed310bd44f0170aba48dd91039c6036bdb41"
        );
    }

    #[test]
    fn credentials_never_print_their_secret() {
        let printed = format!("{:?}", Credentials::new("public-id", "the-secret"));
        assert!(printed.contains("public-id"));
        assert!(!printed.contains("the-secret"));
    }
}
