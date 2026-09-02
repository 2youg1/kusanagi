// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Which place a locator names, and the one router that opens it.
//!
//! A kusanagi endpoint is configured by a single string. That is not a
//! simplification for the sake of a demo: an invitation has to fit on one line
//! and carry the host with it, so "where the drops are" must be sayable in one
//! token or `join` needs a configuration file and stops being one line.
//!
//! ```text
//! /var/lib/kusanagi              a directory on this machine
//! file:/var/lib/kusanagi         the same, said out loud
//! http://box.example:8443        somebody's HTTP box
//! s3://ACCOUNT.r2.cloudflarestorage.com/bucket/prefix?region=auto
//! ```
//!
//! [`Place`] is the only place in the workspace that knows more than one adapter
//! exists, and it answers "not offered" on behalf of the ones that cannot do a
//! thing — which is why `doctor` can examine a plain directory without either
//! special-casing it or lying about it.

use std::path::PathBuf;
use std::str::FromStr;

use kusanagi_kernel::{DropAddr, PutOutcome, Waypoint, WaypointError};

use crate::conditional::{Conditional, Fetched, TtlOutcome, Validator};
use crate::dir::DirWaypoint;
use crate::http::HttpWaypoint;
use crate::s3::S3Waypoint;
use crate::sigv4::Credentials;

/// Where an endpoint keeps its drops, before anything has been opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Locator {
    /// A directory on this machine.
    Directory(PathBuf),
    /// An HTTP box, named by its root URL.
    Box {
        /// The root URL, without a trailing slash.
        base: String,
    },
    /// A bucket on an S3-compatible host.
    Bucket {
        /// The endpoint URL, without the bucket.
        endpoint: String,
        /// The bucket name.
        bucket: String,
        /// A key prefix, possibly empty.
        prefix: String,
        /// The signing region; `auto` for R2.
        region: String,
    },
}

impl FromStr for Locator {
    type Err = LocatorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(path) = text.strip_prefix("file:") {
            return Ok(Self::Directory(PathBuf::from(path)));
        }
        if text.starts_with("http://") || text.starts_with("https://") {
            return Ok(Self::Box {
                base: text.trim_end_matches('/').to_owned(),
            });
        }
        if let Some(rest) = text.strip_prefix("s3://") {
            return parse_bucket(rest);
        }
        if text.is_empty() {
            return Err(LocatorError::Empty);
        }
        Ok(Self::Directory(PathBuf::from(text)))
    }
}

fn parse_bucket(rest: &str) -> Result<Locator, LocatorError> {
    let (location, query) = rest.split_once('?').unwrap_or((rest, ""));
    let mut parts = location.splitn(3, '/');
    let endpoint = parts.next().unwrap_or_default();
    let bucket = parts.next().unwrap_or_default();
    let prefix = parts.next().unwrap_or_default();
    if endpoint.is_empty() || bucket.is_empty() {
        return Err(LocatorError::BucketIncomplete);
    }

    let region = query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .find(|(name, _)| *name == "region")
        .map_or("auto", |(_, value)| value);

    Ok(Locator::Bucket {
        endpoint: format!("https://{endpoint}"),
        bucket: bucket.to_owned(),
        prefix: prefix.to_owned(),
        region: region.to_owned(),
    })
}

/// Why a locator does not name a place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LocatorError {
    /// The locator is the empty string.
    #[error("a waypoint locator cannot be empty")]
    Empty,
    /// An `s3://` locator did not carry both an endpoint and a bucket.
    #[error("an s3 locator reads s3://ENDPOINT/BUCKET[/PREFIX][?region=REGION]")]
    BucketIncomplete,
    /// The locator names a bucket but no credentials were supplied.
    #[error("this bucket needs credentials; set KUSANAGI_S3_ACCESS_KEY and KUSANAGI_S3_SECRET_KEY")]
    CredentialsMissing,
}

impl LocatorError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "locator.empty",
            Self::BucketIncomplete => "locator.bucket_incomplete",
            Self::CredentialsMissing => "locator.credentials_missing",
        }
    }
}

/// An opened place, ready to hold bytes.
#[derive(Debug)]
pub enum Place {
    /// A directory on this machine.
    Directory(DirWaypoint),
    /// Somebody's HTTP box.
    Box(HttpWaypoint),
    /// A bucket on an S3-compatible host.
    Bucket(S3Waypoint),
}

impl Place {
    /// Opens what `locator` names.
    ///
    /// `credentials` and `now` arrive from the assembly rather than being read
    /// here, because reading the environment and reading the clock are both
    /// things exactly one module in this program is allowed to do.
    ///
    /// # Errors
    ///
    /// [`LocatorError::CredentialsMissing`] when a bucket is named without them.
    pub fn open(
        locator: &Locator,
        credentials: Option<Credentials>,
        now: u64,
    ) -> Result<Self, LocatorError> {
        match locator {
            Locator::Directory(path) => Ok(Self::Directory(DirWaypoint::new(path))),
            Locator::Box { base } => Ok(Self::Box(HttpWaypoint::new(base))),
            Locator::Bucket {
                endpoint,
                bucket,
                prefix,
                region,
            } => {
                let credentials = credentials.ok_or(LocatorError::CredentialsMissing)?;
                Ok(Self::Bucket(S3Waypoint::new(
                    endpoint,
                    bucket,
                    prefix,
                    region,
                    credentials,
                    now,
                )))
            }
        }
    }

    /// One line naming what kind of place this is, for `doctor` output.
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Directory(_) => "directory",
            Self::Box(_) => "http box",
            Self::Bucket(_) => "s3 bucket",
        }
    }
}

impl Waypoint for Place {
    fn put_if_absent(&self, addr: &DropAddr, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        match self {
            Self::Directory(place) => place.put_if_absent(addr, bytes),
            Self::Box(place) => place.put_if_absent(addr, bytes),
            Self::Bucket(place) => place.put_if_absent(addr, bytes),
        }
    }

    fn get(&self, addr: &DropAddr) -> Result<Option<Vec<u8>>, WaypointError> {
        match self {
            Self::Directory(place) => place.get(addr),
            Self::Box(place) => place.get(addr),
            Self::Bucket(place) => place.get(addr),
        }
    }
}

impl Conditional for Place {
    fn get_if_changed(
        &self,
        addr: &DropAddr,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError> {
        match self {
            // A directory has no validators and no cheap "unchanged" answer, so
            // it always sends the bytes. Saying that plainly is what lets one
            // `doctor` run describe every kind of host.
            Self::Directory(place) => {
                Ok(place
                    .get(addr)?
                    .map_or(Fetched::Absent, |bytes| Fetched::Fresh {
                        bytes,
                        validator: None,
                    }))
            }
            Self::Box(place) => place.get_if_changed(addr, known),
            Self::Bucket(place) => place.get_if_changed(addr, known),
        }
    }

    fn put_with_ttl(
        &self,
        addr: &DropAddr,
        bytes: &[u8],
        seconds: u64,
    ) -> Result<TtlOutcome, WaypointError> {
        match self {
            Self::Directory(place) => {
                place.put_if_absent(addr, bytes)?;
                Ok(TtlOutcome::NotOffered)
            }
            Self::Box(place) => place.put_with_ttl(addr, bytes, seconds),
            Self::Bucket(place) => place.put_with_ttl(addr, bytes, seconds),
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
    use super::{Locator, LocatorError, Place};
    use std::path::PathBuf;
    use std::str::FromStr as _;

    #[test]
    fn a_bare_path_is_a_directory() {
        assert_eq!(
            Locator::from_str("/var/lib/kusanagi").unwrap(),
            Locator::Directory(PathBuf::from("/var/lib/kusanagi"))
        );
        assert_eq!(
            Locator::from_str("file:C:/drops").unwrap(),
            Locator::Directory(PathBuf::from("C:/drops"))
        );
    }

    #[test]
    fn a_url_is_a_box_without_its_trailing_slash() {
        assert_eq!(
            Locator::from_str("http://box.example:8443/").unwrap(),
            Locator::Box {
                base: "http://box.example:8443".to_owned()
            }
        );
    }

    #[test]
    fn a_bucket_locator_carries_endpoint_bucket_prefix_and_region() {
        assert_eq!(
            Locator::from_str("s3://account.r2.cloudflarestorage.com/drops/team/?region=auto")
                .unwrap(),
            Locator::Bucket {
                endpoint: "https://account.r2.cloudflarestorage.com".to_owned(),
                bucket: "drops".to_owned(),
                prefix: "team/".to_owned(),
                region: "auto".to_owned(),
            }
        );
    }

    #[test]
    fn a_bucket_without_a_bucket_is_refused() {
        assert_eq!(
            Locator::from_str("s3://account.r2.cloudflarestorage.com"),
            Err(LocatorError::BucketIncomplete)
        );
        assert_eq!(Locator::from_str(""), Err(LocatorError::Empty));
    }

    #[test]
    fn a_bucket_without_credentials_does_not_open() {
        let locator = Locator::from_str("s3://host/bucket").unwrap();
        assert_eq!(
            Place::open(&locator, None, 0).unwrap_err(),
            LocatorError::CredentialsMissing
        );
    }

    #[test]
    fn every_kind_names_itself() {
        let directory = Place::open(&Locator::from_str("./drops").unwrap(), None, 0).unwrap();
        assert_eq!(directory.kind(), "directory");
        let boxed = Place::open(&Locator::from_str("http://host").unwrap(), None, 0).unwrap();
        assert_eq!(boxed.kind(), "http box");
    }
}
