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

use crate::complaint::Complaint;

impl Complaint {
    /// The command that would move the caller forward from here.
    pub(crate) fn recover(&self) -> String {
        match self {
            // Two of the waypoint's failures have a way out of their own, and
            // both are about the host rather than the network: one sent this
            // endpoint somewhere it did not choose, the other said nothing at
            // all. Telling somebody to run `doctor` against a host that is not
            // answering wastes the one thing they have, which is a guess.
            Self::Waypoint(WaypointError::Redirected { .. }) => {
                "this host is not a box: it answered with somewhere else to go, and that was \
                 refused rather than followed. Check the waypoint url"
                    .to_owned()
            }
            Self::Waypoint(WaypointError::Unanswered { .. }) => {
                "retry; if it persists the host is down".to_owned()
            }
            Self::Waypoint(_) => {
                "run `kusanagi doctor <waypoint>` to see what the host actually does".to_owned()
            }
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
            Self::Locator(_) => {
                "a waypoint is a path, an http:// url, or s3://ENDPOINT/BUCKET[?region=R]"
                    .to_owned()
            }
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
            Self::NoInvitation => "ask for a fresh invitation: this one has expired, or the                  host no longer holds what it points at"
                .to_owned(),
            Self::InviteSpent => {
                "ask for a fresh invitation; each one admits exactly one endpoint".to_owned()
            }
            Self::ForeignRecord { .. } => "this site was made on another platform: run                  `kusanagi export` there, and pipe the archive into `kusanagi import` here"
                .to_owned(),
            Self::BadRecovery => "check the recovery key: it is the 64 hexadecimal digits                  `kusanagi export` printed once, and it goes in on the first line of stdin"
                .to_owned(),
            Self::OwnInvitation => "hand this line to the endpoint you mean to admit; \
                 the channel it opens is already here under the name you gave it"
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
