// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The claim with the network in it: two endpoints that share nothing but a TCP
//! connection to a host neither of them runs.
//!
//! Everything else in the test suite reaches the host through a filesystem, which
//! proves the protocol but not the transport. Here the host is `kusanagi host`
//! itself, listening on a real port, and the only thing crossing between the
//! endpoints is bytes on a socket. No outside network is needed, so this runs in
//! CI unchanged.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

mod common;

use common::{Endpoint, invite_line, json, scratch};
use kusanagi::{HOST_ADDRESS, Request, SystemClock, Whose};
use kusanagi_box::Server;
use std::net::TcpListener;

/// Starts a host on a port the operating system picks, and returns its URL.
fn host_on(directory: std::path::PathBuf) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
    let port = listener.local_addr().expect("no local address").port();
    std::thread::spawn(
        move || match Server::new(&directory, SystemClock).serve(&listener) {
            Ok(()) => {}
            Err(error) => eprintln!("test host stopped: {error}"),
        },
    );
    format!("http://127.0.0.1:{port}")
}

#[test]
fn two_endpoints_meet_over_tcp_through_a_host_neither_of_them_trusts() {
    let ground = scratch("tcp");
    let waypoint = host_on(ground.join("host"));
    let alice = Endpoint::new(ground.join("alice"));
    let bob = Endpoint::new(ground.join("bob"));

    // The invitation carries the host's address, so joining needs nothing else.
    let invitation = invite_line(&alice, "bob", &waypoint);
    assert!(invitation.contains("kusanagi2:"));
    bob.run(&Request::Join {
        invite: invitation,
        name: "alice".to_owned(),
        habit: kusanagi::Habit::default(),
    })
    .expect("bob could not join over tcp");

    alice.send("bob", "sent across a socket");
    alice.send("bob", "and a second time");
    bob.send("alice", "received across a socket");

    let heard = json(
        &bob.run(&Request::Read {
            name: "alice".to_owned(),
            after: None,
            whose: Whose::Peer,
        })
        .expect("bob could not read alice over tcp"),
    );
    assert_eq!(heard["height"], 1);
    assert_eq!(heard["segments"][0]["text"], "sent across a socket");

    let answered = json(
        &alice
            .run(&Request::Read {
                name: "bob".to_owned(),
                after: None,
                whose: Whose::Peer,
            })
            .expect("alice could not read bob over tcp"),
    );
    assert_eq!(answered["segments"][0]["text"], "received across a socket");

    std::fs::remove_dir_all(&ground).ok();
}

#[test]
fn doctor_measures_a_running_host_and_certifies_it() {
    let ground = scratch("doctor-tcp");
    let waypoint = host_on(ground.join("host"));
    let endpoint = Endpoint::new(ground.join("who"));

    let report = json(
        &endpoint
            .run(&Request::Doctor {
                waypoint: waypoint.clone(),
            })
            .expect("doctor could not reach the host"),
    );

    assert_eq!(report["waypoint"], waypoint);
    assert_eq!(report["kind"], "http box");
    assert_eq!(report["tier"], "write-once");

    // A box holds all four; a report that shows anything broken is a host this
    // endpoint should not be using.
    let capabilities = report["capabilities"].as_array().unwrap();
    assert_eq!(capabilities.len(), 4);
    for measured in capabilities {
        assert_eq!(
            measured["verdict"], "held",
            "the box failed `{}`: {}",
            measured["capability"], measured["detail"]
        );
    }

    std::fs::remove_dir_all(&ground).ok();
}

/// The address a host is told to take is one another program can already hold,
/// and what comes back has to say so.
///
/// Before this, a taken port produced `kusanagi.local`, whose recovery reads
/// "check that --root names a writable directory" — advice about a flag that has
/// nothing to do with the failure. An agent that follows it changes the wrong
/// thing and tries again.
#[test]
fn an_address_somebody_else_holds_is_refused_by_name() {
    let ground = scratch("bind-taken");
    let held = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
    let port = held.local_addr().expect("no local address").port();
    let endpoint = Endpoint::new(ground.join("who"));

    let refused = endpoint
        .run(&Request::Host {
            bind: port.to_string(),
            directory: ground.join("host"),
            capacity: 1_024,
        })
        .expect_err("a host took an address another listener already holds");

    assert_eq!(refused.code(), "kusanagi.address_unavailable");
    let recovery = refused.render(true);
    assert!(
        recovery.contains("--bind"),
        "the way out of a taken address is another address: {recovery}"
    );
    assert!(
        !recovery.contains("--root"),
        "a taken port is not a question about --root: {recovery}"
    );

    std::fs::remove_dir_all(&ground).ok();
}

/// The default is a port IANA lists as unassigned, not the registered one.
///
/// 8443 is `pcsync-https` in the registry and is taken in practice by Tomcat,
/// ingress controllers and development proxies, so the first command in the
/// documentation collided with whatever else the machine was running.
#[test]
fn the_default_host_address_is_loopback_on_an_unassigned_port() {
    assert_eq!(HOST_ADDRESS, "127.0.0.1:8963");
}

#[test]
fn doctor_reports_a_plain_directory_honestly_rather_than_failing_it() {
    let ground = scratch("doctor-dir");
    let endpoint = Endpoint::new(ground.join("who"));

    let report = json(
        &endpoint
            .run(&Request::Doctor {
                waypoint: ground.join("host").display().to_string(),
            })
            .expect("doctor could not examine a directory"),
    );

    assert_eq!(report["kind"], "directory");
    assert_eq!(report["tier"], "write-once");

    let by_name = |name: &str| -> serde_json::Value {
        report["capabilities"]
            .as_array()
            .unwrap()
            .iter()
            .find(|measured| measured["capability"] == name)
            .cloned()
            .unwrap()
    };
    assert_eq!(by_name("write-once")["verdict"], "held");
    // A directory has no ETags and no lifetimes. That is a named absence, not a
    // failure — the distinction is the whole reason `doctor` exists.
    assert_eq!(by_name("conditional-read")["verdict"], "not offered");
    assert_eq!(by_name("expiry")["verdict"], "not offered");
    assert!(by_name("expiry")["detail"].is_string());

    std::fs::remove_dir_all(&ground).ok();
}
