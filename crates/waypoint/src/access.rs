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

use std::time::Duration;

use crate::client::PATIENCE;
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
    /// **A SOCKS proxy is always asked to resolve the name.** `socks5://` and
    /// `socks4://` mean "resolve the host here, then ask the proxy for that
    /// address", which sends a plaintext DNS query for the host from this
    /// machine — to the one observer this setting exists to hide the connection
    /// from. `socks5h://` and `socks4a://` hand the name over instead, so those
    /// are what is built, whichever of the pair was typed. Somebody who points
    /// this at Tor and watches their own resolver should see nothing, and before
    /// this they saw every host they talked to.
    ///
    /// # Errors
    ///
    /// [`LocatorError::BadProxy`] when the text does not name a proxy.
    pub fn parse(text: &str) -> Result<Self, LocatorError> {
        ureq::Proxy::new(&deferring(text))
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

/// The same proxy, spelled so that the proxy does the name resolution.
///
/// Only the scheme is touched, so credentials, host and port survive whatever
/// they were: `socks5://user:pw@h:9050` becomes `socks5h://user:pw@h:9050`. A
/// scheme that already defers, or that has no name to defer, is returned as it
/// arrived.
fn deferring(text: &str) -> String {
    let Some((scheme, rest)) = text.split_once("://") else {
        return text.to_owned();
    };
    match scheme.to_ascii_lowercase().as_str() {
        "socks" | "socks5" => format!("socks5h://{rest}"),
        "socks4" => format!("socks4a://{rest}"),
        _ => text.to_owned(),
    }
}

/// What the world outside a locator supplies in order to reach it.
///
/// One value rather than a growing list of parameters, and every part comes
/// from the same place: `kusanagi::assembly` reads the environment once, which
/// is the only module allowed to. A locator says *where*; this says *how to get
/// there from here*, and the two change for different reasons.
#[derive(Debug, Clone)]
pub struct Access {
    /// What signs a request to a bucket.
    pub credentials: Option<Credentials>,
    /// The socket every request leaves through, if not the default one.
    pub proxy: Option<Proxy>,
    /// How long one request may take in total before the verb gives up.
    ///
    /// A parameter rather than a seam: a test needs a second where production
    /// needs a minute, and that is a number, not an implementation.
    pub patience: Duration,
}

/// The production setting, which is also what every test that does not care gets.
///
/// Written out rather than derived, because a derived `Duration::ZERO` here is a
/// client that gives up before it starts — a default that fails closed by
/// accident is still a default nobody chose.
impl Default for Access {
    fn default() -> Self {
        Self {
            credentials: None,
            proxy: None,
            patience: PATIENCE,
        }
    }
}
