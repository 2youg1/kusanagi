// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What to do next, which is the field the layer below could not fill in.
//!
//! A failure carries what went wrong, what it went wrong on, and a stable code.
//! Those come from the layer that failed. **The way out cannot**, because only
//! this layer knows what the caller was trying to do and what verb would move
//! them forward from here — `kusanagi-site` cannot say "run `kusanagi channels`"
//! without acquiring verbs it does not have.
//!
//! Apart from the enum because the two change for different reasons: a new
//! failure adds a variant there, and a reworded instruction changes only a
//! sentence here. Every sentence in this file is read by somebody who has just
//! been stopped, so it names a command wherever a command exists.

use kusanagi_kernel::WaypointError;
use kusanagi_waypoint::LocatorError;

use crate::complaint::Complaint;

/// What to do about a locator that names no place this program will open.
fn locator_trouble(error: &LocatorError) -> String {
    match error {
        LocatorError::NetworkPath => "mount the share yourself and name the drive or mount \
             point it appears as; this program never opens a network path on its own"
            .to_owned(),
        _ => "a waypoint is a path, an http:// url, or s3://ENDPOINT/BUCKET[?region=R]".to_owned(),
    }
}

/// What to do when the history a read needs is not where a read looks.
fn history_trouble(complaint: &Complaint) -> String {
    match complaint {
        Complaint::WardOverfull { .. } => "wait for the period to end and read again; if it \
             persists, this ward is crowded: make a fresh identity in a new root and invite \
             your peers there"
            .to_owned(),
        _ => "this channel releases what its peer has read, so the archive is the history: \
              run `kusanagi import` with the backup `kusanagi export` made"
            .to_owned(),
    }
}

impl Complaint {
    /// The command that would move the caller forward from here.
    pub(crate) fn recover(&self) -> String {
        match self {
            Self::Waypoint(failure) => host_trouble(failure),
            Self::Segment(_)
            | Self::Chain(_)
            | Self::Sealed(_)
            | Self::NotThePeer { .. }
            | Self::BadGreeting { .. } => {
                "the bytes at that address are not what this channel expects; \
                 keep them and open an issue — this is either damage or an attack"
                    .to_owned()
            }
            Self::Grant(_) => {
                "ask whoever invited you for a new invitation: this one no longer authorises it"
                    .to_owned()
            }
            Self::Locator(error) => locator_trouble(error),
            // `--bind 0` first, because it always works and needs no guess. The
            // named form comes second for the host whose address is already in
            // somebody's invitation, and which therefore has to come back on the
            // port it left on.
            Self::Listening { .. } => "pass --bind 0 to take any free port, which is printed \
                 when the host starts, or --bind ADDRESS to name one this machine has"
                .to_owned(),
            Self::Local { .. } => {
                "check that --root names a writable directory, then run the command again"
                    .to_owned()
            }
            Self::Permissions { .. } => "choose a --root on a local disk that keeps per-file \
                 permissions: NTFS on Windows, any ordinary filesystem elsewhere. FAT, exFAT \
                 and most network shares cannot keep a channel secret from other accounts"
                .to_owned(),
            Self::BadName { .. } => "pick a name of 1 to 32 characters from a-z, 0-9 and -, \
                 not starting with -, and run the command again"
                .to_owned(),
            // The advice names the pipe because there is no other way in. An
            // invitation carries the channel secret, so it is not an argument,
            // and telling somebody to "copy the invitation" without saying where
            // to put it sends them looking for a flag that does not exist.
            Self::BadInvitation { .. } => "pipe the whole invitation in, including the \
                 `kusanagi2:` prefix: pbpaste | kusanagi join --name NAME"
                .to_owned(),
            Self::BadRecord { .. } => "this file is not one this build can read; keep it and \
                 report it, because a record written here should not fail to parse"
                .to_owned(),
            Self::NoInvitation => "ask for a fresh invitation: this one has expired, \
                 or the host no longer holds what it points at"
                .to_owned(),
            Self::InviteSpent => {
                "ask for a fresh invitation; each one admits exactly one endpoint".to_owned()
            }
            // The second command names its argument as a slot rather than
            // standing alone. `kusanagi import` by itself is advice nobody can
            // take — it reads a key and an archive from a pipe, so typing it at a
            // terminal refuses — and advice that does not run is not advice.
            Self::ForeignRecord { .. } => "this site was made on another platform: run \
                 `kusanagi export` there, then pipe the archive into \
                 `kusanagi import --root <EMPTY_DIRECTORY>` here"
                .to_owned(),
            Self::Burned(_) | Self::NeedsCairn { .. } | Self::WardOverfull { .. } => {
                history_trouble(self)
            }
            Self::NotSlotted { name } => {
                format!(
                    "`tick` is for a channel with a period; send on this one with \
                     `kusanagi send --to {name}`"
                )
            }
            Self::BadRecovery => "check the recovery key: it is the 64 hexadecimal digits \
                 `kusanagi export` printed once, and it goes in on the first line of stdin"
                .to_owned(),
            Self::OwnInvitation => "hand this line to the endpoint you mean to admit; \
                 the channel it opens is already here under the name you gave it"
                .to_owned(),
            Self::ProxyRequired => "set KUSANAGI_PROXY=socks5://127.0.0.1:9050 (or another proxy) and run                  the command again; `kusanagi proxy --optional` lifts the requirement"
                .to_owned(),
            Self::CannotRevokeRoot { name } => {
                format!(
                    "leave instead: `kusanagi forget --channel {name}` drops the channel here, \
                     and nothing on the host is touched"
                )
            }
            Self::UnknownChannel { .. } | Self::NoPeerYet { .. } => {
                "run `kusanagi channels` to see what is here".to_owned()
            }
            // A group is made by writing its roster, so the way out of a missing
            // one is to write it rather than to look for it.
            Self::UnknownGroup { .. } => "run `kusanagi channels` to see the groups here, or \
                 make this one: printf 'alice\\nbob' | kusanagi group --name NAME"
                .to_owned(),
            Self::NoIdentity => {
                "run `kusanagi id` to create this endpoint's identity, then try again".to_owned()
            }
            Self::NoRoot { .. } => {
                "pass --root to say where this endpoint should keep its identity and channels"
                    .to_owned()
            }
            Self::HistoryChanged { .. } => "run `kusanagi doctor <waypoint>`: only a write-once \
                 host can promise this cannot happen, and this one just did it"
                .to_owned(),
            Self::ChannelExists { name } => {
                format!(
                    "pick another name, or read the one you have with `kusanagi read --from {name}`"
                )
            }
            Self::DropTaken { name, .. } => {
                format!(
                    "run `kusanagi read --from {name}` to pick up the new head, then send again"
                )
            }
            Self::Argument { instead, .. } => (*instead).to_owned(),
        }
    }
}

/// What to do about a host that would not do what it was asked.
///
/// Apart from the rest because these answers are about somebody else's machine
/// rather than about this endpoint's own state, and because telling somebody to
/// run `doctor` against a host that is not answering wastes the one thing they
/// have, which is a guess.
fn host_trouble(failure: &WaypointError) -> String {
    match failure {
        WaypointError::Redirected { .. } => "this host is not a box: it answered with somewhere              else to go, and that was refused rather than followed. Check the waypoint url"
            .to_owned(),
        WaypointError::Unanswered { .. } => "retry; if it persists the host is down".to_owned(),
        WaypointError::DeletionRefused => "this host will not delete, so a channel opened with              --release cannot keep its promise here. Open it without --release, or move it to a              host that deletes"
            .to_owned(),
        _ => "run `kusanagi doctor <waypoint>` to see what the host actually does".to_owned(),
    }
}
