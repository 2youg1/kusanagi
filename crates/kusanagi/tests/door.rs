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
    let written = child
        .stdin
        .take()
        .expect("stdin was not piped")
        .write_all(fed);
    match written {
        Ok(()) => {}
        // A reader that has read its limit and exited closes the pipe under us.
        // That is the bound working, not the harness failing: `join` reads at
        // most 16 KiB and refuses rather than buffering whatever arrives.
        Err(error) if error.kind() == std::io::ErrorKind::BrokenPipe => {}
        Err(error) => panic!("could not write to the child: {error}"),
    }
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
    // The invitation goes in on stdin, never as an argument: it is a bearer
    // token, and arguments are readable by every account on the machine.
    reported(&door(&bob, &["join", "--name", "alice"], line.as_bytes()));

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

/// The ways an invitation actually arrives on stdin, and the ways it does not.
///
/// The invitation stopped being an argument because an argument is public: on
/// Linux any account can read another process's command line out of `/proc`, and
/// the shell keeps it afterwards. Moving it to stdin closes that, and creates a
/// new set of edges — a paste with a trailing newline, a paste with none, a paste
/// from a Windows clipboard carrying `\r\n`, an empty pipe, and a pipe carrying
/// something that is not an invitation at all.
#[test]
fn an_invitation_arrives_on_stdin_however_it_was_pasted() {
    let ground = scratch("door-stdin");
    let host = ground.join("host").display().to_string();
    let alice = ground.join("alice");

    // One invitation admits exactly one endpoint, so each clipboard gets its
    // own. That rule is asserted elsewhere; here it is a constraint on the test.
    let clipboards: [fn(&str) -> String; 4] = [
        |line| line.to_owned(),
        |line| format!("{line}\n"),
        // What a Windows clipboard hands over.
        |line| format!("{line}\r\n"),
        // What a chat window hands over.
        |line| format!("  {line}  \n\n"),
    ];

    for (round, paste) in clipboards.iter().enumerate() {
        let channel = format!("bob-{round}");
        let invited = reported(&door(
            &alice,
            &["invite", "--name", &channel, "--waypoint", &host],
            b"",
        ));
        let line = invited["invite"].as_str().unwrap();

        let bob = ground.join(format!("bob-{round}"));
        let joined = reported(&door(
            &bob,
            &["join", "--name", "alice"],
            paste(line).as_bytes(),
        ));
        assert_eq!(joined["command"], "joined", "paste {round} did not join");
    }

    std::fs::remove_dir_all(&ground).ok();
}

/// A whole channel, used from end to end, with no name and no text on argv.
///
/// `ARCHITECTURE.md` §8 took the invitation off the command line because a
/// command line is public. A channel name is worse: an invitation leaks one
/// chance to enter one channel, while `send --to bob` leaks who is talking to
/// whom on every single message — which is the relationship graph the derived
/// addresses of §3 exist to hide. So the same fix has to cover its own kind.
#[test]
fn no_verb_needs_a_channel_name_or_a_message_on_the_command_line() {
    let ground = scratch("door-off-argv");
    let host = ground.join("host").display().to_string();
    // The two roots are not named after the channels, so that the assertion
    // below can look at the whole command line and not just part of it.
    let one = ground.join("one");
    let two = ground.join("two");
    let secret = "a message no shell will ever see";

    // Every command in this test goes through here, and every one of them is
    // checked: nothing on the command line names a channel or carries the text.
    let quiet = |root: &Path, arguments: &[&str], fed: &[u8]| {
        for argument in arguments {
            assert!(
                !argument.contains("peer") && !argument.contains(secret),
                "`{argument}` puts what has to stay off the command line on it"
            );
        }
        door(root, arguments, fed)
    };

    let invited = reported(&quiet(
        &one,
        &["invite", "--name", "-", "--waypoint", &host],
        b"peer-two\n",
    ));
    let line = invited["invite"].as_str().unwrap().to_owned();

    let joined = reported(&quiet(
        &two,
        &["join", "--name", "-"],
        format!("peer-one\n{line}").as_bytes(),
    ));
    assert_eq!(joined["command"], "joined");

    reported(&quiet(
        &two,
        &["send", "--to", "-"],
        format!("peer-one\n{secret}").as_bytes(),
    ));

    let heard = reported(&quiet(&one, &["read", "--from", "-"], b"peer-two\n"));
    assert_eq!(heard["segments"][0]["text"], secret);

    // Hiding the name while the message stays on the command line is half a
    // fix, and half a fix that reads as a whole one is worse than none.
    let refused = complained(&door(&two, &["send", "--to", "-", "hello"], b"peer-one\n"));
    assert_eq!(refused["code"], "kusanagi.argument");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_pipe_with_nothing_in_it_is_a_complaint_and_not_a_hang() {
    let ground = scratch("door-empty-stdin");
    let bob = ground.join("bob");

    // Somebody typed the command and forgot the pipe. The answer has to be the
    // ordinary shape — a stable code and a way out — rather than a wait.
    let said = complained(&door(&bob, &["join", "--name", "alice"], b""));
    assert_eq!(said["code"], "kusanagi.malformed");
    // And the way out names the pipe, because there is no other way in. Advice
    // that says "copy the invitation" without saying where to put it sends a
    // person looking for a flag this program does not have.
    let recover = said["recover"].as_str().unwrap();
    assert!(
        recover.contains("pipe") && recover.contains("join"),
        "the way out of an empty pipe does not mention the pipe: {recover}"
    );

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn a_pipe_carrying_something_else_is_refused_rather_than_buffered() {
    let ground = scratch("door-junk-stdin");
    let bob = ground.join("bob");

    // Far more than an invitation can be, so the bound in `invitation` decides
    // this rather than the parser. What matters is that it ends, with an answer.
    let flood = vec![b'x'; 1_000_000];
    let said = complained(&door(&bob, &["join", "--name", "alice"], &flood));
    assert!(
        !said["code"].as_str().unwrap().is_empty(),
        "a flood on stdin was not answered with a code: {said}"
    );

    std::fs::remove_dir_all(&ground).ok();
}
