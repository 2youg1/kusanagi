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

use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

use kusanagi_kernel::{Clock, DropAddr, Hex, Instant, PutOutcome, Waypoint as _};

use crate::dir::DirWaypoint;

/// The largest request head this server will read, in bytes.
const MAX_HEAD: usize = 8_192;

/// The largest body this server will accept, in bytes. A segment is capped well
/// below this; the margin is for the envelope and for a client that pads.
const MAX_BODY: usize = 1_048_576;

/// How long a connection may stay silent before it is dropped.
const IDLE: Duration = Duration::from_secs(30);

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

/// The address a request target names, if it names one.
fn address_of(target: &str) -> Option<DropAddr> {
    target.strip_prefix("/d/")?.parse().ok()
}

/// A version name for these exact bytes.
///
/// The content hash, so it is stable by construction rather than by the host
/// remembering to keep it stable — which is one of the four things `doctor`
/// measures, and one this host cannot fail.
fn etag(bytes: &[u8]) -> String {
    format!("\"{}\"", Hex(blake3::hash(bytes).as_bytes()))
}

/// One request, already bounded.
struct Request {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    fn read(reader: &mut BufReader<TcpStream>) -> Result<Self, String> {
        let mut line = String::new();
        take_line(reader, &mut line)?;
        let mut parts = line.split_whitespace();
        let method = parts.next().ok_or("no method")?.to_owned();
        let target = parts.next().ok_or("no request target")?.to_owned();

        let mut headers = Vec::new();
        let mut head_size = line.len();
        loop {
            let mut header = String::new();
            take_line(reader, &mut header)?;
            let trimmed = header.trim_end();
            if trimmed.is_empty() {
                break;
            }
            head_size = head_size.saturating_add(header.len());
            if head_size > MAX_HEAD {
                return Err("request head too large".to_owned());
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
            }
        }

        let declared = headers
            .iter()
            .find(|(name, _)| name == "content-length")
            .map_or(Ok(0), |(_, value)| value.parse::<usize>())
            .map_err(|_| "content-length is not a number".to_owned())?;
        if declared > MAX_BODY {
            return Err(format!("a body of {declared} bytes is too large"));
        }
        let mut body = vec![0_u8; declared];
        reader
            .read_exact(&mut body)
            .map_err(|error| format!("the body ended early: {error}"))?;

        Ok(Self {
            method,
            target,
            headers,
            body,
        })
    }
}

fn take_line(reader: &mut BufReader<TcpStream>, into: &mut String) -> Result<(), String> {
    match reader.read_line(into) {
        Ok(0) => Err("the connection closed before a request arrived".to_owned()),
        Ok(_) => Ok(()),
        Err(error) => Err(format!("could not read the request: {error}")),
    }
}

/// One response, written and then closed.
struct Response {
    status: u16,
    etag: Option<String>,
    body: Vec<u8>,
}

impl Response {
    fn text(status: u16, message: &str) -> Self {
        Self {
            status,
            etag: None,
            body: message.as_bytes().to_vec(),
        }
    }

    fn write(&self, stream: &mut TcpStream) -> Result<(), io::Error> {
        let reason = match self.status {
            200 => "OK",
            201 => "Created",
            304 => "Not Modified",
            400 => "Bad Request",
            404 => "Not Found",
            405 => "Method Not Allowed",
            412 => "Precondition Failed",
            428 => "Precondition Required",
            _ => "Internal Server Error",
        };
        let tag = self
            .etag
            .as_ref()
            .map_or_else(String::new, |tag| format!("ETag: {tag}\r\n"));
        let head = format!(
            "HTTP/1.1 {} {reason}\r\nContent-Length: {}\r\nConnection: close\r\n{tag}\r\n",
            self.status,
            self.body.len()
        );
        stream.write_all(head.as_bytes())?;
        stream.write_all(&self.body)?;
        stream.flush()
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
    use crate::conditional::{Conditional as _, Fetched, TtlOutcome};
    use crate::http::HttpWaypoint;
    use kusanagi_kernel::{FixedClock, Instant, PutOutcome, Signer, Waypoint as _};
    use kusanagi_seal::{Secret, Stream, derive};
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
        crate::conformance::run(&client, &namespace(5)).expect("the box broke the contract");
        std::fs::remove_dir_all(&root).ok();
    }
}
