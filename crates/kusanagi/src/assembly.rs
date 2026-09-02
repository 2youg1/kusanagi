// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! The only place that knows which concrete things exist.
//!
//! Everything below this module is written against traits and values. This is
//! where a locator becomes a place, where the environment is read, where the
//! clock is sampled — **once per command, at the top** — and where the verbs are
//! composed out of the crates beneath. Keeping that knowledge here is what lets
//! the rest of the program be driven by a test rather than by a shell.
//!
//! The verbs themselves live next door — `membership.rs` for who is on a channel,
//! `traffic.rs` for what moves on it — because this file is the router and the
//! three things that touch the world: the clock, the environment, and a listener.
//!
//! The one rule those modules hold and nothing else does: **a command checks its
//! authority before it does anything.** Sending checks the standing that permits
//! sending, reading checks that the peer was permitted to write, and both check
//! the revocation list, so cutting somebody off takes effect on the next command
//! either side runs.

use std::net::TcpListener;

use kusanagi_box::Server;
use kusanagi_kernel::{Clock as _, Instant, Signer};
use kusanagi_seal::Secret;
use kusanagi_waypoint::{Locator, Place, probe};

use kusanagi_site::Site;

use crate::complaint::Complaint;
use crate::membership::{forget, invite, join, revoke};
use crate::report::Outcome;
use crate::request::Request;
use crate::traffic::{read, send};
use crate::world::{SystemClock, fresh_seed};

/// Carries out one request.
///
/// # Errors
///
/// Any [`Complaint`] the request produces, already carrying the command that
/// would recover from it.
pub fn run(site: &Site, request: &Request) -> Result<Outcome, Complaint> {
    let now = SystemClock.now();
    match request {
        Request::Identity => identity(site),
        Request::Channels => channels(site, now),
        Request::Invite {
            name,
            waypoint,
            lifetime,
            abilities,
        } => invite(site, name, waypoint, *lifetime, *abilities, now),
        Request::Join { invite, name } => join(site, invite, name, now),
        Request::Send { name, payload } => send(site, name, payload, now),
        Request::Read { name, after, whose } => read(site, name, *after, *whose, now),
        Request::Revoke { name } => revoke(site, name),
        Request::Forget { name } => forget(site, name),
        Request::Doctor { waypoint } => doctor(waypoint, now),
        Request::Host { bind, directory } => host(bind, directory),
    }
}

/// This endpoint's signer, created on first use.
///
/// An identity appears the first time one is needed rather than through a setup
/// step, because a setup step is a thing to forget and this one has no decisions
/// in it.
pub(crate) fn signer(site: &Site) -> Result<Signer, Complaint> {
    match site.identity()? {
        Some(signer) => Ok(signer),
        None => Ok(site.adopt(&fresh_seed()?)?),
    }
}

fn identity(site: &Site) -> Result<Outcome, Complaint> {
    Ok(Outcome::Identity {
        handle: signer(site)?.handle().to_string(),
        site: site.root().display().to_string(),
    })
}

/// Lists what is here, with each standing checked against the clock.
///
/// The abilities reported are the ones that survive verification right now, not
/// the ones the record claims: an expired or revoked channel is visible as such
/// in the listing instead of being discovered by a command that fails. No
/// request leaves this machine — expiry and revocation are local facts.
fn channels(site: &Site, now: Instant) -> Result<Outcome, Complaint> {
    let revoked = site.revocations()?;
    let mut channels = Vec::new();
    for name in site.names()? {
        let channel = site.channel(&name)?;
        let me = signer(site)?.handle();
        channels.push(Outcome::summarise(&name, &channel, &me, now, &revoked));
    }
    Ok(Outcome::Channels { channels })
}

/// Opens what a locator names, supplying credentials from the environment.
///
/// The environment is read here and nowhere else, for the same reason the clock
/// is: a program that picks up configuration in the middle of a call graph is a
/// program whose behaviour cannot be reproduced from its arguments.
pub(crate) fn open(locator: &str, now: Instant) -> Result<Place, Complaint> {
    let locator: Locator = locator.parse()?;
    let credentials = match (
        std::env::var("KUSANAGI_S3_ACCESS_KEY"),
        std::env::var("KUSANAGI_S3_SECRET_KEY"),
    ) {
        (Ok(access), Ok(secret)) => Some(kusanagi_waypoint::Credentials::new(&access, &secret)),
        _ => None,
    };
    Ok(Place::open(&locator, credentials, now.as_unix_seconds())?)
}

fn doctor(waypoint: &str, now: Instant) -> Result<Outcome, Complaint> {
    let place = open(waypoint, now)?;
    // A fresh secret every run: the probe writes to real addresses on a real
    // host, and addresses nobody can predict are addresses nothing collides with.
    let namespace =
        Secret::from_bytes(fresh_seed()?).stream(&Signer::from_seed(&fresh_seed()?).handle());
    let certificate = probe::examine(&place, &namespace);
    Ok(Outcome::examined(waypoint, place.kind(), &certificate))
}

fn host(bind: &str, directory: &std::path::Path) -> Result<Outcome, Complaint> {
    let listener = TcpListener::bind(bind).map_err(|source| Complaint::Local {
        action: "listen on that address",
        source,
    })?;
    let address = listener
        .local_addr()
        .map_or_else(|_| bind.to_owned(), |addr| addr.to_string());

    // Progress goes to stderr so that stdout carries only the result. A host runs
    // until it is killed, so the result arrives long after this line.
    eprintln!(
        "kusanagi host: serving {} on {address}",
        directory.display()
    );

    Server::new(directory, SystemClock)
        .serve(&listener)
        .map_err(|source| Complaint::Local {
            action: "serve requests",
            source,
        })?;

    Ok(Outcome::Hosted {
        address,
        directory: directory.display().to_string(),
    })
}
