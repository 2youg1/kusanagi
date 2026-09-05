// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What one string names, before anything has been opened.
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
//! Apart from `place.rs` because parsing a string and opening a socket fail for
//! different reasons and change for different reasons: a new scheme is a variant
//! here, and a new adapter is a variant there.
//!
//! **A `carry://` locator names a prefix and never a program.** The program is
//! this machine's own `KUSANAGI_CARRIER`; a locator arrives inside somebody
//! else's invitation, and a locator that could name a program would be remote
//! code execution spelled as configuration. See `carrier.rs`.

use std::path::PathBuf;
use std::str::FromStr;

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
    /// Wherever this machine's configured carrier puts things, under a prefix.
    Carrier {
        /// What the carrier is asked to prepend to every address.
        prefix: String,
    },
}

/// The scheme a string announces, if it announces one.
///
/// A scheme is letters, digits, `+`, `-` or `.` before `://`, which no path on
/// any system this runs on begins with.
fn announced(text: &str) -> Option<&str> {
    let (scheme, _) = text.split_once("://")?;
    let plausible = !scheme.is_empty()
        && scheme.starts_with(|letter: char| letter.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|letter| letter.is_ascii_alphanumeric() || matches!(letter, '+' | '-' | '.'));
    plausible.then_some(scheme)
}

impl FromStr for Locator {
    type Err = LocatorError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if let Some(path) = text.strip_prefix("file:") {
            return directory(path);
        }
        if text.starts_with("http://") || text.starts_with("https://") {
            return Ok(Self::Box {
                base: text.trim_end_matches('/').to_owned(),
            });
        }
        if let Some(rest) = text.strip_prefix("s3://") {
            return parse_bucket(rest);
        }
        if let Some(prefix) = text.strip_prefix("carry://") {
            return Ok(Self::Carrier {
                prefix: prefix.to_owned(),
            });
        }
        if text.is_empty() {
            return Err(LocatorError::Empty);
        }
        // Anything else is a path — except a string that plainly announces a
        // scheme. Treating `ftp://host` as a relative directory produced four
        // measured failures about a filename, which is a true answer to a
        // question nobody asked.
        if let Some(scheme) = announced(text) {
            return Err(LocatorError::UnknownScheme {
                scheme: scheme.to_owned(),
            });
        }
        directory(text)
    }
}

/// A directory locator, provided it is a directory and not a network.
///
/// A locator arrives inside somebody else's invitation and is opened on this
/// machine, so a path the operating system resolves over the network — a UNC
/// name, or the `//host/share` spelling of one — is a connection the inviter
/// chose, made outside any proxy this program was told to use, and on Windows
/// carrying this account's credentials. A share is still usable as a dead
/// drop: the person mounts it, and the program sees a drive letter.
fn directory(text: &str) -> Result<Locator, LocatorError> {
    let networked = text.starts_with("\\\\") || text.starts_with("//");
    if networked {
        return Err(LocatorError::NetworkPath);
    }
    Ok(Locator::Directory(PathBuf::from(text)))
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
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
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
    /// The locator rides a carrier and this machine has not been told of one.
    #[error("this locator rides a carrier; set KUSANAGI_CARRIER to the program that moves bytes")]
    CarrierMissing,
    /// `KUSANAGI_CARRIER` is set to something that is not a program.
    #[error("that is not a carrier: {reason}")]
    BadCarrier {
        /// What was wrong with it.
        reason: String,
    },
    /// The locator names a scheme this build does not speak.
    #[error("`{scheme}://` is not a kind of waypoint this build knows")]
    UnknownScheme {
        /// The scheme as it was written.
        scheme: String,
    },
    /// The locator is a path the operating system would reach over the network.
    #[error(
        "a waypoint directory cannot be a network path: opening one is a connection \
         the inviter chose, outside any proxy, carrying this account's credentials"
    )]
    NetworkPath,
    /// A proxy was configured and is not one.
    #[error("that is not a proxy: {reason}")]
    BadProxy {
        /// What the client library said was wrong with it.
        reason: String,
    },
}

impl LocatorError {
    /// A stable code for machine callers. Never changes once published.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Empty => "locator.empty",
            Self::BucketIncomplete => "locator.bucket_incomplete",
            Self::CredentialsMissing => "locator.credentials_missing",
            Self::CarrierMissing => "locator.carrier_missing",
            Self::BadCarrier { .. } => "locator.bad_carrier",
            Self::UnknownScheme { .. } => "locator.unknown_scheme",
            Self::NetworkPath => "locator.network_path",
            Self::BadProxy { .. } => "locator.bad_proxy",
        }
    }
}
