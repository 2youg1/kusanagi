// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! What the world outside a locator supplies in order to reach it.
//!
//! kusanagi does not hide an endpoint's IP address and never claimed to: the
//! host learns it, and a path observer reads the DNS query and the TLS SNI that
//! precede the connection. `ARCHITECTURE.md` §3 states that boundary, and §8
//! rules on what to do about it — **hand the problem to a network built for it,
//! and provide the socket rather than the network.**
//!
//! So this is an interface and not a mechanism. What it accepts is what those
//! networks offer: a SOCKS5 listener, which is how Tor and every VPN client on
//! the desktop expose themselves, or an HTTP CONNECT proxy, which is what a
//! corporate egress is.

use crate::place::LocatorError;
use crate::sigv4::Credentials;

/// Where to send every request instead of straight at the host.
///
/// Deliberately a type of ours rather than the client library's. It is reached
/// from the assembly, which is the one module allowed to read the environment,
/// and a parse failure has to arrive as a [`LocatorError`] with a stable code —
/// a caller who mistypes a proxy has to be told that, not handed a message from
/// a crate they did not know they were using.
#[derive(Debug, Clone)]
pub struct Proxy(ureq::Proxy);

impl Proxy {
    /// Reads `socks5://host:port` or `http://host:port`, with optional
    /// credentials.
    ///
    /// # Errors
    ///
    /// [`LocatorError::BadProxy`] when the text does not name a proxy.
    pub fn parse(text: &str) -> Result<Self, LocatorError> {
        ureq::Proxy::new(text)
            .map(Self)
            .map_err(|source| LocatorError::BadProxy {
                reason: source.to_string(),
            })
    }

    /// Applies this proxy to a client configuration.
    ///
    /// Takes an `Option<&Self>` so that "no proxy" and "this proxy" are one call
    /// at every site rather than a branch each adapter repeats.
    pub(crate) fn applied(
        proxy: Option<&Self>,
        builder: ureq::config::ConfigBuilder<ureq::typestate::AgentScope>,
    ) -> ureq::config::ConfigBuilder<ureq::typestate::AgentScope> {
        match proxy {
            None => builder,
            Some(Self(proxy)) => builder.proxy(Some(proxy.clone())),
        }
    }
}

/// What the world outside a locator supplies in order to reach it.
///
/// One value rather than a growing list of parameters, and both halves come
/// from the same place: `kusanagi::assembly` reads the environment once, which
/// is the only module allowed to. A locator says *where*; this says *how to get
/// there from here*, and the two change for different reasons.
#[derive(Debug, Clone, Default)]
pub struct Access {
    /// What signs a request to a bucket.
    pub credentials: Option<Credentials>,
    /// The socket every request leaves through, if not the default one.
    pub proxy: Option<Proxy>,
}
