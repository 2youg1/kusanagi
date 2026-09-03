// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Why local state could not be read, written, or believed.
//!
//! Three shapes and no more, because there are only three ways this layer can
//! fail: the operating system refused, the bytes are not what they claim to be,
//! or the thing asked for is not here. Each carries what the layer above needs to
//! phrase a way out — the action attempted, the structure at fault, the name that
//! was asked for — and none of them carries the way out itself. The door owns
//! that, and `kusanagi::Complaint` is where it is written.

use kusanagi_grant::GrantError;
use kusanagi_kernel::{DigestParseError, HexError};

/// Why a site operation did not happen.
///
/// Deliberately **not** `#[non_exhaustive]`, unlike the failure types beneath it.
/// The door matches this enum arm by arm to assign a stable code and a recovery
/// command, and a fourth way to fail that nobody has priced is exactly what
/// should stop the build until somebody does.
#[derive(Debug, thiserror::Error)]
pub enum SiteError {
    /// The operating system refused a read or a write.
    #[error("could not {action}: {source}")]
    Local {
        /// What was being attempted, in the words a person would use.
        action: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// A name this endpoint was asked to use is not one it can use.
    ///
    /// Apart from the two below because the way out is different: a name is
    /// something the caller typed and can retype, and telling them anything
    /// about invitations would send them looking for a thing they never had.
    #[error("a channel name is malformed: {reason}")]
    BadName {
        /// What was typed.
        name: String,
        /// Why it cannot be used.
        reason: String,
    },
    /// A line offered as an invitation is not one.
    #[error("an invitation is malformed: {reason}")]
    BadInvitation {
        /// What was wrong with it.
        reason: String,
    },
    /// Bytes already on this disk are not the structure they claim to be.
    #[error("{what} is malformed: {reason}")]
    BadRecord {
        /// Which structure.
        what: &'static str,
        /// What was wrong with it.
        reason: String,
    },
    /// No channel by that name is here.
    #[error("there is no channel called `{name}` here")]
    UnknownChannel {
        /// The name that was asked for.
        name: String,
    },
    /// A channel was to be written before this endpoint had an identity.
    ///
    /// Channel files are filed under a name derived from the identity seed, so
    /// there is nothing to file one under until the identity exists. Reading is
    /// not affected: a site with no identity provably holds no channels, and
    /// says so as [`SiteError::UnknownChannel`].
    #[error("this endpoint has no identity yet, so it cannot keep a channel")]
    NoIdentity,
    /// The operating system would not attach the restriction a site needs.
    ///
    /// Apart from [`SiteError::Local`] because the way out is different: this is
    /// not "the write failed", it is "the write would have succeeded and left
    /// the bytes readable by somebody else", so it refuses instead. A filesystem
    /// with no access lists at all — FAT32, exFAT, most network shares — reaches
    /// here rather than quietly writing an open file.
    #[error("could not {what}: {source}")]
    Permissions {
        /// What was being attempted, in the words a person would use.
        what: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// An archive did not open under the recovery key that was offered.
    ///
    /// One answer for every reason it could fail — a wrong key, a damaged file,
    /// an archive from somebody else — because an attacker who learns *why* a
    /// key was wrong has been handed a way to test guesses one at a time.
    #[error("this archive did not open under that recovery key")]
    BadRecovery,
    /// A grant inside a record or an invitation does not decode.
    #[error(transparent)]
    Grant(#[from] GrantError),
}

/// Sealed bytes that will not open are one answer here, whatever went wrong.
impl From<kusanagi_seal::OpenFailed> for SiteError {
    fn from(_: kusanagi_seal::OpenFailed) -> Self {
        Self::BadRecovery
    }
}

impl From<HexError> for SiteError {
    fn from(error: HexError) -> Self {
        Self::BadInvitation {
            reason: error.to_string(),
        }
    }
}

impl From<DigestParseError> for SiteError {
    fn from(error: DigestParseError) -> Self {
        Self::BadRecord {
            what: "the revocation list",
            reason: error.to_string(),
        }
    }
}
