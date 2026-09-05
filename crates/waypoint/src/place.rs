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
//! http://box.example:8963        somebody's HTTP box
//! s3://ACCOUNT.r2.cloudflarestorage.com/bucket/prefix?region=auto
//! carry://remote:drops           whatever KUSANAGI_CARRIER runs, under a prefix
//! ```
//!
//! **A `carry://` locator names a prefix and never a program.** The program is
//! this machine's own `KUSANAGI_CARRIER`; a locator arrives inside somebody
//! else's invitation, and a locator that could name a program would be remote
//! code execution spelled as configuration. See `carrier.rs`.
//!
//! [`Place`] is the only place in the workspace that knows more than one adapter
//! exists, and it answers "not offered" on behalf of the ones that cannot do a
//! thing — which is why `doctor` can examine a plain directory without either
//! special-casing it or lying about it.

use kusanagi_kernel::{Listing, Object, PutOutcome, Sweep, Waypoint, WaypointError};

use crate::access::Access;
use crate::carrier::CarrierWaypoint;
use crate::conditional::{Conditional, Fetched, TtlOutcome, Validator};
use crate::dir::DirWaypoint;
use crate::http::HttpWaypoint;
use crate::locator::{Locator, LocatorError};
use crate::s3::S3Waypoint;

/// An opened place, ready to hold bytes.
#[derive(Debug)]
pub enum Place {
    /// A directory on this machine.
    Directory(DirWaypoint),
    /// Somebody's HTTP box.
    Box(HttpWaypoint),
    /// A bucket on an S3-compatible host.
    Bucket(S3Waypoint),
    /// Somewhere this machine's own carrier program puts things.
    Carried(CarrierWaypoint),
}

impl Place {
    /// Opens what `locator` names.
    ///
    /// `access` and `now` arrive from the assembly rather than being read here,
    /// because reading the environment and reading the clock are both things
    /// exactly one module in this program is allowed to do.
    ///
    /// A directory ignores both halves of `access`: nothing leaves the machine,
    /// so there is no socket to route and nothing to sign.
    ///
    /// # Errors
    ///
    /// [`LocatorError::CredentialsMissing`] when a bucket is named without them.
    pub fn open(locator: &Locator, access: &Access, now: u64) -> Result<Self, LocatorError> {
        match locator {
            Locator::Directory(path) => Ok(Self::Directory(DirWaypoint::new(path))),
            Locator::Box { base } => Ok(Self::Box(HttpWaypoint::new(base, access))),
            Locator::Bucket {
                endpoint,
                bucket,
                prefix,
                region,
            } => {
                let credentials = access
                    .credentials
                    .clone()
                    .ok_or(LocatorError::CredentialsMissing)?;
                Ok(Self::Bucket(S3Waypoint::new(
                    endpoint,
                    bucket,
                    prefix,
                    region,
                    credentials,
                    access,
                    now,
                )))
            }
            Locator::Carrier { prefix } => {
                let carrier = access.carrier.clone().ok_or(LocatorError::CarrierMissing)?;
                Ok(Self::Carried(CarrierWaypoint::new(carrier, prefix)))
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
            Self::Carried(_) => "carrier",
        }
    }
}

impl Waypoint for Place {
    fn put_if_absent(&self, at: &Object, bytes: &[u8]) -> Result<PutOutcome, WaypointError> {
        match self {
            Self::Directory(place) => place.put_if_absent(at, bytes),
            Self::Box(place) => place.put_if_absent(at, bytes),
            Self::Bucket(place) => place.put_if_absent(at, bytes),
            Self::Carried(place) => place.put_if_absent(at, bytes),
        }
    }

    fn get(&self, at: &Object) -> Result<Option<Vec<u8>>, WaypointError> {
        match self {
            Self::Directory(place) => place.get(at),
            Self::Box(place) => place.get(at),
            Self::Bucket(place) => place.get(at),
            Self::Carried(place) => place.get(at),
        }
    }

    fn delete(&self, at: &Object) -> Result<(), WaypointError> {
        match self {
            Self::Directory(place) => place.delete(at),
            Self::Box(place) => place.delete(at),
            Self::Bucket(place) => place.delete(at),
            Self::Carried(place) => place.delete(at),
        }
    }
}

impl Listing for Place {
    /// Every kind of place lists, because a read that could not would have to
    /// name an address again.
    ///
    /// That is why there is no arm here answering
    /// [`WaypointError::ListingRefused`]: the refusal exists for an adapter
    /// written outside this repository, and every adapter inside it pays the
    /// cost of the property instead of declining it.
    fn list(&self, sweep: &Sweep) -> Result<Vec<Object>, WaypointError> {
        match self {
            Self::Directory(place) => place.list(sweep),
            Self::Box(place) => place.list(sweep),
            Self::Bucket(place) => place.list(sweep),
            Self::Carried(place) => place.list(sweep),
        }
    }
}

impl Conditional for Place {
    fn get_if_changed(
        &self,
        at: &Object,
        known: Option<&Validator>,
    ) -> Result<Fetched, WaypointError> {
        match self {
            // Neither a directory nor a carrier has validators or a cheap
            // "unchanged" answer, so both always send the bytes. Saying that
            // plainly is what lets one `doctor` run describe every kind of host.
            Self::Directory(place) => {
                Ok(place
                    .get(at)?
                    .map_or(Fetched::Absent, |bytes| Fetched::Fresh {
                        bytes,
                        validator: None,
                    }))
            }
            Self::Carried(place) => {
                Ok(place
                    .get(at)?
                    .map_or(Fetched::Absent, |bytes| Fetched::Fresh {
                        bytes,
                        validator: None,
                    }))
            }
            Self::Box(place) => place.get_if_changed(at, known),
            Self::Bucket(place) => place.get_if_changed(at, known),
        }
    }

    fn put_with_ttl(
        &self,
        at: &Object,
        bytes: &[u8],
        seconds: u64,
    ) -> Result<TtlOutcome, WaypointError> {
        match self {
            Self::Directory(place) => {
                place.put_if_absent(at, bytes)?;
                Ok(TtlOutcome::NotOffered)
            }
            Self::Carried(place) => {
                place.put_if_absent(at, bytes)?;
                Ok(TtlOutcome::NotOffered)
            }
            Self::Box(place) => place.put_with_ttl(at, bytes, seconds),
            Self::Bucket(place) => place.put_with_ttl(at, bytes, seconds),
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
    /// A scheme this build does not speak is refused rather than filed as a
    /// relative directory. Measuring `ftp://host` as a directory answered four
    /// questions about a filename that nobody had asked.
    #[test]
    fn a_scheme_this_build_does_not_know_is_refused() {
        for text in ["ftp://host/bucket", "s4://host", "gopher://host/x"] {
            let refused = text.parse::<Locator>().unwrap_err();
            assert_eq!(refused.code(), "locator.unknown_scheme", "{text}");
        }
        // A path that merely contains a colon is still a path.
        assert!(matches!(
            "C:/drops".parse::<Locator>(),
            Ok(Locator::Directory(_))
        ));
    }

    use super::Place;
    use crate::access::Access;
    use crate::locator::{Locator, LocatorError};
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
            Locator::from_str("http://box.example:8963/").unwrap(),
            Locator::Box {
                base: "http://box.example:8963".to_owned()
            }
        );
    }

    /// An onion service is a box like any other: the name is handed to the
    /// SOCKS proxy unresolved (`socks5h`, `hostile_host.rs`), which is what
    /// lets Tor find it, and nothing here treats the suffix specially.
    #[test]
    fn an_onion_service_is_a_box() {
        assert_eq!(
            Locator::from_str("http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz234567abcd.onion:8963").unwrap(),
            Locator::Box {
                base: "http://abcdefghijklmnopqrstuvwxyz234567abcdefghijklmnopqrstuvwxyz234567abcd.onion:8963".to_owned()
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
            Place::open(&locator, &Access::default(), 0).unwrap_err(),
            LocatorError::CredentialsMissing
        );
    }

    #[test]
    fn every_kind_names_itself() {
        let directory = Place::open(
            &Locator::from_str("./drops").unwrap(),
            &Access::default(),
            0,
        )
        .unwrap();
        assert_eq!(directory.kind(), "directory");
        let boxed = Place::open(
            &Locator::from_str("http://host").unwrap(),
            &Access::default(),
            0,
        )
        .unwrap();
        assert_eq!(boxed.kind(), "http box");
    }
}
