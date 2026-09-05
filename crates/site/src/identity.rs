// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Who this endpoint is, and which corner of a host it reads.
//!
//! ```text
//! version   1 byte
//! seed     32 bytes   the whole of who this endpoint is
//! ward      2 bytes   which bin of a host its writers file into
//! ```
//!
//! **The ward is here rather than in each channel record because it belongs to
//! the reader, not to the conversation.** One ward serves every channel this
//! endpoint has: a sweep of it collects the segments of all of them at once, so
//! polling costs what one channel costs no matter how many there are. A ward
//! chosen per channel would put each conversation in its own corner and hand the
//! host back the very thing the bin exists to hide — a crowd of one.
//!
//! It is random and derived from nothing. A ward computed from a handle would
//! let a host work out whose corner it was looking at, and a ward chosen by a
//! writer would let one writer decide which crowd its reader stands in.
//!
//! **Version 1 is the first identity record with any structure at all.** What
//! came before was thirty-two bare bytes, so a site made by an older build is
//! refused by length with the one instruction that recovers it. Before the first
//! release that is the whole migration story; `export` and `import` carry the
//! ward from version 1 onwards.

use kusanagi_kernel::{Signer, Ward};

use crate::site::Site;
use kusanagi_vault as vault;

use crate::error::SiteError;

/// The record this build writes and reads.
const VERSION: u8 = 1;

/// How many bytes one identity record is.
const WIDTH: usize = 35;

/// Everything the identity file holds.
///
/// Not `Clone` and not `Copy`: the seed is this endpoint, and a value that
/// duplicates itself by accident is a seed in a page nobody erases.
#[derive(Debug)]
pub(crate) struct Identity {
    /// The whole of who this endpoint is.
    pub(crate) seed: [u8; 32],
    /// Which bin of a host this endpoint's writers file into.
    pub(crate) ward: Ward,
}

impl Identity {
    /// The bytes this record is stored as.
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(WIDTH);
        out.push(VERSION);
        out.extend_from_slice(&self.seed);
        out.extend_from_slice(&self.ward.bits().to_be_bytes());
        out
    }

    /// Reads what [`Identity::to_bytes`] wrote.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadRecord`] for any other bytes, including the thirty-two
    /// bare ones an older build wrote — refused rather than read as a ward of
    /// zero, because every endpoint that guessed the same would land in one bin
    /// together.
    pub(crate) fn from_bytes(bytes: &[u8]) -> Result<Self, SiteError> {
        let malformed = |reason: String| SiteError::BadRecord {
            what: "an identity file",
            reason,
        };
        if bytes.len() == 32 {
            return Err(malformed(
                "this identity was written before identities carried a ward; \
                 export it with the build that made it and import it here"
                    .to_owned(),
            ));
        }
        let (Some(version), true) = (bytes.first(), bytes.len() == WIDTH) else {
            return Err(malformed(format!(
                "an identity record is {WIDTH} bytes; this one is {}",
                bytes.len()
            )));
        };
        if *version != VERSION {
            return Err(malformed(format!(
                "this identity is version {version}, and this build reads {VERSION}"
            )));
        }
        let (Some(seed), Some(ward)) = (bytes.get(1..33), bytes.get(33..35)) else {
            return Err(malformed("an identity record is truncated".to_owned()));
        };
        let (Ok(seed), Ok(ward)) = (<[u8; 32]>::try_from(seed), <[u8; 2]>::try_from(ward)) else {
            return Err(malformed("an identity record is truncated".to_owned()));
        };
        Ok(Self {
            seed,
            ward: Ward::from_bits(u16::from_be_bytes(ward)),
        })
    }
}

impl Site {
    /// This endpoint's identity, if it has one yet.
    ///
    /// Expanding the seed into a signing key is the most expensive thing this
    /// crate does, so anything that needs the seed rather than the signer takes
    /// [`Site::seed`] and does not pay for it.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the file exists and cannot be read, and
    /// [`SiteError::BadRecord`] when it is not a seed.
    pub fn identity(&self) -> Result<Option<Signer>, SiteError> {
        Ok(self.seed()?.as_ref().map(Signer::from_seed))
    }

    /// Which bin of a host this endpoint's writers file into.
    ///
    /// One ward for the whole site, so that one sweep collects every channel.
    /// `None` when there is no identity, which is also when there is nothing to
    /// sweep for.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the file exists and cannot be read, and
    /// [`SiteError::BadRecord`] when it is not an identity record.
    pub fn ward(&self) -> Result<Option<Ward>, SiteError> {
        Ok(self.record()?.map(|identity| identity.ward))
    }

    /// The identity record, decoded once for every view of it.
    fn record(&self) -> Result<Option<Identity>, SiteError> {
        let Some(bytes) =
            vault::read(&self.root.join("identity"), "read this endpoint's identity")?
        else {
            return Ok(None);
        };
        Identity::from_bytes(&bytes).map(Some)
    }

    /// The 32 bytes in the identity file, if there are any.
    ///
    /// `pub(crate)` and nothing wider. The seed **is** this endpoint, so the one
    /// caller outside this file is `archive`, which puts it in a sealed backup —
    /// the one place it is meant to leave the disk.
    pub(crate) fn seed(&self) -> Result<Option<[u8; 32]>, SiteError> {
        Ok(self.record()?.map(|identity| identity.seed))
    }

    /// Writes `seed` and `ward` as this endpoint's identity and returns the signer.
    ///
    /// Refuses to replace an identity that already exists: overwriting one
    /// abandons every channel it holds, silently and irreversibly.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the file cannot be written.
    pub fn adopt(&self, seed: &[u8; 32], ward: Ward) -> Result<Signer, SiteError> {
        if let Some(existing) = self.identity()? {
            return Ok(existing);
        }
        self.make_root()?;
        let identity = Identity { seed: *seed, ward };
        vault::write_new(
            &self.root.join("identity"),
            &identity.to_bytes(),
            "write an identity",
        )?;
        Ok(Signer::from_seed(seed))
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
    use super::{Identity, VERSION, WIDTH};
    use kusanagi_kernel::Ward;

    #[test]
    fn an_identity_survives_a_round_trip() {
        let identity = Identity {
            seed: [0x5a; 32],
            ward: Ward::from_bits(0x3c5a),
        };
        let read = Identity::from_bytes(&identity.to_bytes()).unwrap();
        assert_eq!(read.seed, identity.seed);
        assert_eq!(read.ward, identity.ward);
    }

    #[test]
    fn the_thirty_two_bare_bytes_of_an_older_build_are_refused_by_name() {
        let error = Identity::from_bytes(&[0x5a; 32]).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("export it with the build that made it"),
            "an old identity must say how to move it, not just that it is wrong: {error}"
        );
    }

    #[test]
    fn a_version_this_build_does_not_know_is_refused_rather_than_guessed() {
        let mut bytes = Identity {
            seed: [1; 32],
            ward: Ward::from_bits(9),
        }
        .to_bytes();
        bytes[0] = VERSION + 1;
        assert!(Identity::from_bytes(&bytes).is_err());
        assert!(Identity::from_bytes(&bytes[..WIDTH - 1]).is_err());
        assert!(Identity::from_bytes(&[]).is_err());
    }
}
