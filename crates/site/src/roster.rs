// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Which channels one name stands for.
//!
//! **A group here is a list, not a cryptographic object**, and `site-SPEC.md`
//! §14 says why that decides everything else about this file. Plain text in the
//! shape of `revoked`: the first line is what this endpoint calls the group and
//! every line after it is a member channel, with no length prefixes and no
//! escaping, because `naming::check` has already confined a name to `a-z`, `0-9`
//! and `-`, none of which is a newline.

use std::path::{Path, PathBuf};

use crate::error::SiteError;
use crate::permissions;
use crate::records;

/// A group: what it is called here, and the channels a message to it reaches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Roster {
    /// What this endpoint calls the group.
    pub name: String,
    /// The channels it fans out to, in the order they were given.
    pub members: Vec<String>,
}

impl Roster {
    /// The bytes this roster is stored as.
    #[must_use]
    pub(crate) fn to_bytes(&self) -> Vec<u8> {
        let mut text = self.name.clone();
        for member in &self.members {
            text.push('\n');
            text.push_str(member);
        }
        text.into_bytes()
    }

    /// Reads a roster back, checking it is filed under the name it carries.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadRecord`] when the bytes are not text, hold no name, or
    /// call themselves something other than `filed_as`.
    pub(crate) fn from_bytes(bytes: &[u8], filed_as: &str) -> Result<Self, SiteError> {
        let text = String::from_utf8(bytes.to_vec()).map_err(|_| SiteError::BadRecord {
            what: "a group",
            reason: "this record is not text".to_owned(),
        })?;
        let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
        let name = lines.next().unwrap_or_default().to_owned();
        if name != filed_as {
            return Err(SiteError::BadRecord {
                what: "a group",
                reason: format!("this record is filed as `{filed_as}` and calls itself `{name}`"),
            });
        }
        Ok(Self {
            name,
            members: lines.map(ToOwned::to_owned).collect(),
        })
    }
}

/// Where one group's roster sits.
fn path(root: &Path, filed: &str) -> PathBuf {
    root.join("groups").join(filed)
}

/// One group's roster, or `None` when this endpoint has no such group.
///
/// # Errors
///
/// [`SiteError::Local`] when the file cannot be read, and
/// [`SiteError::BadRecord`] when it does not decode.
pub(crate) fn read(root: &Path, filed: &str, name: &str) -> Result<Option<Roster>, SiteError> {
    permissions::read(&path(root, filed), "read a group")?
        .map(|bytes| Roster::from_bytes(&bytes, name))
        .transpose()
}

/// Replaces one group's roster, creating it if there was none.
///
/// # Errors
///
/// [`SiteError::Local`] when the file cannot be written.
pub(crate) fn write(root: &Path, filed: &str, roster: &Roster) -> Result<(), SiteError> {
    let at = path(root, filed);
    if let Some(parent) = at.parent() {
        permissions::create_dir(parent, "create the group directory")?;
    }
    permissions::write(&at, &roster.to_bytes(), "write a group")
}

/// Every group here, with its members, in a stable order.
///
/// Each roster is read whole, because the file is not named after the group and
/// the caller that lists them wants their members anyway. The name a record is
/// checked against is its own first line, which is all a listing has: the filed
/// name proves nothing without the name it was hashed from.
///
/// # Errors
///
/// [`SiteError::Local`] when a record cannot be read, and
/// [`SiteError::BadRecord`] when one does not decode.
pub(crate) fn all(root: &Path) -> Result<Vec<Roster>, SiteError> {
    let mut rosters = records::each(root, "groups", "list the groups")?
        .iter()
        .map(|bytes| {
            let text = String::from_utf8_lossy(bytes);
            let named = text.lines().next().unwrap_or_default().trim().to_owned();
            Roster::from_bytes(bytes, &named)
        })
        .collect::<Result<Vec<Roster>, SiteError>>()?;
    rosters.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(rosters)
}
