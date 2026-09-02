// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The other end of `http.rs`: a host anybody can run, and nobody has to trust.
//!
//! It is here rather than in the binary because a protocol with its two halves in
//! two crates is a protocol with two authorities. The client and the server are
//! written against the same four rules, and the tests in this file drive the
//! second through the first.
//!
//! What the server refuses is as important as what it does:
//!
//! - **There is no unconditional write.** A `PUT` without `If-None-Match: *` is
//!   answered `428`, so no request in the protocol can overwrite a drop.
//! - **There is no listing.** A caller who does not already know an address
//!   learns nothing, which is what makes address unlinkability worth anything.
//! - **There is no account.** The server never learns who anybody is, so it has
//!   nothing to disclose.
//!
//! Objects carry an expiry in front of the bytes, so a swept object and an object
//! that was never written are the same answer, `404`, with no bookkeeping to
//! reconcile.

use std::io::{self, BufReader};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;

use kusanagi_kernel::{Clock, DropAddr, Instant, PutOutcome, Waypoint as _};

use crate::exchange::{IDLE, Request, Response, address_of, etag};
use kusanagi_waypoint::DirWaypoint;

/// What this server says it can do, for `doctor` and for people.
const BANNER: &str = "kusanagi-box/1 write-once=yes conditional-read=yes expiry=yes";

/// A host: a directory, an HTTP door, and no opinions.
#[derive(Debug)]
pub struct Server<C> {
    drops: DirWaypoint,
    clock: C,
}

impl<C: Clock> Server<C> {
    /// Serves `root`, dating objects by `clock`.
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, clock: C) -> Self {
        Self {
            drops: DirWaypoint::new(root.into()),
            clock,
        }
    }

    /// Answers requests until the listener fails.
    ///
    /// One thread per connection and `Connection: close` on every response: a box
    /// serving a handful of agents does not need a connection pool, and the
    /// simplest thing that cannot deadlock is worth more here than throughput.
    ///
    /// # Errors
    ///
    /// [`io::Error`] only when the listener itself stops working. A failure while
    /// answering one caller is reported on stderr and does not stop the others.
    pub fn serve(&self, listener: &TcpListener) -> Result<(), io::Error>
    where
        C: Sync,
    {
        std::thread::scope(|scope| {
            for incoming in listener.incoming() {
                match incoming {
                    Ok(stream) => {
                        scope.spawn(|| match self.answer(stream) {
                            Ok(()) => {}
                            Err(error) => eprintln!("kusanagi host: {error}"),
                        });
                    }
                    Err(error) => eprintln!("kusanagi host: accept failed: {error}"),
                }
            }
            Ok(())
        })
    }

    /// Answers exactly one request.
    ///
    /// # Errors
    ///
    /// [`io::Error`] when the connection fails. A malformed request is a response,
    /// not an error.
    pub fn answer(&self, stream: TcpStream) -> Result<(), io::Error> {
        stream.set_read_timeout(Some(IDLE))?;
        stream.set_write_timeout(Some(IDLE))?;
        let mut reader = BufReader::new(stream);
        let response = match Request::read(&mut reader) {
            Ok(request) => self.route(&request),
            Err(reason) => Response::text(400, &reason),
        };
        response.write(reader.get_mut())
    }

    fn route(&self, request: &Request) -> Response {
        match (request.method.as_str(), request.target.as_str()) {
            ("GET", "/health") => Response::text(200, BANNER),
            ("GET", target) => match address_of(target) {
                Some(addr) => self.read(&addr, request),
                None => Response::text(404, "no such resource"),
            },
            ("PUT", target) => match address_of(target) {
                Some(addr) => self.write(&addr, request),
                None => Response::text(404, "no such resource"),
            },
            _ => Response::text(405, "this host answers GET and PUT"),
        }
    }

    fn read(&self, addr: &DropAddr, request: &Request) -> Response {
        let stored = match self.drops.get(addr) {
            Ok(stored) => stored,
            Err(error) => return Response::text(500, &error.to_string()),
        };
        let Some(bytes) = stored.and_then(|envelope| self.unwrap_envelope(&envelope)) else {
            return Response::text(404, "nothing is here");
        };

        let tag = etag(&bytes);
        if request
            .header("if-none-match")
            .is_some_and(|value| value == tag)
        {
            return Response {
                status: 304,
                etag: Some(tag),
                body: Vec::new(),
            };
        }
        Response {
            status: 200,
            etag: Some(tag),
            body: bytes,
        }
    }

    fn write(&self, addr: &DropAddr, request: &Request) -> Response {
        if request.header("if-none-match") != Some("*") {
            return Response::text(
                428,
                "this host has no unconditional write; send If-None-Match: *",
            );
        }
        let expires_at = match request.header("x-kusanagi-ttl") {
            None => Instant::NEVER,
            Some(value) => match value.trim().parse::<u64>() {
                Ok(seconds) => self.clock.now().plus_seconds(seconds),
                Err(_) => return Response::text(400, "a lifetime is a whole number of seconds"),
            },
        };

        let mut envelope = expires_at.as_unix_seconds().to_be_bytes().to_vec();
        envelope.extend_from_slice(&request.body);
        match self.drops.put_if_absent(addr, &envelope) {
            Ok(PutOutcome::Stored) => Response::text(201, "stored"),
            Ok(PutOutcome::AlreadyPresent) => {
                Response::text(412, "this drop has already been claimed")
            }
            Err(error) => Response::text(500, &error.to_string()),
        }
    }

    /// Strips the expiry header, returning the bytes only while they still live.
    fn unwrap_envelope(&self, envelope: &[u8]) -> Option<Vec<u8>> {
        let (stamp, bytes) = envelope.split_at_checked(8)?;
        let expires_at =
            Instant::from_unix_seconds(u64::from_be_bytes(<[u8; 8]>::try_from(stamp).ok()?));
        (self.clock.now() < expires_at).then(|| bytes.to_vec())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test code"
)]
mod tests {
    use super::Server;
    use kusanagi_kernel::{FixedClock, Instant, PutOutcome, Signer, Waypoint as _};
    use kusanagi_seal::{Secret, Stream, derive};
    use kusanagi_waypoint::{Conditional as _, Fetched, HttpWaypoint, TtlOutcome};
    use std::net::TcpListener;

    fn namespace(tag: u8) -> Stream {
        Secret::from_bytes([tag; 32]).stream(&Signer::from_seed(&[tag; 32]).handle())
    }

    /// Starts a real server on a real port and returns a client pointed at it.
    ///
    /// The two processes of the acceptance criterion become two threads here;
    /// what crosses between them is a TCP connection either way, which is the
    /// part that had never been exercised before this module existed.
    fn box_on(tag: &str, clock: FixedClock) -> (HttpWaypoint, std::path::PathBuf) {
        let root =
            std::env::temp_dir().join(format!("kusanagi-serve-{}-{tag}", std::process::id()));
        let listener = TcpListener::bind("127.0.0.1:0").expect("could not bind a port");
        let port = listener.local_addr().expect("no local address").port();
        let directory = root.clone();
        std::thread::spawn(move || {
            let host = Server::new(&directory, clock);
            match host.serve(&listener) {
                Ok(()) => {}
                Err(error) => eprintln!("test host stopped: {error}"),
            }
        });
        (HttpWaypoint::new(&format!("http://127.0.0.1:{port}")), root)
    }

    #[test]
    fn a_segment_crosses_a_tcp_connection_and_comes_back_whole() {
        let (client, root) = box_on(
            "roundtrip",
            FixedClock::at(Instant::from_unix_seconds(1_000)),
        );
        let (addr, _) = derive(&namespace(1), 0);

        assert_eq!(client.get(&addr).unwrap(), None);
        assert_eq!(
            client.put_if_absent(&addr, b"a segment").unwrap(),
            PutOutcome::Stored
        );
        assert_eq!(client.get(&addr).unwrap(), Some(b"a segment".to_vec()));
        assert!(client.health().unwrap().starts_with("kusanagi-box/1"));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_claimed_drop_is_refused_a_second_time() {
        let (client, root) = box_on(
            "write-once",
            FixedClock::at(Instant::from_unix_seconds(1_000)),
        );
        let (addr, _) = derive(&namespace(2), 0);

        assert_eq!(
            client.put_if_absent(&addr, b"first").unwrap(),
            PutOutcome::Stored
        );
        assert_eq!(
            client.put_if_absent(&addr, b"second").unwrap(),
            PutOutcome::AlreadyPresent
        );
        assert_eq!(client.get(&addr).unwrap(), Some(b"first".to_vec()));

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn a_reader_that_is_current_is_told_so_without_the_bytes() {
        let (client, root) = box_on(
            "conditional",
            FixedClock::at(Instant::from_unix_seconds(1_000)),
        );
        let (addr, _) = derive(&namespace(3), 0);
        client.put_if_absent(&addr, b"a segment").unwrap();

        let Fetched::Fresh { validator, .. } = client.get_if_changed(&addr, None).unwrap() else {
            panic!("the host did not send the bytes it was holding");
        };
        let validator = validator.expect("the host named no version");
        assert_eq!(
            client.get_if_changed(&addr, Some(&validator)).unwrap(),
            Fetched::Unchanged
        );

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn an_object_written_already_expired_is_never_served() {
        let (client, root) = box_on("expiry", FixedClock::at(Instant::from_unix_seconds(1_000)));
        let (addr, _) = derive(&namespace(4), 0);

        assert_eq!(
            client.put_with_ttl(&addr, b"transient", 0).unwrap(),
            TtlOutcome::Accepted
        );
        assert_eq!(client.get(&addr).unwrap(), None);

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn the_whole_contract_holds_over_tcp() {
        let (client, root) = box_on(
            "conformance",
            FixedClock::at(Instant::from_unix_seconds(1_000)),
        );
        kusanagi_waypoint::conformance::run(&client, &namespace(5))
            .expect("the box broke the contract");
        std::fs::remove_dir_all(&root).ok();
    }
}
