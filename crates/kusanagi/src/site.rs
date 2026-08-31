// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Everything this endpoint keeps on its own disk.
//!
//! There is no daemon and no database. A site is a directory holding one seed,
//! one file per channel, and a list of revoked steps — so killing any command at
//! any point loses at most the command, and running two of them at once cannot
//! corrupt a third thing that does not exist.
//!
//! ```text
//! <root>/identity          32 bytes: this endpoint's signing seed
//! <root>/channels/<name>   one channel record
//! <root>/revoked           one revoked step identifier per line
//! ```
//!
//! Channel names are checked rather than escaped. A name is a path component
//! here, and the set of characters that are safe in a path component on every
//! system worth supporting is small enough to just say out loud.

use std::fs;
use std::path::{Path, PathBuf};

use kusanagi_grant::{Revocations, StepId};
use kusanagi_kernel::{Handle, Signer};

use crate::channel::Channel;
use crate::complaint::Complaint;

/// The longest a channel name may be.
const MAX_NAME: usize = 32;

/// One endpoint's local state.
#[derive(Debug, Clone)]
pub struct Site {
    root: PathBuf,
}

impl Site {
    /// Uses `root` as the site. Nothing is created until something is written.
    #[must_use]
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Where this site lives.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// This endpoint's identity, if it has one yet.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the file exists and cannot be read, and
    /// [`Complaint::Malformed`] when it is not a seed.
    pub fn identity(&self) -> Result<Option<Signer>, Complaint> {
        let path = self.root.join("identity");
        match fs::read(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(Complaint::Local {
                action: "read this endpoint's identity",
                source,
            }),
            Ok(bytes) => {
                let seed =
                    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| Complaint::Malformed {
                        what: "an identity file",
                        reason: format!("an identity is 32 bytes; this one is {}", bytes.len()),
                    })?;
                Ok(Some(Signer::from_seed(&seed)))
            }
        }
    }

    /// Writes `seed` as this endpoint's identity and returns the signer.
    ///
    /// Refuses to replace an identity that already exists: overwriting one
    /// abandons every channel it holds, silently and irreversibly.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the file cannot be written.
    pub fn adopt(&self, seed: &[u8; 32]) -> Result<Signer, Complaint> {
        if let Some(existing) = self.identity()? {
            return Ok(existing);
        }
        self.make_root()?;
        write_new(&self.root.join("identity"), seed, "write an identity")?;
        Ok(Signer::from_seed(seed))
    }

    /// Reads one channel.
    ///
    /// # Errors
    ///
    /// [`Complaint::UnknownChannel`] when there is no such channel.
    pub fn channel(&self, name: &str) -> Result<Channel, Complaint> {
        let path = self.channel_path(name)?;
        match fs::read(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                Err(Complaint::UnknownChannel {
                    name: name.to_owned(),
                })
            }
            Err(source) => Err(Complaint::Local {
                action: "read a channel",
                source,
            }),
            Ok(bytes) => Channel::from_bytes(&bytes),
        }
    }

    /// Whether a channel of this name is already here.
    ///
    /// # Errors
    ///
    /// [`Complaint::Malformed`] when the name is not usable as one.
    pub fn holds(&self, name: &str) -> Result<bool, Complaint> {
        Ok(self.channel_path(name)?.exists())
    }

    /// Writes one channel, replacing what was there.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the file cannot be written.
    pub fn keep(&self, name: &str, channel: &Channel) -> Result<(), Complaint> {
        let path = self.channel_path(name)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| Complaint::Local {
                action: "create the channel directory",
                source,
            })?;
        }
        fs::write(&path, channel.to_bytes()).map_err(|source| Complaint::Local {
            action: "write a channel",
            source,
        })
    }

    /// Every channel name here, in a stable order.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the directory cannot be listed.
    pub fn names(&self) -> Result<Vec<String>, Complaint> {
        let dir = self.root.join("channels");
        let entries = match fs::read_dir(&dir) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(Complaint::Local {
                    action: "list the channels",
                    source,
                });
            }
            Ok(entries) => entries,
        };

        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| Complaint::Local {
                action: "list the channels",
                source,
            })?;
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_owned());
            }
        }
        names.sort();
        Ok(names)
    }

    /// Every step this endpoint has revoked.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the list cannot be read, and
    /// [`Complaint::Malformed`] when a line is not a step identifier.
    pub fn revocations(&self) -> Result<Revocations, Complaint> {
        let path = self.root.join("revoked");
        let text = match fs::read_to_string(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Revocations::new());
            }
            Err(source) => {
                return Err(Complaint::Local {
                    action: "read the revocation list",
                    source,
                });
            }
            Ok(text) => text,
        };

        let mut revoked = Revocations::new();
        for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
            revoked = revoked.revoking(line.parse::<StepId>()?);
        }
        Ok(revoked)
    }

    /// Adds a step to the revocation list.
    ///
    /// # Errors
    ///
    /// [`Complaint::Local`] when the list cannot be written.
    pub fn revoke(&self, step: StepId) -> Result<(), Complaint> {
        let revoked = self.revocations()?.revoking(step);
        let lines: Vec<String> = revoked.iter().map(ToString::to_string).collect();
        self.make_root()?;
        fs::write(self.root.join("revoked"), lines.join("\n")).map_err(|source| Complaint::Local {
            action: "write the revocation list",
            source,
        })
    }

    fn make_root(&self) -> Result<(), Complaint> {
        fs::create_dir_all(&self.root).map_err(|source| Complaint::Local {
            action: "create the site directory",
            source,
        })
    }

    fn channel_path(&self, name: &str) -> Result<PathBuf, Complaint> {
        check_name(name)?;
        Ok(self.root.join("channels").join(name))
    }
}

/// Refuses anything that is not plainly a name.
///
/// The rule is deliberately narrower than any filesystem's: a name that is safe
/// in a path, safe in a shell, and safe in a URL is safe everywhere this network
/// might carry it, and the ways of getting escaping wrong all start with allowing
/// something interesting.
fn check_name(name: &str) -> Result<(), Complaint> {
    let usable = !name.is_empty()
        && name.len() <= MAX_NAME
        && name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
    if usable {
        return Ok(());
    }
    Err(Complaint::Malformed {
        what: "a channel name",
        reason: format!(
            "a name is 1 to {MAX_NAME} characters of a-z, 0-9 and -, and `{name}` is not"
        ),
    })
}

/// Writes a file that must not already exist.
fn write_new(path: &Path, bytes: &[u8], action: &'static str) -> Result<(), Complaint> {
    use std::io::Write as _;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|source| Complaint::Local { action, source })?;
    file.write_all(bytes)
        .map_err(|source| Complaint::Local { action, source })?;
    file.sync_all()
        .map_err(|source| Complaint::Local { action, source })
}

/// A handle rendered short enough to read, for listings.
#[must_use]
pub fn abbreviate(handle: &Handle) -> String {
    handle.to_string().chars().take(12).collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::Site;
    use crate::channel::{Channel, Standing};
    use kusanagi_grant::StepId;
    use kusanagi_kernel::Signer;
    use kusanagi_seal::Secret;

    fn scratch(tag: &str) -> Site {
        let root = std::env::temp_dir().join(format!("kusanagi-site-{}-{tag}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        Site::at(root)
    }

    fn channel() -> Channel {
        let root = Signer::from_seed(&[1; 32]);
        Channel {
            secret: Secret::from_bytes([7; 32]),
            root: root.handle(),
            introduction: Signer::from_seed(&[2; 32]).handle(),
            locator: "./drops".to_owned(),
            standing: Standing::Root,
            peer: None,
        }
    }

    #[test]
    fn an_identity_is_written_once_and_read_back() {
        let site = scratch("identity");
        assert!(site.identity().unwrap().is_none());

        let first = site.adopt(&[5; 32]).unwrap().handle();
        assert_eq!(site.identity().unwrap().unwrap().handle(), first);

        // a second adoption must not silently replace the first
        assert_eq!(site.adopt(&[6; 32]).unwrap().handle(), first);
        std::fs::remove_dir_all(site.root()).unwrap();
    }

    #[test]
    fn channels_are_kept_and_listed() {
        let site = scratch("channels");
        assert!(site.names().unwrap().is_empty());
        assert!(!site.holds("alice").unwrap());

        site.keep("alice", &channel()).unwrap();
        site.keep("bob", &channel()).unwrap();
        assert_eq!(site.names().unwrap(), vec!["alice", "bob"]);
        assert!(site.holds("alice").unwrap());
        assert_eq!(site.channel("alice").unwrap().locator, "./drops");
        std::fs::remove_dir_all(site.root()).unwrap();
    }

    #[test]
    fn an_unknown_channel_is_named_as_such() {
        let site = scratch("unknown");
        assert_eq!(
            site.channel("nobody").unwrap_err().code(),
            "kusanagi.unknown_channel"
        );
    }

    #[test]
    fn a_name_that_could_escape_the_directory_is_refused() {
        let site = scratch("names");
        for bad in ["../escape", "with/slash", "Upper", "", "with space"] {
            assert_eq!(
                site.channel(bad).unwrap_err().code(),
                "kusanagi.malformed",
                "`{bad}` was accepted as a channel name"
            );
        }
    }

    #[test]
    fn revocations_accumulate_and_survive() {
        let site = scratch("revoked");
        assert!(site.revocations().unwrap().is_empty());

        site.revoke(StepId::from_bytes([1; 32])).unwrap();
        site.revoke(StepId::from_bytes([2; 32])).unwrap();
        site.revoke(StepId::from_bytes([1; 32])).unwrap();

        let revoked = site.revocations().unwrap();
        assert_eq!(revoked.len(), 2);
        assert!(revoked.contains(&StepId::from_bytes([1; 32])));
        std::fs::remove_dir_all(site.root()).unwrap();
    }
}
