// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Why the operating system would not hold a file the way this crate asks.
//!
//! Three shapes, one per answer the platform can give: it refused the operation,
//! it would have performed the operation and left the bytes readable by somebody
//! else, or the bytes it handed back were sealed by a store this build has not.
//! None of them carries a recovery command; the crate above phrases those in its
//! own vocabulary, and `kusanagi::Complaint` phrases them in verbs.

/// Why a vault operation did not happen.
///
/// Deliberately **not** `#[non_exhaustive]`. Every caller maps this enum arm by
/// arm onto its own failures, and a fourth way to fail that nobody has priced is
/// exactly what should stop the build until somebody does.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    /// The operating system refused a read or a write.
    #[error("could not {action}: {source}")]
    Local {
        /// What was being attempted, in the words a person would use.
        action: &'static str,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// The operating system would not attach the restriction a vault needs.
    ///
    /// Apart from [`VaultError::Local`] because the way out is different: this
    /// is not "the write failed", it is "the write would have succeeded and left
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
    /// A record on this disk was sealed by a platform store this one has not.
    ///
    /// A vault outlives the machine it was made on. The tag in front of every
    /// record says which store sealed it, and a build with no such store refuses
    /// by name rather than handing back a blob as though it were a record.
    #[error("this record was sealed by a store this platform does not have (tag {tag:#04x})")]
    ForeignRecord {
        /// The tag the record carries.
        tag: u8,
    },
}
