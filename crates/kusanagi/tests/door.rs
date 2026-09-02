// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The door as a caller actually finds it: a process, two streams, an exit code.
//!
//! Every other test drives the library. These drive the binary, because the few
//! dozen lines that turn arguments into a request are exactly the lines a
//! library test cannot reach — and they are the first thing an agent meets.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use common::scratch;
use kusanagi_kernel::unhex;

/// Runs the binary the way a caller would, with bytes on stdin.
fn door(root: &Path, arguments: &[&str], fed: &[u8]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_kusanagi"))
        .arg("--root")
        .arg(root)
        .arg("--json")
        .args(arguments)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary would not start");
    child
        .stdin
        .take()
        .expect("stdin was not piped")
        .write_all(fed)
        .expect("could not write to the child");
    child.wait_with_output().expect("the child did not finish")
}

/// Runs the binary with exactly these arguments and nothing added.
///
/// `door` speaks for a caller who got the command line right. This one is for a
/// caller who did not, which is the case that has to leave by the same door.
fn raw(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_kusanagi"))
        .args(arguments)
        .stdin(Stdio::null())
        .output()
        .expect("the binary would not start")
}

fn reported(output: &Output) -> serde_json::Value {
    assert!(
        output.status.success(),
        "the command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("stdout was not JSON")
}

fn complained(output: &Output) -> serde_json::Value {
    assert!(!output.status.success(), "the command was expected to fail");
    serde_json::from_slice(&output.stderr).expect("stderr was not JSON")
}

#[test]
fn bytes_piped_in_come_back_out_exactly() {
    let ground = scratch("door");
    let host = ground.join("host").display().to_string();
    let alice = ground.join("alice");
    let bob = ground.join("bob");

    let invited = reported(&door(
        &alice,
        &["invite", "--name", "bob", "--waypoint", &host],
        b"",
    ));
    let line = invited["invite"].as_str().unwrap().to_owned();
    reported(&door(&bob, &["join", &line, "--name", "alice"], b""));

    // A payload no shell would carry: a NUL, a byte that is not UTF-8, and a
    // newline in the middle of it.
    let sent: &[u8] = b"\x00\xffline one\nline two";
    reported(&door(&bob, &["send", "--to", "alice"], sent));

    let heard = reported(&door(&alice, &["read", "--from", "bob"], b""));
    let carried = heard["segments"][0]["payload"].as_str().unwrap();
    assert_eq!(unhex(carried).expect("the payload was not hex"), sent);

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn an_argument_this_program_cannot_act_on_is_refused_with_a_code() {
    let ground = scratch("door-argument");
    let host = ground.join("host").display().to_string();
    let alice = ground.join("alice");

    let refused = complained(&door(
        &alice,
        &[
            "invite",
            "--name",
            "bob",
            "--waypoint",
            &host,
            "--can",
            "send,fly",
        ],
        b"",
    ));
    assert_eq!(refused["code"], "kusanagi.argument");
    assert!(
        refused["recover"]
            .as_str()
            .is_some_and(|recover| recover.contains("send")),
        "the recovery does not say what to pass instead"
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// A command line this program cannot act on leaves by the door every other
/// failure leaves by.
///
/// Found by `adversary/`: one missed key on `--root` used to reach clap's own
/// error path, which exits with a code this door does not define and prints
/// prose even when the caller asked for JSON. An agent cannot act on that.
#[test]
fn a_mistyped_flag_is_a_complaint_like_any_other() {
    let ground = scratch("mistyped");
    let refused = raw(&["--json", "-root", &ground.display().to_string(), "id"]);
    assert_eq!(refused.status.code(), Some(1), "exit code");

    let complaint: serde_json::Value = serde_json::from_slice(&refused.stderr)
        .expect("a refusal that a program cannot parse is not an answer");
    assert_eq!(complaint["code"], "kusanagi.argument");
    assert!(!complaint["recover"].as_str().unwrap_or_default().is_empty());
    assert!(refused.stdout.is_empty(), "stdout carried something");

    // Help is not a failure: it is what a person asks for, and it succeeds.
    let asked = raw(&["--help"]);
    assert_eq!(asked.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&asked.stdout).contains("forget"));

    std::fs::remove_dir_all(&ground).ok();
}
