// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What `kusanagi-site`'s failures become at this door.
//!
//! Apart from the enum because the two change for different reasons: a new
//! failure adds a variant there, and a changed mapping changes only here. The
//! shapes are the same on both sides, and that is the point rather than an
//! accident: `kusanagi-site` says what was being done and what was wrong with
//! the bytes, and this is where that becomes a stable code plus a command a
//! caller can run. Merging the two types would put the words
//! `kusanagi channels` inside a crate that has no verbs.

use kusanagi_site::SiteError;

use super::Complaint;

impl From<SiteError> for Complaint {
    fn from(error: SiteError) -> Self {
        match error {
            SiteError::Local { action, source } => Self::Local { action, source },
            SiteError::Permissions { what, source } => Self::Permissions { what, source },
            SiteError::BadName { name, reason } => Self::BadName { name, reason },
            SiteError::BadInvitation { reason } => Self::BadInvitation { reason },
            SiteError::BadRecord { what, reason } => Self::BadRecord { what, reason },
            SiteError::UnknownChannel { name } => Self::UnknownChannel { name },
            SiteError::NoIdentity => Self::NoIdentity,
            SiteError::BadRecovery => Self::BadRecovery,
            SiteError::ForeignRecord { tag } => Self::ForeignRecord { tag },
            SiteError::Grant(error) => Self::Grant(error),
        }
    }
}
