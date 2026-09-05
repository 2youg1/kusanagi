// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Whether this site may reach a host without a proxy.
//!
//! `KUSANAGI_PROXY` is an environment variable, and an environment variable is
//! the easiest thing on a machine to lose: a new shell, a scheduler task, a
//! service unit written in a hurry. A privacy setting that silently fails open
//! when it is missing is worse than one nobody offered, so the site can record
//! that it must never leave without one, and every verb that would open a
//! host then refuses instead of going direct.

use std::path::Path;

use crate::error::SiteError;
use crate::permissions;
use crate::site::Site;

/// How this site is allowed to reach a host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Egress {
    /// Through a proxy when one is configured, directly otherwise. The default.
    Free,
    /// Through a proxy, or not at all.
    ProxyRequired,
}

const FILE: &str = "egress";
const REQUIRED: &[u8] = b"proxy-required";

/// What the site has recorded; [`Egress::Free`] when it recorded nothing.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be read, and
/// [`SiteError::BadRecord`] when it holds a word this build does not know.
fn read(root: &Path) -> Result<Egress, SiteError> {
    let Some(bytes) = permissions::read(&root.join(FILE), "read the egress record")? else {
        return Ok(Egress::Free);
    };
    if bytes.trim_ascii() == REQUIRED {
        Ok(Egress::ProxyRequired)
    } else {
        Err(SiteError::BadRecord {
            what: "the egress record",
            reason: "this file does not say `proxy-required`".to_owned(),
        })
    }
}

/// Records `egress`, replacing whatever was recorded.
///
/// # Errors
///
/// [`SiteError::Local`] when the record cannot be written.
fn write(root: &Path, egress: Egress) -> Result<(), SiteError> {
    permissions::create_dir(root, "create the site directory")?;
    match egress {
        Egress::ProxyRequired => {
            permissions::write(&root.join(FILE), REQUIRED, "write the egress record")
        }
        Egress::Free => match std::fs::remove_file(root.join(FILE)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(source) => Err(SiteError::Local {
                action: "remove the egress record",
                source,
            }),
        },
    }
}

impl Site {
    /// How this site may reach a host; see [`Egress`].
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be read, and
    /// [`SiteError::BadRecord`] when it holds a word this build does not know.
    pub fn egress(&self) -> Result<Egress, SiteError> {
        read(&self.root)
    }

    /// Records how this site may reach a host.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn set_egress(&self, egress: Egress) -> Result<(), SiteError> {
        write(&self.root, egress)
    }
}
