// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The room verbs as MCP tools, and what one call becomes.
//!
//! Apart from `tools.rs` because that dispatch is at its line limit: five more
//! arms would push it past what one function may hold, and five catalogue
//! entries would push the file past what one file may hold.

use std::collections::BTreeMap;

use serde_json::{Value, json};

use crate::request::Request;
use crate::tools::{Tool, need, object, text};
use kusanagi_door::Complaint;

/// The five room tools as catalogue entries, in a stable order.
///
/// Beside `room_called` because the two change for the same reason: a room
/// verb arrives as a catalogue entry and as a call arm together.
pub(crate) const ROOM_TOOLS: &[Tool] = &[
    Tool {
        name: "kusanagi_room",
        about: "Open a room: one secret, one ward every member sweeps, one signed roster.",
        schema: || {
            object(
                &json!({
                    "name": text("what to call the room here"),
                    "waypoint": text("where the drops live: a path, an http:// url, or s3://…"),
                }),
                &["name", "waypoint"],
            )
        },
    },
    Tool {
        name: "kusanagi_room_invite",
        about: "Mint the one line that invites somebody into a room. The line is a bearer credential.",
        schema: || {
            object(
                &json!({
                    "name": text("which room"),
                    "lifetime": { "type": "integer", "description": "seconds the invitation stays valid" },
                }),
                &["name"],
            )
        },
    },
    Tool {
        name: "kusanagi_room_join",
        about: "Accept a room invitation somebody handed over.",
        schema: || {
            object(
                &json!({
                    "name": text("what to call the room here"),
                    "invite": text("the invitation line"),
                }),
                &["name", "invite"],
            )
        },
    },
    Tool {
        name: "kusanagi_room_send",
        about: "Append one segment to your stream in a room.",
        schema: || {
            object(
                &json!({
                    "name": text("which room"),
                    "text": text("what to say"),
                }),
                &["name", "text"],
            )
        },
    },
    Tool {
        name: "kusanagi_room_read",
        about: "Read a room: sweep its ward once, verify every member's stream. One row per author.",
        schema: || {
            object(
                &json!({
                    "name": text("which room"),
                    "after": { "type": "object", "additionalProperties": { "type": "integer" }, "description": "per author handle, the height already held; an author not named is reported whole" },
                }),
                &["name"],
            )
        },
    },
];

/// Turns a room `tools/call` into the request it names.
///
/// Apart from `called` because that dispatch is at its line limit: five more
/// arms would push it past what one function may hold.
pub(crate) fn room_called(name: &str, arguments: &Value) -> Result<Request, Complaint> {
    Ok(match name {
        "kusanagi_room" => Request::Room {
            name: need(arguments, "name")?.to_owned(),
            waypoint: need(arguments, "waypoint")?.to_owned(),
        },
        "kusanagi_room_invite" => Request::RoomInvite {
            name: need(arguments, "name")?.to_owned(),
            lifetime: arguments
                .get("lifetime")
                .and_then(Value::as_u64)
                .unwrap_or(604_800),
        },
        "kusanagi_room_join" => Request::RoomJoin {
            invite: need(arguments, "invite")?.to_owned(),
            name: need(arguments, "name")?.to_owned(),
        },
        "kusanagi_room_send" => Request::RoomSend {
            name: need(arguments, "name")?.to_owned(),
            payload: need(arguments, "text")?.as_bytes().to_vec(),
        },
        "kusanagi_room_read" => Request::RoomRead {
            name: need(arguments, "name")?.to_owned(),
            after: match arguments.get("after") {
                None => BTreeMap::new(),
                Some(floors) => floors
                    .as_object()
                    .ok_or_else(|| floors_wrong("is not an object"))?
                    .iter()
                    .map(|(handle, height)| {
                        height
                            .as_u64()
                            .map(|height| (handle.clone(), height))
                            .ok_or_else(|| {
                                floors_wrong("maps a handle to something other than a height")
                            })
                    })
                    .collect::<Result<_, _>>()?,
            },
        },
        _ => {
            return Err(Complaint::Argument {
                what: "tool",
                reason: "is not a room tool this endpoint offers".to_owned(),
                instead: "call tools/list to see what there is",
            });
        }
    })
}

/// What a malformed `after` is refused with.
fn floors_wrong(reason: &str) -> Complaint {
    Complaint::Argument {
        what: "after",
        reason: reason.to_owned(),
        instead: "pass {\"<author handle>\": <height already held>} or leave it out",
    }
}
