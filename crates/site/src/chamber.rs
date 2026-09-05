// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One room on this endpoint's disk: reading it and writing it.
//!
//! Apart from `site.rs` because the two change for different reasons: a site
//! gains a method when a new kind of record arrives, and a room's access gains
//! a rule when rooms learn something new. The filed-name check sits beside the
//! read, the way a channel's does beside its own.

use crate::error::SiteError;
use crate::room::Room;
use crate::site::Site;
use kusanagi_vault as vault;

impl Site {
    /// Reads one room.
    ///
    /// # Errors
    ///
    /// [`SiteError::UnknownChannel`] when there is no such room, and
    /// [`SiteError::BadRecord`] when it does not decode.
    pub fn room(&self, name: &str) -> Result<Room, SiteError> {
        let path = self.room_path(name)?;
        match vault::read(&path, "read a room")? {
            None => Err(SiteError::UnknownChannel {
                name: name.to_owned(),
            }),
            Some(bytes) => {
                let room = Room::from_bytes(&bytes)?;
                if room.name != name {
                    return Err(SiteError::BadRecord {
                        what: "a room",
                        reason: format!(
                            "this record is filed as `{name}` and calls itself `{}`",
                            room.name
                        ),
                    });
                }
                Ok(room)
            }
        }
    }

    /// Writes one room under the name the record carries, replacing what was
    /// there.
    ///
    /// # Errors
    ///
    /// [`SiteError::NoIdentity`] when this endpoint has no identity to file it
    /// under, and [`SiteError::Local`] when the file cannot be written.
    pub fn keep_room(&self, room: &Room) -> Result<(), SiteError> {
        let filed = self.filed(&room.name)?.ok_or(SiteError::NoIdentity)?;
        let path = self.root.join("rooms").join(filed);
        if let Some(parent) = path.parent() {
            vault::create_dir(parent, "create the room directory")?;
        }
        let bytes = room.to_bytes().map_err(|error| SiteError::BadRecord {
            what: "a room",
            reason: error.to_string(),
        })?;
        vault::write(&path, &bytes, "write a room").map_err(Into::into)
    }
}
