// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The one client every host is reached through, and the three limits it holds.
//!
//! A host is not trusted, and an untrusted host is reached through a client that
//! does only what it was told to. Three defaults of a general-purpose HTTP client
//! are wrong for this one, and each is wrong in a way that costs something this
//! design says it protects:
//!
//! 1. **Redirects are not followed.** A host that answers `302` is asking this
//!    endpoint to open a connection to somewhere it did not choose, which hands
//!    a third party the endpoint's IP address and the drop address it wanted —
//!    over plain HTTP, in front of every observer on the path. A box has no
//!    reason to redirect, so a redirect is a refusal here.
//! 2. **Every request has a deadline.** Law 1 says a verb is a one-shot command
//!    that exits; a host that accepts a connection and then says nothing turns it
//!    into a process that never does.
//! 3. **Every response body is capped.** What comes back is allocated before it
//!    is verified, so the cap is what stops a host from choosing this endpoint's
//!    memory footprint.
//!
//! They live together because they answer one question — *what may a host do to
//! a caller?* — and because a second place holding any of them is a second place
//! to forget one.

use std::time::Duration;

use kusanagi_kernel::WaypointError;

use crate::access::{Access, Proxy};

/// The largest object this network ever reads back from a host, in bytes.
///
/// A drop is 131 072 bytes and an S3 error document is a few hundred, so this is
/// eight times the largest thing that can legitimately arrive. `kusanagi-box`
/// refuses to *accept* more than this for the same reason, and takes the number
/// from here rather than restating it.
pub const MAX_OBJECT: u64 = 1_048_576;

/// How long a host has, in total, to answer one request.
///
/// Total rather than per-read: a host that sends one byte a minute would satisfy
/// any idle timeout forever, and the thing being bounded is how long a verb can
/// take before it exits.
pub const PATIENCE: Duration = Duration::from_mins(1);

/// A configured HTTP client, and the vocabulary its failures arrive in.
#[derive(Debug, Clone)]
pub(crate) struct Client {
    agent: ureq::Agent,
    patience: Duration,
}

impl Client {
    /// Builds the client `access` describes.
    ///
    /// **No `User-Agent`.** An agent string puts a library name and version into
    /// every request, where a host, a proxy and any log on the path can read it,
    /// and a version narrows the set of people it could be. Sending none says
    /// less than sending anything, and is unremarkable: plenty of API traffic
    /// carries no agent string. A borrowed browser header above a handshake that
    /// is plainly not a browser's would be a worse tell than silence.
    ///
    /// Status codes are *not* treated as transport errors: 404, 412 and 304 are
    /// all normal answers in this protocol, and an adapter that could not tell
    /// them apart from a broken connection would have to guess.
    pub(crate) fn new(access: &Access) -> Self {
        let agent = Proxy::applied(
            access.proxy.as_ref(),
            ureq::Agent::config_builder()
                .http_status_as_error(false)
                .user_agent(ureq::config::AutoHeaderValue::None)
                .max_redirects(0)
                .timeout_global(Some(access.patience)),
        )
        .build()
        .into();
        Self {
            agent,
            patience: access.patience,
        }
    }

    /// The agent to build a request on.
    pub(crate) const fn agent(&self) -> &ureq::Agent {
        &self.agent
    }

    /// Names the transport failure `source` in this crate's vocabulary.
    ///
    /// A timeout is separated from every other kind of io because it is the one
    /// a caller should retry: everything else that reaches here is a host that
    /// answered wrongly, not slowly.
    pub(crate) fn failed(&self, action: &'static str, source: &ureq::Error) -> WaypointError {
        match source {
            ureq::Error::Timeout(_) => WaypointError::Unanswered {
                action,
                after: self.patience,
            },
            other => WaypointError::Io {
                action,
                source: std::io::Error::other(other.to_string()),
            },
        }
    }

    /// The status of a response a caller may act on.
    ///
    /// A redirect is not one. `max_redirects(0)` makes ureq hand the redirect
    /// back instead of following it, and this is where that becomes a refusal —
    /// which is the whole point of not following it.
    ///
    /// **304 is not a redirect and is excluded by name.** It carries no
    /// `Location` and asks the caller to go nowhere: it is how a host answers
    /// `If-None-Match` with *you already have this*, which is a first-class
    /// answer in this protocol and the reason `Conditional` exists.
    pub(crate) fn actionable(
        action: &'static str,
        response: &ureq::http::Response<ureq::Body>,
    ) -> Result<u16, WaypointError> {
        const NOT_MODIFIED: u16 = 304;
        let status = response.status().as_u16();
        if (300..400).contains(&status) && status != NOT_MODIFIED {
            return Err(WaypointError::Redirected {
                action,
                to: response
                    .headers()
                    .get("location")
                    .and_then(|value| value.to_str().ok())
                    .unwrap_or("somewhere it did not name")
                    .to_owned(),
            });
        }
        Ok(status)
    }

    /// Reads a response body, refusing to allocate more than [`MAX_OBJECT`].
    pub(crate) fn body(
        action: &'static str,
        response: &mut ureq::http::Response<ureq::Body>,
    ) -> Result<Vec<u8>, WaypointError> {
        response
            .body_mut()
            .with_config()
            .limit(MAX_OBJECT)
            .read_to_vec()
            .map_err(|source| WaypointError::Io {
                action,
                source: std::io::Error::other(source.to_string()),
            })
    }
}
