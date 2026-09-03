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
//! <root>/channels/<filed>         one channel record
//! <root>/cairns/<filed>/<author>  how far that author's stream is verified
//! <root>/revoked                  one revoked step identifier per line
//! ```
//!
//! **`<filed>` is not the channel's name.** It is a keyed hash of the name under
//! a key only this site can compute, so a directory listing says how many
//! channels there are and nothing about who they are with. The name itself lives
//! inside the record, where reading it already means holding the channel secret.
//!
//! The two facts are different sizes of harm. "This endpoint has three channels"
//! is a count; "they are with bob, carol and dave" is the relationship graph that
//! every derived address in this network exists to hide — and a file name is the
//! part of a file that leaks the most widely, into backup catalogues, sync
//! clients, crash reports and any listing anybody happens to take.
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
use crate::naming;
use crate::permissions;
use crate::revoked;

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

    /// The 32 bytes in the identity file, if there are any.
    ///
    /// `pub(crate)` and nothing wider. The seed **is** this endpoint, so the one
    /// caller outside this file is `archive`, which puts it in a sealed backup —
    /// the one place it is meant to leave the disk.
    pub(crate) fn seed(&self) -> Result<Option<[u8; 32]>, SiteError> {
        match fs::read(self.root.join("identity")) {
            Err(source) if source.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(SiteError::Local {
                action: "read this endpoint's identity",
                source,
            }),
            Ok(bytes) => <[u8; 32]>::try_from(bytes.as_slice())
                .map(Some)
                .map_err(|_| SiteError::BadRecord {
                    what: "an identity file",
                    reason: format!("an identity is 32 bytes; this one is {}", bytes.len()),
                }),
        }
    }

    /// What this site files a channel called `name` under.
    ///
    /// `None` when there is no identity yet, which is also when there are
    /// provably no channels: the key comes from the identity seed, so a site
    /// that has never had one has never written a channel either.
    ///
    /// What it is called instead is [`naming::filed`]'s rule, not this one's.
    fn filed(&self, name: &str) -> Result<Option<String>, SiteError> {
        naming::check(name)?;
        Ok(self.seed()?.map(|seed| naming::filed(&seed, name)))
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
            Ok(bytes) => {
                let channel = Channel::from_bytes(&bytes)?;
                // The record says what it is called and the file says where it
                // was filed; they are derived from each other, so disagreement
                // means the file was moved or written by something else.
                if channel.name != name {
                    return Err(SiteError::BadRecord {
                        what: "a channel",
                        reason: format!(
                            "this record is filed as `{name}` and calls itself `{}`",
                            channel.name
                        ),
                    });
                }
                Ok(channel)
            }
        }
    }

    /// Whether a channel of this name is already here.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when the name is not usable as one.
    pub fn holds(&self, name: &str) -> Result<bool, SiteError> {
        match self.channel_path(name) {
            Ok(path) => Ok(path.exists()),
            Err(SiteError::UnknownChannel { .. }) => Ok(false),
            Err(other) => Err(other),
        }
    }

    /// Writes one channel under the name the record carries, replacing what was
    /// there.
    ///
    /// # Errors
    ///
    /// [`SiteError::NoIdentity`] when this endpoint has no identity to file it
    /// under, and [`SiteError::Local`] when the file cannot be written.
    pub fn keep(&self, channel: &Channel) -> Result<(), SiteError> {
        let filed = self.filed(&channel.name)?.ok_or(SiteError::NoIdentity)?;
        let path = self.root.join("channels").join(filed);
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
                if let Ok(cairns) = self.cairn_dir(name) {
                    fs::remove_dir_all(cairns).ok();
                }
                Ok(())
            }
        }
    }

    /// Every channel name here, in a stable order.
    ///
    /// Each name is read out of its record, because the file is no longer named
    /// after it. That costs one read per channel and buys the property the file
    /// names used to give away; a listing is rare and short, and every caller of
    /// this opens each record immediately afterwards anyway.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the directory cannot be listed or a record
    /// cannot be read, and [`SiteError::BadRecord`] when one does not decode.
    /// A record this build cannot read is reported rather than skipped: a
    /// channel that quietly stops being listed is a channel its owner believes
    /// they no longer have.
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
            // A filed name is 64 hexadecimal characters and never starts with a
            // dot. The one thing that produces a dotted name here is a staged
            // record left behind by a write this process did not live to finish.
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let bytes = fs::read(entry.path()).map_err(|source| SiteError::Local {
                action: "read a channel while listing them",
                source,
            })?;
            names.push(Channel::from_bytes(&bytes)?.name);
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
        revoked::all(&self.root)
    }

    /// Adds a step to the revocation list.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the list cannot be written.
    pub fn revoke(&self, step: StepId) -> Result<(), SiteError> {
        revoked::add(&self.root, step)
    }

    fn make_root(&self) -> Result<(), SiteError> {
        permissions::create_dir(&self.root, "create the site directory")
    }

    /// Where the record for `name` sits, if this site could hold one at all.
    ///
    /// A site with no identity has no channels, so asking for one by name is
    /// answered the same way as asking for one that was never joined.
    fn channel_path(&self, name: &str) -> Result<PathBuf, SiteError> {
        let filed = self.filed(name)?.ok_or_else(|| SiteError::UnknownChannel {
            name: name.to_owned(),
        })?;
        Ok(self.root.join("channels").join(filed))
    }

    /// A handle renders as 64 hexadecimal characters, which needs no checking
    /// against [`naming::check`]: it cannot be empty, cannot escape a directory,
    /// and cannot collide with another author.
    fn cairn_path(&self, name: &str, author: &Handle) -> Result<PathBuf, SiteError> {
        Ok(self.cairn_dir(name)?.join(author.to_string()))
    }

    /// Where one channel's cairns sit, under the same filed name as its record.
    fn cairn_dir(&self, name: &str) -> Result<PathBuf, SiteError> {
        let filed = self.filed(name)?.ok_or_else(|| SiteError::UnknownChannel {
            name: name.to_owned(),
        })?;
        Ok(self.root.join("cairns").join(filed))
    }
}
