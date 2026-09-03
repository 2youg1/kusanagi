// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! A host on a real port, and a way to say anything at all to it.
//!
//! Both files that use this speak to the server over a socket rather than
//! through `HttpWaypoint`, and for the same reason: the client is not the thing
//! under suspicion. A scanner does not use our client, and neither does somebody
//! trying to get a second write past the door.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "test code"
)]

use std::io::{Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use kusanagi_box::Server;
use kusanagi_kernel::{FixedClock, Instant};

/// Starts a host that answers `count` requests and then stops.
pub fn host(tag: &str, count: usize) -> (String, PathBuf) {
    let root = std::env::temp_dir().join(format!("kusanagi-{tag}-{}", std::process::id()));
    std::fs::remove_dir_all(&root).ok();
    let listener = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
    let port = listener.local_addr().expect("no local address").port();
    let directory = root.clone();
    std::thread::spawn(move || {
        let server = Server::new(
            &directory,
            FixedClock::at(Instant::from_unix_seconds(1_000)),
        );
        for _ in 0..count {
            match listener.accept() {
                Ok((stream, _)) => match server.answer(stream) {
                    Ok(()) => {}
                    Err(error) => eprintln!("test host stopped: {error}"),
                },
                Err(error) => eprintln!("test host could not accept: {error}"),
            }
        }
    });
    (format!("127.0.0.1:{port}"), root)
}

/// Sends exactly these bytes and returns exactly what came back.
pub fn probe(address: &str, request: &str) -> Vec<u8> {
    let mut stream = TcpStream::connect(address).expect("the host is not listening");
    stream.write_all(request.as_bytes()).expect("could not ask");
    stream.flush().expect("could not flush");
    let mut answer = Vec::new();
    stream
        .read_to_end(&mut answer)
        .expect("no answer came back");
    answer
}

/// The status code an answer carries.
pub fn status(answer: &[u8]) -> u16 {
    String::from_utf8_lossy(answer)
        .split_whitespace()
        .nth(1)
        .and_then(|code| code.parse().ok())
        .unwrap_or(0)
}
