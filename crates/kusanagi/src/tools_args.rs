// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How a tool call spells the arguments every verb shares.
//!
//! Apart from `tools.rs` because that file is at its line limit: what
//! `alias` means, what `every` and `release` mean, and what `send,read`
//! means are three readings that change when the verbs change, not when the
//! catalogue does.

use serde_json::Value;

use crate::request::{Habit, Naming};
use kusanagi_door::Complaint;
use kusanagi_grant::{Abilities, Ability};

/// What a `kusanagi_name` call means: `alias` sets, `clear` clears, neither asks.
pub(crate) fn naming(arguments: &Value) -> Naming {
    match arguments.get("alias").and_then(Value::as_str) {
        Some(alias) => Naming::Set(alias.to_owned()),
        None if arguments.get("clear").and_then(Value::as_bool) == Some(true) => Naming::Clear,
        None => Naming::Ask,
    }
}

/// The two habits a channel is opened with, as a call spells them.
pub(crate) fn habit(arguments: &Value) -> Result<Habit, Complaint> {
    Ok(Habit {
        cadence: match arguments.get("every").and_then(Value::as_u64) {
            None => kusanagi_site::Cadence::OnDemand,
            Some(seconds) => kusanagi_site::Cadence::Slotted {
                period: u32::try_from(seconds)
                    .ok()
                    .and_then(core::num::NonZeroU32::new)
                    .ok_or(Complaint::Argument {
                        what: "every",
                        reason: "is a period in seconds, from 1 upwards".to_owned(),
                        instead: "pass a whole number of seconds, or leave it out",
                    })?,
            },
        },
        retention: if arguments.get("release").and_then(Value::as_bool) == Some(true) {
            kusanagi_site::Retention::ReleaseOnAck
        } else {
            kusanagi_site::Retention::Keep
        },
    })
}

/// Reads `send,read` into a set of abilities, defaulting to both.
pub(crate) fn abilities(text: Option<&str>) -> Result<Abilities, Complaint> {
    let mut abilities = Abilities::NONE;
    for word in text
        .unwrap_or("send,read")
        .split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
    {
        match word {
            "send" => abilities = abilities.with(Ability::Send),
            "read" => abilities = abilities.with(Ability::Read),
            other => {
                return Err(Complaint::Argument {
                    what: "can",
                    reason: format!("does not know the ability `{other}`"),
                    instead: "pass a comma-separated list of send and read",
                });
            }
        }
    }
    Ok(abilities)
}
