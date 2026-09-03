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
//! <root>/identity                 32 bytes: this endpoint's signing seed
//! <root>/channels/<name>          one channel record
//! <root>/cairns/<name>/<author>   how far that author's stream is verified
//! <root>/revoked                  one revoked step identifier per line
//! ```
//!
//! Three of those four are facts this endpoint cannot recompute. A cairn is the
//! exception: it can always be rebuilt by reading the stream again from height
//! zero, which is why every way of failing to read one is treated as not having
//! one. Losing every cairn costs requests and privacy, never correctness.
//!
//! Channel names are checked rather than escaped. A name is a path component
//! here, and the set of characters that are safe in a path component on every
//! system worth supporting is small enough to just say out loud.
//!
//! Nothing here calls `fs::write` or `fs::create_dir_all` directly. Every write
//! goes through `permissions`, which is the one place that decides who else on
//! this machine can read a channel secret.

use std::fs;
use std::path::{Path, PathBuf};

use kusanagi_chain::Cairn;
use kusanagi_grant::{Revocations, StepId};
use kusanagi_kernel::{Handle, Signer};

use crate::channel::Channel;
use crate::error::SiteError;
use crate::permissions;

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
    /// [`SiteError::Local`] when the file exists and cannot be read, and
    /// [`SiteError::BadRecord`] when it is not a seed.
    pub fn identity(&self) -> Result<Option<Signer>, SiteError> {
        let path = self.root.join("identity");
        match fs::read(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SiteError::Local {
                action: "read this endpoint's identity",
                source,
            }),
            Ok(bytes) => {
                let seed =
                    <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| SiteError::BadRecord {
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
    /// [`SiteError::Local`] when the file cannot be written.
    pub fn adopt(&self, seed: &[u8; 32]) -> Result<Signer, SiteError> {
        if let Some(existing) = self.identity()? {
            return Ok(existing);
        }
        self.make_root()?;
        permissions::write_new(&self.root.join("identity"), seed, "write an identity")?;
        Ok(Signer::from_seed(seed))
    }

    /// Reads one channel.
    ///
    /// # Errors
    ///
    /// [`SiteError::UnknownChannel`] when there is no such channel.
    pub fn channel(&self, name: &str) -> Result<Channel, SiteError> {
        let path = self.channel_path(name)?;
        match fs::read(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                Err(SiteError::UnknownChannel {
                    name: name.to_owned(),
                })
            }
            Err(source) => Err(SiteError::Local {
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
    /// [`SiteError::BadName`] when the name is not usable as one.
    pub fn holds(&self, name: &str) -> Result<bool, SiteError> {
        Ok(self.channel_path(name)?.exists())
    }

    /// Writes one channel, replacing what was there.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the file cannot be written.
    pub fn keep(&self, name: &str, channel: &Channel) -> Result<(), SiteError> {
        let path = self.channel_path(name)?;
        if let Some(parent) = path.parent() {
            permissions::create_dir(parent, "create the channel directory")?;
        }
        permissions::write(&path, &channel.to_bytes(), "write a channel")
    }

    /// How far one author's stream on one channel has been verified.
    ///
    /// **Every way of failing to read a cairn is reported as not having one**,
    /// and that is one rule rather than a swallowed error. A cairn is the only
    /// thing on this disk that can be recomputed — walking the stream from height
    /// zero rebuilds it exactly — so falling back is always correct, while any
    /// other answer would let a torn write, an older build's record, or a
    /// permission fault stop an endpoint from reading a channel at all.
    ///
    /// What that gives up is a signal: an endpoint whose cairns are being deleted
    /// walks from genesis every time and does not complain. It is given up
    /// because refusing would not buy it back — whoever can corrupt a cairn can
    /// delete it, and a deleted cairn is indistinguishable from a channel that
    /// has never been read.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one. That is a caller
    /// mistake rather than a state of the disk, so it is not a miss.
    pub fn cairn(&self, name: &str, author: &Handle) -> Result<Option<Cairn>, SiteError> {
        let path = self.cairn_path(name, author)?;
        Ok(fs::read(&path)
            .ok()
            .and_then(|bytes| Cairn::from_bytes(&bytes).ok()))
    }

    /// Writes down how far `cairn`'s author has been verified on this channel.
    ///
    /// The file is named after the author inside the cairn, so a record cannot
    /// end up describing a stream other than the one it is filed under.
    ///
    /// Unlike reading, a failure here is reported. A miss on read is a cost; a
    /// disk that refuses writes is a fact about this endpoint that its operator
    /// has to learn from something, and staying quiet would mean every later read
    /// pays a full walk with nothing ever saying why.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn mark(&self, name: &str, cairn: &Cairn) -> Result<(), SiteError> {
        let path = self.cairn_path(name, &cairn.author())?;
        if let Some(parent) = path.parent() {
            permissions::create_dir(parent, "create the cairn directory")?;
        }
        permissions::write(&path, &cairn.to_bytes(), "write a cairn")
    }

    /// Deletes one channel record.
    ///
    /// This is the only destructive operation a site has, and what it destroys
    /// is the channel secret: every address on that channel derives from it, so
    /// a forgotten channel cannot be re-entered by any means, including a fresh
    /// copy of the invitation that opened it.
    ///
    /// The revocation list is deliberately left alone. A revoked step has to
    /// outlive the record that mentioned it, or joining the same name again
    /// would bring a revoked grant back to life.
    ///
    /// # Errors
    ///
    /// [`SiteError::UnknownChannel`] when there is no such channel, and
    /// [`SiteError::Local`] when the file cannot be removed.
    pub fn forget(&self, name: &str) -> Result<(), SiteError> {
        let path = self.channel_path(name)?;
        match fs::remove_file(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                Err(SiteError::UnknownChannel {
                    name: name.to_owned(),
                })
            }
            Err(source) => Err(SiteError::Local {
                action: "forget a channel",
                source,
            }),
            Ok(()) => {
                // The cairns go with it. They are recomputable and therefore not
                // worth a failure of their own, but leaving them behind would let
                // a later channel of the same name inherit a stranger's heights.
                fs::remove_dir_all(self.root.join("cairns").join(name)).ok();
                Ok(())
            }
        }
    }

    /// Every channel name here, in a stable order.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the directory cannot be listed.
    pub fn names(&self) -> Result<Vec<String>, SiteError> {
        let dir = self.root.join("channels");
        let entries = match fs::read_dir(&dir) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(SiteError::Local {
                    action: "list the channels",
                    source,
                });
            }
            Ok(entries) => entries,
        };

        let mut names = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| SiteError::Local {
                action: "list the channels",
                source,
            })?;
            // A name a channel can have never starts with a dot — `check_name`
            // allows only `a-z`, `0-9` and `-`. So anything that does is not a
            // channel, and the one thing that produces one is a staged record
            // left behind by a write this process did not live to finish.
            if let Some(name) = entry.file_name().to_str()
                && !name.starts_with('.')
            {
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
    /// [`SiteError::Local`] when the list cannot be read, and
    /// [`SiteError::BadRecord`] when a line is not a step identifier.
    pub fn revocations(&self) -> Result<Revocations, SiteError> {
        let path = self.root.join("revoked");
        let text = match fs::read_to_string(&path) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Revocations::new());
            }
            Err(source) => {
                return Err(SiteError::Local {
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
    /// [`SiteError::Local`] when the list cannot be written.
    pub fn revoke(&self, step: StepId) -> Result<(), SiteError> {
        let revoked = self.revocations()?.revoking(step);
        let lines: Vec<String> = revoked.iter().map(ToString::to_string).collect();
        self.make_root()?;
        permissions::write(
            &self.root.join("revoked"),
            lines.join("\n").as_bytes(),
            "write the revocation list",
        )
    }

    fn make_root(&self) -> Result<(), SiteError> {
        permissions::create_dir(&self.root, "create the site directory")
    }

    fn channel_path(&self, name: &str) -> Result<PathBuf, SiteError> {
        check_name(name)?;
        Ok(self.root.join("channels").join(name))
    }

    /// A handle renders as 64 hexadecimal characters, which needs no checking
    /// against [`check_name`]: it cannot be empty, cannot escape a directory, and
    /// cannot collide with another author.
    fn cairn_path(&self, name: &str, author: &Handle) -> Result<PathBuf, SiteError> {
        check_name(name)?;
        Ok(self.root.join("cairns").join(name).join(author.to_string()))
    }
}

/// Refuses anything that is not plainly a name.
///
/// The rule is deliberately narrower than any filesystem's: a name that is safe
/// in a path, safe in a shell, and safe in a URL is safe everywhere this network
/// might carry it, and the ways of getting escaping wrong all start with allowing
/// something interesting.
///
/// A name may not begin with `-`. Every command line ever written reads a
/// leading hyphen as a flag, and this one reads a bare `-` as "the name arrives
/// on stdin" — so a name that starts with one is a name somebody cannot type.
fn check_name(name: &str) -> Result<(), SiteError> {
    let plain = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-';
    let usable = name.len() <= MAX_NAME
        && name.bytes().all(plain)
        && name.bytes().next().is_some_and(|first| first != b'-');
    if usable {
        return Ok(());
    }
    Err(SiteError::BadName {
        name: name.to_owned(),
        reason: format!(
            "a name is 1 to {MAX_NAME} characters of a-z, 0-9 and -, and does not start with -"
        ),
    })
}
