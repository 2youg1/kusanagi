// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The same verbs, through the door an agent is already standing at.
//!
//! What is worth asserting about a second front end is not that it works — that
//! is the first front end's tests — but that it is a **reading of the same
//! authority rather than a second one**. So the load-bearing test here is the
//! one that walks `Request` and the tool catalogue together: a verb added to the
//! enum and forgotten here turns it red.
//!
//! Everything else is the protocol's own contract: an id comes back on every
//! answer, a notification gets none, and a refused verb is a *result* that says
//! it failed rather than a transport error — an agent that confused the two
//! would retry something that will fail identically forever.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, scratch};
use kusanagi::{Request, Site};
use serde_json::Value;

/// Feeds `lines` to one session and returns what came back, one value per line.
fn session(site: &Site, lines: &[&str]) -> Vec<Value> {
    let mut input = lines.join("\n").into_bytes();
    input.push(b'\n');
    let mut output = Vec::new();
    kusanagi::serve(site, &mut input.as_slice(), &mut output).expect("the session ran");
    String::from_utf8(output)
        .expect("a front end wrote something that is not text")
        .lines()
        .map(|line| {
            serde_json::from_str(line).expect("a front end wrote something that is not JSON")
        })
        .collect()
}

#[test]
fn a_session_introduces_itself_and_lists_its_tools() {
    let endpoint = Endpoint::new(scratch("port-hello"));
    let site = Site::at(endpoint.site_root());
    let answers = session(
        &site,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
        ],
    );

    assert_eq!(
        answers.len(),
        2,
        "a notification carries no id and must be answered with nothing"
    );
    assert_eq!(answers[0]["id"], 1);
    assert_eq!(answers[0]["result"]["serverInfo"]["name"], "kusanagi");
    assert!(answers[0]["result"]["capabilities"]["tools"].is_object());

    let tools = answers[1]["result"]["tools"].as_array().unwrap();
    assert!(tools.iter().all(|tool| {
        tool["name"].is_string()
            && tool["description"].is_string()
            && tool["inputSchema"].is_object()
    }));
}

/// The one test that keeps the two front ends from drifting apart.
///
/// Every verb this program can be asked to do is either offered as a tool or is
/// one of the three that cannot be: `host` and `port` run until they are killed,
/// and `export` puts an archive on stdout. Adding a verb without deciding which
/// of those it is turns this red.
#[test]
fn every_verb_is_either_a_tool_or_deliberately_not_one() {
    let endpoint = Endpoint::new(scratch("port-catalogue"));
    let site = Site::at(endpoint.site_root());
    let answers = session(
        &site,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#],
    );
    let offered: Vec<String> = answers[0]["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|tool| tool["name"].as_str().unwrap().to_owned())
        .collect();

    // Named rather than derived, because deriving them from the enum would make
    // this test agree with whatever the code happens to do.
    let expected = [
        "kusanagi_id",
        "kusanagi_channels",
        "kusanagi_invite",
        "kusanagi_join",
        "kusanagi_send",
        "kusanagi_send_to_group",
        "kusanagi_read",
        "kusanagi_tick",
        "kusanagi_group",
        "kusanagi_revoke",
        "kusanagi_forget",
        "kusanagi_doctor",
    ];
    assert_eq!(offered, expected);

    // And the three that are not tools are not tools by decision: a streaming
    // archive and two processes that do not return.
    for absent in ["kusanagi_host", "kusanagi_port", "kusanagi_export"] {
        assert!(!offered.iter().any(|name| name == absent));
    }
}

#[test]
fn a_tool_call_does_the_same_thing_the_verb_does() {
    let endpoint = Endpoint::new(scratch("port-call"));
    let site = Site::at(endpoint.site_root());
    let answers = session(
        &site,
        &[
            r#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"kusanagi_id","arguments":{}}}"#,
        ],
    );

    assert_eq!(answers[0]["id"], 7);
    assert_eq!(answers[0]["result"]["isError"], false);
    // The text a model reads is prose, and the JSON is beside it where a
    // program reads: the same outcome, in the shape each reader has.
    let text = answers[0]["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.starts_with("this endpoint is "), "{text}");
    let reported: &Value = &answers[0]["result"]["structuredContent"];
    assert_eq!(reported["contract"], 1);

    // The same question through the other door gives the same handle. One
    // authority, two readings.
    assert_eq!(reported["handle"].as_str().unwrap(), endpoint.handle());
}

#[test]
fn a_refused_verb_is_a_failed_result_and_not_a_broken_transport() {
    let endpoint = Endpoint::new(scratch("port-refused"));
    let site = Site::at(endpoint.site_root());
    let answers = session(
        &site,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"kusanagi_read","arguments":{"name":"nobody"}}}"#,
            r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"kusanagi_fly","arguments":{}}}"#,
            r#"{"jsonrpc":"2.0","id":3,"method":"nonsense"}"#,
            "not json at all",
        ],
    );

    // A channel that is not here: the call happened and answered.
    assert!(answers[0]["error"].is_null());
    assert_eq!(answers[0]["result"]["isError"], true);
    assert_eq!(
        answers[0]["result"]["structuredContent"]["code"],
        "kusanagi.unknown_channel"
    );

    // A tool that is not offered is the same kind of answer, with a code.
    assert_eq!(answers[1]["result"]["isError"], true);
    assert_eq!(
        answers[1]["result"]["structuredContent"]["code"],
        "kusanagi.argument"
    );

    // A method that does not exist is a transport error, because it is one.
    assert_eq!(answers[2]["error"]["code"], -32_601);

    // And a line that is not JSON does not end the session.
    assert_eq!(answers[3]["error"]["code"], -32_700);
    assert_eq!(answers.len(), 4);
}

#[test]
fn a_channel_opened_through_a_tool_carries_the_habit_it_was_given() {
    let endpoint = Endpoint::new(scratch("port-habit"));
    let site = Site::at(endpoint.site_root());
    let host = scratch("port-habit-host");
    let call = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"kusanagi_invite","arguments":{{"name":"bob","waypoint":"{}","every":900,"release":true}}}}}}"#,
        host.display().to_string().replace('\\', "\\\\")
    );
    let answers = session(&site, &[&call]);
    assert_eq!(answers[0]["result"]["isError"], false, "{:?}", answers[0]);

    let listed = common::json(&endpoint.run(&Request::Channels).unwrap());
    assert_eq!(listed["channels"][0]["period"], 900);
    assert_eq!(listed["channels"][0]["retention"], "release");
}
