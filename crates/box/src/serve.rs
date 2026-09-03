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
//!   ignored, so no request in the protocol can overwrite a drop.
//! - **There is no confirmed write.** Every `PUT` is answered `404`, empty,
//!   whether it was stored, refused, or dropped for want of room.
//! - **There is no listing.** A caller who does not already know an address
//!   learns nothing, which is what makes address unlinkability worth anything.
//! - **There is no account.** The server never learns who anybody is, so it has
//!   nothing to disclose.
//! - **There is no self-description.** No response names this program, its
//!   version, or what it is for. A caller who does not already hold an address
//!   gets one answer — `404`, empty — to every question, which is what an
//!   ordinary static file server gives them.
//!
//! The last three are one property: the difference between a host somebody runs
//! and a host somebody can be found running. A banner at a well-known path turns
//! an internet-wide scan into a list of this network's users at one request per
//! address — and so did a `201`, until this file stopped sending one. A host is
//! measured rather than asked: `kusanagi doctor` writes and reads back
//! (`ARCHITECTURE.md` §8), and so does every ordinary write.
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

/// How many bytes a host will hold before it stops accepting more.
///
/// A host answers strangers, and a stranger's write is free to them and not to
/// the disk it lands on. A gigabyte is eight thousand drops: more than any two
/// people will exchange, and small enough that filling it is somebody's project
/// rather than somebody's afternoon.
pub const CAPACITY: u64 = 1_073_741_824;

/// A host: a directory, an HTTP door, and no opinions.
#[derive(Debug)]
pub struct Server<C> {
    drops: DirWaypoint,
    root: PathBuf,
    clock: C,
    capacity: u64,
}

impl<C: Clock> Server<C> {
    /// Serves `root`, dating objects by `clock`, holding at most [`CAPACITY`].
    #[must_use]
    pub fn new(root: impl Into<PathBuf>, clock: C) -> Self {
        let root = root.into();
        Self {
            drops: DirWaypoint::new(root.clone()),
            root,
            clock,
            capacity: CAPACITY,
        }
    }

    /// The same host, holding at most `bytes`.
    #[must_use]
    pub const fn holding(mut self, bytes: u64) -> Self {
        self.capacity = bytes;
        self
    }

    /// What this host is already holding, in bytes.
    ///
    /// Walked per write rather than counted incrementally, because a counter is
    /// state and a host that is killed loses it. A box holds thousands of files,
    /// not millions, and the walk is one `readdir` per shard.
    fn held(&self) -> u64 {
        fn beneath(path: &std::path::Path) -> u64 {
            let Ok(entries) = std::fs::read_dir(path) else {
                return 0;
            };
            entries
                .flatten()
                .map(|entry| match entry.metadata() {
                    Ok(data) if data.is_dir() => beneath(&entry.path()),
                    Ok(data) => data.len(),
                    Err(_) => 0,
                })
                .fold(0_u64, u64::saturating_add)
        }
        beneath(&self.root)
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
            Err(_) => Response::empty(400),
        };
        response.write(reader.get_mut())
    }

    /// Routes one request, telling a stranger nothing.
    ///
    /// Every answer that is not about a drop somebody already knows the address
    /// of is the same answer: `404` with an empty body. A method this host does
    /// not implement, a path that is not a drop, and a drop that is not there are
    /// deliberately one response rather than three — three would let a scanner
    /// recover the address grammar, and the grammar is enough to tell this host
    /// apart from any other server on the same port.
    fn route(&self, request: &Request) -> Response {
        match (request.method.as_str(), address_of(&request.target)) {
            ("GET", Some(addr)) => self.read(&addr, request),
            ("PUT", Some(addr)) => self.write(&addr, request),
            _ => Response::empty(404),
        }
    }

    fn read(&self, addr: &DropAddr, request: &Request) -> Response {
        // The reason stays on this machine. A message describing what failed on
        // the host's own disk is a description of the host's software.
        let Ok(stored) = self.drops.get(addr) else {
            return Response::empty(500);
        };
        let Some(bytes) = stored.and_then(|envelope| self.unwrap_envelope(&envelope)) else {
            return Response::empty(404);
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

    /// Takes a write, and says nothing about it either way.
    ///
    /// **Every outcome is the same empty `404` every other request gets.** §3
    /// claims a host answers a stranger exactly as a static file server does, and
    /// a static file server does not answer `201` to a `PUT`. Before this, one
    /// request to `/d/<40 hex>` told a scanner it had found a box; a scan of the
    /// internet was therefore a list of them, and the list is the relationship
    /// graph's first column.
    ///
    /// The caller loses nothing, because it never believed the status anyway:
    /// `waypoint::http` reads the address back and compares bytes.
    /// **Hosts are measured, not believed** — §8 already ruled that for reads,
    /// and this is the write path catching up.
    ///
    /// Nothing is written once the directory is full, and that is also a `404`.
    /// A host that reported fullness would be telling a stranger how much of it
    /// they had used.
    fn write(&self, addr: &DropAddr, request: &Request) -> Response {
        let refused = Response::empty(404);
        if request.header("if-none-match") != Some("*") {
            return refused;
        }
        let expires_at = match request.header("cache-control").and_then(max_age) {
            None => Instant::NEVER,
            Some(seconds) => self.clock.now().plus_seconds(seconds),
        };

        let mut envelope = expires_at.as_unix_seconds().to_be_bytes().to_vec();
        envelope.extend_from_slice(&request.body);
        let wanted = u64::try_from(envelope.len()).unwrap_or(u64::MAX);
        if self.held().saturating_add(wanted) > self.capacity {
            return refused;
        }
        match self.drops.put_if_absent(addr, &envelope) {
            Ok(PutOutcome::Stored | PutOutcome::AlreadyPresent) | Err(_) => refused,
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

/// The lifetime a `Cache-Control` value asks for, if it asks for one.
///
/// `max-age` is the header a browser, a CDN and a package manager all send
/// anyway, so a lifetime asks for itself the way everything else on the wire
/// does. A header named after this product would be a fingerprint in every
/// request, readable by every proxy and log on the path.
///
/// An unparsable directive is ignored rather than refused, which is what
/// RFC 9111 §5.2 asks of a recipient and also what keeps a malformed value from
/// being a way to tell this host apart from a cache.
fn max_age(value: &str) -> Option<u64> {
    value
        .split(',')
        .filter_map(|directive| directive.trim().strip_prefix("max-age="))
        .find_map(|seconds| seconds.trim().parse::<u64>().ok())
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
    use kusanagi_waypoint::{Access, Conditional as _, Fetched, HttpWaypoint, TtlOutcome};
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
        (
            HttpWaypoint::new(&format!("http://127.0.0.1:{port}"), &Access::default()),
            root,
        )
    }

    #[test]
    fn a_lifetime_is_read_out_of_an_ordinary_cache_header() {
        // Every one of these is something a real cache sends. Getting any of
        // them wrong is either an object that never expires when it should, or
        // one that expires immediately when it should not.
        assert_eq!(super::max_age("max-age=60"), Some(60));
        assert_eq!(super::max_age("no-cache, max-age=60"), Some(60));
        assert_eq!(super::max_age(" max-age = 60 "), None);
        assert_eq!(super::max_age("max-age=0"), Some(0));
        assert_eq!(
            super::max_age("public, max-age=3600, immutable"),
            Some(3_600)
        );
        // Not a lifetime, and ignored rather than refused: RFC 9111 §5.2 asks a
        // recipient to skip what it does not understand, and refusing would make
        // a malformed value into a way of telling this host apart from a cache.
        assert_eq!(super::max_age("max-age="), None);
        assert_eq!(super::max_age("max-age=soon"), None);
        assert_eq!(super::max_age("max-age=-1"), None);
        assert_eq!(super::max_age("s-maxage=60"), None);
        assert_eq!(super::max_age(""), None);
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
