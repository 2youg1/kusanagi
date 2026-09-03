// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! One request in, one response out, both bounded before they are believed.
//!
//! A host answers strangers, so every limit here is a refusal to allocate on
//! somebody else's word: the head is capped, the body is capped, and a socket
//! that goes quiet is dropped. Nothing in this file knows what a drop is — that
//! is `serve.rs`, which routes what this file parses.

use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::net::TcpStream;
use std::time::Duration;

use kusanagi_kernel::{DropAddr, Hex};
use kusanagi_waypoint::MAX_OBJECT;

/// How long a connection may stay silent before it is dropped.
pub(crate) const IDLE: Duration = Duration::from_secs(30);

/// The largest request head this server will read, in bytes.
pub(crate) const MAX_HEAD: usize = 8_192;

/// The address a request target names, if it names one.
pub(crate) fn address_of(target: &str) -> Option<DropAddr> {
    target.strip_prefix("/d/")?.parse().ok()
}

/// A version name for these exact bytes.
///
/// The content hash, so it is stable by construction rather than by the host
/// remembering to keep it stable — which is one of the four things `doctor`
/// measures, and one this host cannot fail.
pub(crate) fn etag(bytes: &[u8]) -> String {
    format!("\"{}\"", Hex(blake3::hash(bytes).as_bytes()))
}

/// A request this host will not act on.
///
/// Carries no reason, because no reason is ever sent and a reason that is built
/// but not sent is an invitation to send it. What went wrong is the caller's to
/// know from the status.
pub(crate) struct Malformed;

/// One request, already bounded.
pub(crate) struct Request {
    pub(crate) method: String,
    pub(crate) target: String,
    headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl Request {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(header, _)| header == name)
            .map(|(_, value)| value.as_str())
    }

    pub(crate) fn read(reader: &mut BufReader<TcpStream>) -> Result<Self, Malformed> {
        let mut line = String::new();
        take_line(reader, &mut line)?;
        let mut parts = line.split_whitespace();
        let method = parts.next().ok_or(Malformed)?.to_owned();
        let target = parts.next().ok_or(Malformed)?.to_owned();

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
                return Err(Malformed);
            }
            if let Some((name, value)) = trimmed.split_once(':') {
                headers.push((name.trim().to_lowercase(), value.trim().to_owned()));
            }
        }

        // Declared in `u64` and compared before it is turned into a length, so
        // that a caller who announces 2^63 bytes is refused rather than sized
        // for. The cap is the client's, taken from `kusanagi-waypoint` rather
        // than restated: what this host accepts and what a caller will read back
        // are one number, and two copies of it drift.
        let declared = headers
            .iter()
            .find(|(name, _)| name == "content-length")
            .map_or(Ok(0), |(_, value)| value.parse::<u64>())
            .map_err(|_| Malformed)?;
        if declared > MAX_OBJECT {
            return Err(Malformed);
        }
        let declared = usize::try_from(declared).map_err(|_| Malformed)?;
        let mut body = vec![0_u8; declared];
        reader.read_exact(&mut body).map_err(|_| Malformed)?;

        Ok(Self {
            method,
            target,
            headers,
            body,
        })
    }
}

/// Reads one line, treating a closed connection as a request that never came.
///
/// An empty read and a broken socket are the same answer here, and the reason is
/// the same one that makes every refusal an empty response: what this host knows
/// about how a caller failed is not a caller's to learn.
fn take_line(reader: &mut BufReader<TcpStream>, into: &mut String) -> Result<(), Malformed> {
    match reader.read_line(into) {
        Ok(read) if read > 0 => Ok(()),
        _ => Err(Malformed),
    }
}

/// One response, written and then closed.
pub(crate) struct Response {
    pub(crate) status: u16,
    pub(crate) etag: Option<String>,
    pub(crate) body: Vec<u8>,
}

impl Response {
    /// An answer that is only a status.
    ///
    /// Every refusal this host makes uses it. A body explaining what went wrong
    /// would describe this host's parser, its storage and its rules to whoever
    /// asked — and whoever asked may be enumerating servers rather than reading
    /// a drop.
    pub(crate) const fn empty(status: u16) -> Self {
        Self {
            status,
            etag: None,
            body: Vec::new(),
        }
    }

    pub(crate) fn write(&self, stream: &mut TcpStream) -> Result<(), io::Error> {
        let reason = match self.status {
            200 => "OK",
            201 => "Created",
            304 => "Not Modified",
            400 => "Bad Request",
            404 => "Not Found",
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
