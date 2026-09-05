// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What a host can do to the endpoint that trusts it with nothing.
//!
//! Every other test of this client asks whether it can talk to a box. This one
//! asks what a box that is not one can do back: send the caller somewhere else,
//! never answer, or answer with more bytes than the caller has memory. All three
//! are defaults of a general-purpose HTTP client, and all three cost something
//! `ARCHITECTURE.md` §3 or §7 promises.
//!
//! The hosts here are raw TCP listeners rather than `kusanagi host`, because the
//! thing under test is the client and a real box would refuse to misbehave.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unused_io_amount,
    clippy::disallowed_methods,
    clippy::let_underscore_must_use,
    reason = "test code"
)]

use std::io::{Read as _, Write as _};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use kusanagi_kernel::{DropAddr, Waypoint as _};
use kusanagi_waypoint::{Access, Circuit, HttpWaypoint, Proxy};

/// The address every read here asks for. Nothing is ever written to it.
fn address() -> DropAddr {
    "0123456789abcdef0123456789abcdef01234567".parse().unwrap()
}

/// A listener bound to a free port, with the port returned before it is served.
fn bound() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    (listener, port)
}

/// Serves `answer` to every caller, and counts the callers.
///
/// The count is the assertion in the redirect case: what matters is not only
/// that the read failed, but that the machine the host named was never reached.
fn answering(listener: TcpListener, answer: Vec<u8>) -> Arc<AtomicUsize> {
    let reached = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&reached);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut head = [0_u8; 1024];
            stream.read(&mut head).ok();
            stream.write_all(&answer).ok();
            stream.flush().ok();
        }
    });
    reached
}

/// A client whose patience is a second, because a test should not take a minute.
fn impatient(port: u16) -> HttpWaypoint {
    HttpWaypoint::new(
        &format!("http://127.0.0.1:{port}"),
        &Access {
            patience: Duration::from_secs(1),
            ..Access::default()
        },
    )
}

#[test]
fn a_host_that_answers_with_somewhere_else_is_not_followed_there() {
    let (elsewhere, elsewhere_port) = bound();
    let reached = answering(
        elsewhere,
        b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_vec(),
    );

    let (redirector, redirector_port) = bound();
    answering(
        redirector,
        format!(
            "HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:{elsewhere_port}/d/x\r\n\
             Content-Length: 0\r\n\r\n"
        )
        .into_bytes(),
    );

    let error = impatient(redirector_port)
        .get(&address())
        .expect_err("a redirect is not an answer this client accepts");

    assert_eq!(error.code(), "waypoint.redirected");
    // The second assertion is the evidence. A client that failed *after*
    // following the redirect would still have handed the third party this
    // endpoint's address and the drop it wanted.
    assert_eq!(
        reached.load(Ordering::SeqCst),
        0,
        "the machine the host named was contacted"
    );
}

#[test]
fn a_host_that_never_answers_does_not_hold_a_one_shot_command_open() {
    let (silent, port) = bound();
    // Accept, and say nothing at all. An idle timeout would fire here too; what
    // this pins is that *some* deadline exists, which was not true before.
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in silent.incoming() {
            let Ok(stream) = stream else { break };
            held.push(stream);
        }
    });

    let began = Instant::now();
    let error = impatient(port)
        .get(&address())
        .expect_err("a host that says nothing is not an answer");
    let waited = began.elapsed();

    assert_eq!(error.code(), "waypoint.timeout");
    assert!(
        waited < Duration::from_secs(10),
        "gave up after {waited:?}, which is not a deadline"
    );
}

#[test]
fn a_host_cannot_choose_how_much_memory_a_read_allocates() {
    // Two megabytes, which is over the cap and well over anything this protocol
    // can legitimately carry.
    let oversized = 2 * 1_048_576;
    let mut answer = format!("HTTP/1.1 200 OK\r\nContent-Length: {oversized}\r\n\r\n").into_bytes();
    answer.extend(std::iter::repeat_n(b'x', oversized));

    let (generous, port) = bound();
    answering(generous, answer);

    let error = impatient(port)
        .get(&address())
        .expect_err("a body over the cap is not a body this client returns");
    assert_eq!(error.code(), "waypoint.io");
}

/// Reads one SOCKS5 greeting and one request, and reports the address type.
///
/// 3 is a domain name, which is the whole question: it means the client handed
/// the name over instead of resolving it here.
fn socks5_address_type(listener: TcpListener) -> std::sync::mpsc::Receiver<u8> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut greeting = [0_u8; 2];
        if stream.read_exact(&mut greeting).is_err() {
            return;
        }
        let mut methods = vec![0_u8; usize::from(greeting[1])];
        if stream.read_exact(&mut methods).is_err() {
            return;
        }
        // Version 5, no authentication.
        stream.write_all(&[0x05, 0x00]).ok();
        let mut request = [0_u8; 4];
        if stream.read_exact(&mut request).is_err() {
            return;
        }
        sender.send(request[3]).ok();
    });
    receiver
}

#[test]
fn a_socks_proxy_is_given_the_name_rather_than_the_answer_to_it() {
    let (proxy_listener, proxy_port) = bound();
    let seen = socks5_address_type(proxy_listener);

    // A name that cannot resolve anywhere. If this client resolved locally it
    // would fail before the proxy was ever contacted — and on any name that
    // *does* resolve, it would have leaked a plaintext DNS query for it.
    let waypoint = HttpWaypoint::new(
        "http://nonexistent.invalid:8963",
        &Access {
            proxy: Some(
                Proxy::parse(
                    &format!("socks5://127.0.0.1:{proxy_port}"),
                    Circuit::from_bytes([0; 16]),
                )
                .unwrap(),
            ),
            patience: Duration::from_secs(2),
            ..Access::default()
        },
    );
    let _ = waypoint.get(&address());

    let atyp = seen
        .recv_timeout(Duration::from_secs(5))
        .expect("the proxy was never asked to connect anywhere");
    assert_eq!(
        atyp, 3,
        "the client resolved the host itself and sent the proxy an address"
    );
}

/// Answers one SOCKS5 greeting by demanding a username and password, reports
/// the pair it was given, and then closes without connecting anywhere.
fn socks5_credentials(listener: TcpListener) -> std::sync::mpsc::Receiver<(String, String)> {
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut greeting = [0_u8; 2];
        if stream.read_exact(&mut greeting).is_err() {
            return;
        }
        let mut methods = vec![0_u8; usize::from(greeting[1])];
        if stream.read_exact(&mut methods).is_err() {
            return;
        }
        // Version 5, username/password (RFC 1929).
        stream.write_all(&[0x05, 0x02]).ok();
        let mut head = [0_u8; 2];
        if stream.read_exact(&mut head).is_err() {
            return;
        }
        let mut username = vec![0_u8; usize::from(head[1])];
        if stream.read_exact(&mut username).is_err() {
            return;
        }
        let mut length = [0_u8; 1];
        if stream.read_exact(&mut length).is_err() {
            return;
        }
        let mut password = vec![0_u8; usize::from(length[0])];
        if stream.read_exact(&mut password).is_err() {
            return;
        }
        sender
            .send((
                String::from_utf8_lossy(&username).into_owned(),
                String::from_utf8_lossy(&password).into_owned(),
            ))
            .ok();
    });
    receiver
}

/// Two places opened through one SOCKS5 port identify themselves to it
/// differently, so Tor (`IsolateSOCKSAuth`) puts them on two circuits and the
/// host sees two exits rather than one.
#[test]
fn two_places_through_one_socks_port_ride_two_circuits() {
    let mut seen = Vec::new();
    for circuit in [
        Circuit::from_bytes([0x11; 16]),
        Circuit::from_bytes([0x22; 16]),
    ] {
        let (proxy_listener, proxy_port) = bound();
        let credentials = socks5_credentials(proxy_listener);
        let waypoint = HttpWaypoint::new(
            "http://nonexistent.invalid:8963",
            &Access {
                proxy: Some(
                    Proxy::parse(&format!("socks5://127.0.0.1:{proxy_port}"), circuit).unwrap(),
                ),
                patience: Duration::from_secs(2),
                ..Access::default()
            },
        );
        let _ = waypoint.get(&address());
        seen.push(
            credentials
                .recv_timeout(Duration::from_secs(5))
                .expect("the proxy was never offered a username and password"),
        );
    }
    let [(first_user, first_pass), (second_user, second_pass)] = seen.as_slice() else {
        panic!("two connections were expected");
    };
    assert_eq!(first_user, "1111111111111111");
    assert_eq!(first_pass, "1111111111111111");
    assert_ne!(first_user, second_user, "two places shared one circuit");
    assert_ne!((first_user, first_pass), (second_user, second_pass));
}
