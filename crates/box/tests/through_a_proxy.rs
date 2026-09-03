// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! That a configured proxy is actually used, proved by taking it away.
//!
//! kusanagi does not hide an endpoint's IP address (`ARCHITECTURE.md` §3). What
//! it offers is the socket: point `KUSANAGI_PROXY` at a SOCKS5 listener — which
//! is how Tor and every desktop VPN expose themselves — and every request goes
//! through it.
//!
//! **A privacy setting that fails open is worse than one nobody offered**, so
//! the assertion here is not that a proxy can be configured. It is that the same
//! request against the same live host succeeds without one and fails with one
//! that leads nowhere. If the setting were ignored, both would succeed, and this
//! is the only test in the workspace that can tell those two worlds apart.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]

mod common;

use std::net::TcpListener;

use common::host;
use kusanagi_kernel::{DropAddr, Waypoint as _};
use kusanagi_waypoint::{Access, HttpWaypoint, Proxy};

/// A port nothing is listening on, obtained by listening and then stopping.
///
/// Asking the operating system for one is the only way to name a closed port
/// without picking a number and hoping. The race — somebody else binding it in
/// between — would turn this test flaky rather than wrong, and has never been
/// observed because the window is microseconds on a loopback interface.
fn closed_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
    let port = listener.local_addr().expect("no local address").port();
    drop(listener);
    port
}

#[test]
fn a_configured_proxy_is_the_only_way_out() {
    let (address, root) = host("proxy", 1);
    let base = format!("http://{address}");
    let addr = DropAddr::from_bytes([0x5c; 20]);

    // Straight at the host: the drop is not there, which is an answer.
    let direct = HttpWaypoint::new(&base, &Access::default());
    assert!(
        direct
            .get(&addr)
            .expect("the host did not answer")
            .is_none(),
        "an empty address should report empty, not fail"
    );

    // The same request through a proxy that leads nowhere. It has to fail: the
    // request must not quietly go direct when the socket it was told to use is
    // not there.
    let nowhere = Proxy::parse(&format!("socks5://127.0.0.1:{}", closed_port()))
        .expect("that is a proxy locator");
    let proxied = HttpWaypoint::new(
        &base,
        &Access {
            proxy: Some(nowhere),
            ..Access::default()
        },
    );
    assert!(
        proxied.get(&addr).is_err(),
        "the request reached the host without the proxy it was given"
    );

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn something_that_is_not_a_proxy_is_refused_when_it_is_read() {
    // Refused at the point it is configured, with this workspace's own error
    // rather than the client library's. A caller who mistyped one has to be told
    // that, not handed a message from a crate they did not know they were using.
    let refused = Proxy::parse("nonsense://:::").expect_err("that is not a proxy");
    assert_eq!(refused.code(), "locator.bad_proxy");
}
