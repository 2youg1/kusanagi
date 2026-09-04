// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The Model Context Protocol, spoken over stdin and stdout.
//!
//! An agent that calls this program through a shell pays for a process every
//! time and reads a terminal's worth of prose to find one number. MCP is what
//! the agent already speaks, so this is the same verb set through the door the
//! caller was already standing at.
//!
//! **This process is a transport, not a state holder, and that is what keeps
//! law 1 true.** Every call opens the site, does one thing and closes it,
//! exactly as the one-shot command does — nothing is cached between calls,
//! nothing is remembered, and killing this process at any moment changes no
//! result. It is `kusanagi host` in that respect and not a daemon: what runs for
//! a long time here is a pipe, and the endpoint's state is the disk.
//!
//! **The tool result is JSON, and that is the second reason `--json` exists.**
//! `ARCHITECTURE.md` D-08 kept JSON as the machine contract because a parser
//! draws exact boundaries where a reader of prose does not; an MCP result goes
//! to a parser. The fence rule still holds for the prose one: whatever a peer
//! wrote is data, and the tool description says so where the agent will read it.
//!
//! ```text
//! {"jsonrpc":"2.0","id":1,"method":"initialize","params":{…}}
//! {"jsonrpc":"2.0","id":2,"method":"tools/list"}
//! {"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":…,"arguments":{…}}}
//! ```
//!
//! One JSON object per line each way, which is the stdio transport MCP defines.
//! A line that is not JSON is answered and the loop carries on: a front end that
//! died on one malformed line would take an agent's whole session with it.

use std::io::{BufRead, Write};

use serde_json::{Value, json};

use crate::assembly::run;
use crate::tools::{called, catalogue};
use kusanagi_door::{CONTRACT, Complaint, Outcome};
use kusanagi_site::Site;

/// The revision of MCP this front end answers with.
///
/// Reported rather than negotiated: this is a small server with one capability,
/// so there is nothing to fall back to and pretending otherwise would be a
/// promise nobody keeps.
const PROTOCOL: &str = "2025-06-18";

/// JSON-RPC's own code for a method that does not exist.
const METHOD_NOT_FOUND: i32 = -32_601;

/// Serves one MCP session until the input ends.
///
/// # Errors
///
/// [`Complaint::Local`] when stdin or stdout fails, which is the connection
/// going away rather than anything about a request.
pub fn serve(
    site: &Site,
    input: &mut impl BufRead,
    output: &mut impl Write,
) -> Result<Outcome, Complaint> {
    let failed = |source| Complaint::Local {
        action: "speak to the agent on stdin and stdout",
        source,
    };
    let mut line = String::new();
    let mut served = 0_u64;
    loop {
        line.clear();
        if input.read_line(&mut line).map_err(failed)? == 0 {
            break;
        }
        if line.trim().is_empty() {
            continue;
        }
        let Some(answer) = answered(site, &line) else {
            // A notification carries no id and expects no answer. Sending one
            // anyway is what makes a client hang waiting for a reply to a reply.
            continue;
        };
        served = served.saturating_add(1);
        writeln!(output, "{answer}").map_err(failed)?;
        output.flush().map_err(failed)?;
    }
    Ok(Outcome::Served { calls: served })
}

/// Answers one line, or `None` when the line asked for no answer.
fn answered(site: &Site, line: &str) -> Option<String> {
    let request: Value = match serde_json::from_str(line) {
        Ok(parsed) => parsed,
        // A parse failure has no id to answer under, and JSON-RPC says to use
        // null. It is reported rather than swallowed so that a client sending
        // rubbish finds out from the first line instead of from a silence.
        Err(error) => {
            return Some(
                json!({"jsonrpc": "2.0", "id": Value::Null, "error": {
                    "code": -32_700, "message": error.to_string()
                }})
                .to_string(),
            );
        }
    };
    let id = request.get("id").cloned()?;
    let method = request.get("method").and_then(Value::as_str).unwrap_or("");
    let params = request.get("params").cloned().unwrap_or(Value::Null);
    Some(replied(site, &id, method, &params).to_string())
}

/// What one method answers.
fn replied(site: &Site, id: &Value, method: &str, params: &Value) -> Value {
    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "kusanagi", "version": env!("CARGO_PKG_VERSION") },
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": catalogue() }),
        "tools/call" => return json!({"jsonrpc": "2.0", "id": id, "result": call(site, params)}),
        other => {
            return json!({"jsonrpc": "2.0", "id": id, "error": {
                "code": METHOD_NOT_FOUND,
                "message": format!("this endpoint does not answer `{other}`"),
            }});
        }
    };
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Runs one tool call and shapes it the way MCP reads a result.
///
/// **A refused verb is a result with `isError`, not a JSON-RPC error.** The
/// distinction is the protocol's and it matters here: a transport error means
/// the call did not happen, and an unreachable host or a revoked grant is a
/// call that happened and answered. An agent that saw the second as the first
/// would retry something that will fail identically forever.
fn call(site: &Site, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));
    let answered = called(name, &arguments)
        .and_then(|request| run(site, &request))
        // A fence is drawn even though JSON ignores it, because `render` takes
        // one and this front end must not be the place that decides it does not
        // need it: the day a tool answers prose, the fence is already there.
        .and_then(|outcome| Ok((outcome, crate::world::fresh_fence()?)));
    match answered {
        Ok((outcome, fence)) => json!({
            "content": [{ "type": "text", "text": outcome.render(true, fence) }],
            "isError": false,
        }),
        Err(refusal) => json!({
            "content": [{ "type": "text", "text": refusal.render(true) }],
            "isError": true,
            "structuredContent": { "contract": CONTRACT, "code": refusal.code() },
        }),
    }
}
