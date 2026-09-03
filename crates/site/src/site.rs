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

use crate::cairns;
use crate::channel::Channel;
use crate::error::SiteError;
use crate::naming;
use crate::permissions;
use crate::records;
use crate::revoked;
use crate::roster::{self, Roster};

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
        let Some(bytes) =
            permissions::read(&self.root.join("identity"), "read this endpoint's identity")?
        else {
            return Ok(None);
        };
        <[u8; 32]>::try_from(bytes.as_slice())
            .map(Some)
            .map_err(|_| SiteError::BadRecord {
                what: "an identity file",
                reason: format!("an identity is 32 bytes; this one is {}", bytes.len()),
            })
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
        match permissions::read(&path, "read a channel")? {
            None => Err(SiteError::UnknownChannel {
                name: name.to_owned(),
            }),
            Some(bytes) => {
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
    /// Missing, unreadable and undecodable are one answer here; `cairns` owns
    /// that rule and says why.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one. That is a caller
    /// mistake rather than a state of the disk, so it is not a miss.
    pub fn cairn(&self, name: &str, author: &Handle) -> Result<Option<Cairn>, SiteError> {
        Ok(cairns::read(
            &self.root,
            &self.filed_or_unknown(name)?,
            author,
        ))
    }

    /// Writes down how far `cairn`'s author has been verified on this channel.
    ///
    /// # Errors
    ///
    /// [`SiteError::BadName`] when `name` is not usable as one, and
    /// [`SiteError::Local`] when the record cannot be written.
    pub fn mark(&self, name: &str, cairn: &Cairn) -> Result<(), SiteError> {
        cairns::write(&self.root, &self.filed_or_unknown(name)?, cairn)
    }

    /// One group's roster.
    ///
    /// # Errors
    ///
    /// [`SiteError::UnknownChannel`] when there is no group of that name, and
    /// [`SiteError::BadRecord`] when the record does not decode.
    pub fn roster(&self, name: &str) -> Result<Roster, SiteError> {
        roster::read(&self.root, &self.filed_or_unknown(name)?, name)?.ok_or_else(|| {
            SiteError::UnknownChannel {
                name: name.to_owned(),
            }
        })
    }

    /// Replaces one group's roster, creating the group if there was none.
    ///
    /// # Errors
    ///
    /// [`SiteError::NoIdentity`] when this endpoint has nothing to file it
    /// under, and [`SiteError::Local`] when the file cannot be written.
    pub fn enrol(&self, roster: &Roster) -> Result<(), SiteError> {
        let filed = self.filed(&roster.name)?.ok_or(SiteError::NoIdentity)?;
        roster::write(&self.root, &filed, roster)
    }

    /// Every group here, with its members, in a stable order.
    ///
    /// # Errors
    ///
    /// [`SiteError::Local`] when the directory cannot be listed, and
    /// [`SiteError::BadRecord`] when a record does not decode.
    pub fn groups(&self) -> Result<Vec<Roster>, SiteError> {
        roster::all(&self.root)
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
                if let Ok(filed) = self.filed_or_unknown(name) {
                    fs::remove_dir_all(cairns::dir(&self.root, &filed)).ok();
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
    /// [`SiteError::Local`] when a record cannot be read, and
    /// [`SiteError::BadRecord`] when one does not decode.
    pub fn names(&self) -> Result<Vec<String>, SiteError> {
        let mut names = records::each(&self.root, "channels", "list the channels")?
            .iter()
            .map(|bytes| Channel::from_bytes(bytes).map(|channel| channel.name))
            .collect::<Result<Vec<String>, SiteError>>()?;
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
        Ok(self
            .root
            .join("channels")
            .join(self.filed_or_unknown(name)?))
    }

    /// What `name` is filed as, when a site with no identity means no such thing.
    fn filed_or_unknown(&self, name: &str) -> Result<String, SiteError> {
        self.filed(name)?.ok_or_else(|| SiteError::UnknownChannel {
            name: name.to_owned(),
        })
    }
}
