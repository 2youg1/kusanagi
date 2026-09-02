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
    /// Stored or supplied bytes are not the structure they claim to be.
    #[error("{what} is malformed: {reason}")]
    Malformed {
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
    /// A grant inside a record or an invitation does not decode.
    #[error(transparent)]
    Grant(#[from] GrantError),
}

impl From<HexError> for SiteError {
    fn from(error: HexError) -> Self {
        Self::Malformed {
            what: "an invitation",
            reason: error.to_string(),
        }
    }
}

impl From<DigestParseError> for SiteError {
    fn from(error: DigestParseError) -> Self {
        Self::Malformed {
            what: "an identifier",
            reason: error.to_string(),
        }
    }
}
