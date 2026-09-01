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
//! The one rule this module holds and nothing else does: **a command checks its
//! authority before it does anything.** Sending checks the standing that permits
//! sending, reading checks that the peer was permitted to write, and both check
//! the revocation list, so cutting somebody off takes effect on the next command
//! either side runs.

use std::net::TcpListener;

use kusanagi_grant::{Ability, Grant, Scope};
use kusanagi_kernel::{Clock as _, Instant, PutOutcome, Segment, Signer, Waypoint as _};
use kusanagi_seal::{Secret, derive, seal};
use kusanagi_waypoint::{Locator, Place, Server, probe};

use crate::channel::{Channel, Peer, Standing};
use crate::complaint::Complaint;
use crate::invite::Invite;
use crate::report::Outcome;
use crate::request::Request;
use crate::site::Site;
use crate::walk::{peek, walk};
use crate::world::{SystemClock, fresh_seed};

/// The height of the introduction stream that carries a newcomer's greeting.
const INTRODUCTION: u64 = 0;

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
        Request::Channels => channels(site),
        Request::Invite {
            name,
            waypoint,
            lifetime,
            abilities,
        } => invite(site, name, waypoint, *lifetime, *abilities, now),
        Request::Join { invite, name } => join(site, invite, name, now),
        Request::Send { name, payload } => send(site, name, payload, now),
        Request::Read { name, after } => read(site, name, *after, now),
        Request::Revoke { name } => revoke(site, name),
        Request::Doctor { waypoint } => doctor(waypoint, now),
        Request::Host { bind, directory } => host(bind, directory),
    }
}

/// This endpoint's signer, created on first use.
///
/// An identity appears the first time one is needed rather than through a setup
/// step, because a setup step is a thing to forget and this one has no decisions
/// in it.
fn signer(site: &Site) -> Result<Signer, Complaint> {
    match site.identity()? {
        Some(signer) => Ok(signer),
        None => site.adopt(&fresh_seed()?),
    }
}

fn identity(site: &Site) -> Result<Outcome, Complaint> {
    Ok(Outcome::Identity {
        handle: signer(site)?.handle().to_string(),
        site: site.root().display().to_string(),
    })
}

fn channels(site: &Site) -> Result<Outcome, Complaint> {
    let mut channels = Vec::new();
    for name in site.names()? {
        let channel = site.channel(&name)?;
        channels.push(Outcome::summarise(&name, &channel));
    }
    Ok(Outcome::Channels { channels })
}

/// Opens what a locator names, supplying credentials from the environment.
///
/// The environment is read here and nowhere else, for the same reason the clock
/// is: a program that picks up configuration in the middle of a call graph is a
/// program whose behaviour cannot be reproduced from its arguments.
fn open(locator: &str, now: Instant) -> Result<Place, Complaint> {
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

fn invite(
    site: &Site,
    name: &str,
    waypoint: &str,
    lifetime: u64,
    abilities: kusanagi_grant::Abilities,
    now: Instant,
) -> Result<Outcome, Complaint> {
    if site.holds(name)? {
        return Err(Complaint::ChannelExists {
            name: name.to_owned(),
        });
    }
    // Parsed before anything is written, so a mistyped locator costs nothing.
    let _: Locator = waypoint.parse()?;

    let me = signer(site)?;
    let secret = Secret::from_bytes(fresh_seed()?);
    let bearer_seed = fresh_seed()?;
    let bearer = Signer::from_seed(&bearer_seed);
    let expires_at = now.plus_seconds(lifetime);
    let grant = Grant::issue(&me, &bearer.handle(), Scope::new(abilities, expires_at));

    site.keep(
        name,
        &Channel {
            secret: secret.clone(),
            root: me.handle(),
            introduction: bearer.handle(),
            locator: waypoint.to_owned(),
            standing: Standing::Root,
            peer: None,
        },
    )?;

    let invitation = Invite {
        inviter: me.handle(),
        secret,
        bearer_seed,
        locator: waypoint.to_owned(),
        grant,
    };
    Ok(Outcome::Invited {
        name: name.to_owned(),
        invite: invitation.to_string(),
        expires_at: expires_at.as_unix_seconds(),
    })
}

fn join(site: &Site, text: &str, name: &str, now: Instant) -> Result<Outcome, Complaint> {
    if site.holds(name)? {
        return Err(Complaint::ChannelExists {
            name: name.to_owned(),
        });
    }
    let invitation = Invite::parse(text)?;
    let me = signer(site)?;
    // An invitation is for somebody else. A stream is derived from the channel
    // secret and the author's handle, so an endpoint that accepted its own would
    // hold two local names for one stream, discover itself as the peer, and read
    // its own segments back as though a peer had written them. Refusing here is
    // what keeps "one channel, two parties" true.
    if invitation.inviter == me.handle() {
        return Err(Complaint::OwnInvitation);
    }
    let revoked = site.revocations()?;

    // What the invitation conveys is checked before it is used, so an expired or
    // malformed one fails here rather than after a stranger's bytes are written.
    let scope = invitation
        .grant
        .verify(&invitation.inviter, now, &revoked)?;
    let bearer = invitation.bearer();
    let mine = invitation.grant.attenuate(&bearer, &me.handle(), scope)?;

    let place = open(&invitation.locator, now)?;
    let introduction = invitation.secret.stream(&bearer.handle());
    let greeting = Segment::genesis(&me, mine.to_canonical_bytes())?;
    let (address, key) = derive(&introduction, INTRODUCTION);
    let sealed = seal(&key, &greeting.to_canonical_bytes())?;

    // The invitation is one-time because this address is write-once. Nothing
    // tracks whether it has been used; the host refuses the second greeting.
    if place.put_if_absent(&address, &sealed)? == PutOutcome::AlreadyPresent {
        return Err(Complaint::InviteSpent);
    }

    site.keep(
        name,
        &Channel {
            secret: invitation.secret.clone(),
            root: invitation.inviter,
            introduction: bearer.handle(),
            locator: invitation.locator.clone(),
            standing: Standing::Granted(mine),
            peer: Some(Peer {
                handle: invitation.inviter,
                standing: Standing::Root,
            }),
        },
    )?;

    Ok(Outcome::Joined {
        name: name.to_owned(),
        handle: me.handle().to_string(),
        peer: invitation.inviter.to_string(),
        waypoint: invitation.locator,
    })
}

fn send(site: &Site, name: &str, payload: &[u8], now: Instant) -> Result<Outcome, Complaint> {
    let me = signer(site)?;
    let channel = site.channel(name)?;
    channel.standing.permits(
        &channel.root,
        &me.handle(),
        Ability::Send,
        now,
        &site.revocations()?,
    )?;

    let place = open(&channel.locator, now)?;
    let stream = channel.secret.stream(&me.handle());
    let mine = walk(&place, &stream, &me.handle(), name)?;

    // The height comes from the waypoint, not from a file on this disk. Killing
    // this process between any two commands therefore changes nothing.
    let segment = match mine.head() {
        None => Segment::genesis(&me, payload.to_vec()),
        Some(head) => Segment::extend(&me, payload.to_vec(), head),
    }?;

    let (address, key) = derive(&stream, segment.index());
    let sealed = seal(&key, &segment.to_canonical_bytes())?;
    if place.put_if_absent(&address, &sealed)? == PutOutcome::AlreadyPresent {
        return Err(Complaint::DropTaken {
            address: address.to_string(),
            name: name.to_owned(),
        });
    }

    Ok(Outcome::Sent {
        name: name.to_owned(),
        index: segment.index(),
        id: segment.id().to_string(),
        address: address.to_string(),
    })
}

fn read(site: &Site, name: &str, after: Option<u64>, now: Instant) -> Result<Outcome, Complaint> {
    let me = signer(site)?;
    let channel = site.channel(name)?;
    let revoked = site.revocations()?;
    channel
        .standing
        .permits(&channel.root, &me.handle(), Ability::Read, now, &revoked)?;

    let place = open(&channel.locator, now)?;
    let channel = match channel.peer {
        Some(_) => channel,
        None => greet(site, name, channel, &place, now)?,
    };
    let peer = channel.peer.as_ref().ok_or_else(|| Complaint::NoPeerYet {
        name: name.to_owned(),
    })?;

    // The peer's own authority is checked before their bytes are read, so a
    // revoked peer's stream is refused rather than displayed with a warning.
    peer.standing
        .permits(&channel.root, &peer.handle, Ability::Send, now, &revoked)?;

    let stream = channel.secret.stream(&peer.handle);
    let theirs = walk(&place, &stream, &peer.handle, name)?;
    Ok(Outcome::read(
        name,
        &peer.handle.to_string(),
        &theirs,
        after,
    ))
}

/// Learns who accepted an invitation, from the introduction stream.
///
/// This is the one place a read writes: the peer it discovers is a fact that has
/// been verified against the channel's own root, and re-deriving it on every
/// command would mean paying for a request that can only ever give the same
/// answer.
fn greet(
    site: &Site,
    name: &str,
    channel: Channel,
    place: &Place,
    now: Instant,
) -> Result<Channel, Complaint> {
    let introduction = channel.secret.stream(&channel.introduction);
    let Some(greeting) = peek(place, &introduction, INTRODUCTION)? else {
        return Err(Complaint::NoPeerYet {
            name: name.to_owned(),
        });
    };

    let grant = Grant::from_canonical_bytes(greeting.payload())?;
    // Three things have to agree before a stranger becomes the peer: the grant
    // descends from this channel's root, it was issued to the handle that signed
    // the greeting, and it permits that handle to write here.
    grant.permits(
        &channel.root,
        &greeting.author(),
        Ability::Send,
        now,
        &site.revocations()?,
    )?;

    let channel = Channel {
        peer: Some(Peer {
            handle: greeting.author(),
            standing: Standing::Granted(grant),
        }),
        ..channel
    };
    site.keep(name, &channel)?;
    Ok(channel)
}

fn revoke(site: &Site, name: &str) -> Result<Outcome, Complaint> {
    let channel = site.channel(name)?;
    let peer = channel.peer.ok_or_else(|| Complaint::NoPeerYet {
        name: name.to_owned(),
    })?;
    let step = peer
        .standing
        .grant()
        .and_then(|grant| grant.steps().last())
        .ok_or_else(|| Complaint::CannotRevokeRoot {
            name: name.to_owned(),
        })?
        .id();

    site.revoke(step)?;
    Ok(Outcome::Revoked {
        name: name.to_owned(),
        step: step.to_string(),
    })
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
