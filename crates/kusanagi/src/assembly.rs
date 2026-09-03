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
use std::path::PathBuf;

use kusanagi_box::Server;
use kusanagi_kernel::{Clock as _, Instant, Signer};
use kusanagi_seal::Secret;
use kusanagi_waypoint::{Access, Locator, Place, Proxy, probe};

use kusanagi_site::Site;

use crate::membership::{forget, group, invite, join, revoke};
use crate::request::Request;
use crate::traffic::{fanout, read, send};
use crate::world::{SystemClock, fresh_seed};
use kusanagi_door::Complaint;
use kusanagi_door::Grouping;
use kusanagi_door::Outcome;
use zeroize::Zeroize as _;

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
        Request::Group { name, members } => group(site, name, members),
        Request::Fanout { group, payload } => fanout(site, group, payload, now),
        Request::Read { name, after, whose } => read(site, name, *after, *whose, now),
        Request::Revoke { name } => revoke(site, name),
        Request::Forget { name } => forget(site, name),
        Request::Doctor { waypoint } => doctor(waypoint, now),
        Request::Here => here(site),
        Request::Host {
            bind,
            directory,
            capacity,
        } => host(bind, directory, *capacity),
        Request::Export => export(site),
        Request::Import { recovery, archive } => import(site, recovery, archive),
    }
}

/// Seals everything this endpoint holds under a key drawn here and shown once.
///
/// **The recovery key is generated rather than chosen.** A passphrase somebody
/// invents is a passphrase somebody guesses, and there is no rate limit on a
/// file. Thirty-two bytes from the operating system, in hexadecimal, handed back
/// exactly once: whoever runs this writes it down or loses the archive.
fn export(site: &Site) -> Result<Outcome, Complaint> {
    let mut recovery = fresh_seed()?;
    let mut nonce = [0_u8; 12];
    nonce.copy_from_slice(fresh_seed()?.get(..12).unwrap_or(&[0; 12]));
    let archive = kusanagi_site::export(site, &recovery, nonce)?;
    let key = kusanagi_kernel::Hex(&recovery).to_string();
    recovery.zeroize();
    Ok(Outcome::Exported {
        recovery: key,
        archive,
    })
}

/// Puts an archive back into a root that has nothing in it.
fn import(site: &Site, recovery: &[u8; 32], archive: &[u8]) -> Result<Outcome, Complaint> {
    kusanagi_site::import(site, recovery, archive)?;
    let names = site.names()?;
    Ok(Outcome::Imported {
        site: site.root().display().to_string(),
        channels: names.len(),
    })
}

/// Where an endpoint keeps its site when nobody names a directory.
///
/// **A relative path was the wrong default.** It put the identity, the channel
/// keys and every cairn in whatever directory the program was started from —
/// which for an agent is the repository it is working in, a folder a sync client
/// uploads, or a directory somebody shares. The profile directory is where the
/// operating system already keeps per-user state, and on Windows it is also the
/// cheapest half of the file permissions this design owes: what `%LOCALAPPDATA%`
/// inherits admits the owner, `SYSTEM` and administrators and nobody else, so a
/// second standard account on the machine cannot read a site today.
///
/// One environment variable per platform, read here because this module is the
/// only one allowed to read any. The branch that is not this machine's is
/// compiled by CI on a machine that is, and asserted by nothing here — a claim
/// about a platform nobody ran is not evidence.
///
/// # Errors
///
/// [`Complaint::NoRoot`] when the variable that names the profile is absent,
/// which is the one case where this program cannot guess and must be told.
#[cfg(windows)]
pub fn default_root() -> Result<PathBuf, Complaint> {
    beneath("LOCALAPPDATA", "kusanagi")
}

/// Where an endpoint keeps its site when nobody names a directory.
///
/// # Errors
///
/// [`Complaint::NoRoot`] when `HOME` is absent.
#[cfg(target_os = "macos")]
pub fn default_root() -> Result<PathBuf, Complaint> {
    beneath("HOME", "Library/Application Support/kusanagi")
}

/// Where an endpoint keeps its site when nobody names a directory.
///
/// # Errors
///
/// [`Complaint::NoRoot`] when neither `XDG_DATA_HOME` nor `HOME` is set.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn default_root() -> Result<PathBuf, Complaint> {
    beneath("XDG_DATA_HOME", "kusanagi").or_else(|_| beneath("HOME", ".local/share/kusanagi"))
}

/// `$<variable>/<tail>`, or a complaint naming the variable that was missing.
fn beneath(variable: &'static str, tail: &str) -> Result<PathBuf, Complaint> {
    let base = std::env::var(variable).map_err(|_| Complaint::NoRoot { variable })?;
    if base.trim().is_empty() {
        return Err(Complaint::NoRoot { variable });
    }
    Ok(PathBuf::from(base).join(tail))
}

/// Where `kusanagi host` keeps other people's drops when nobody says.
///
/// Beside the site rather than inside it: what a host holds is not this
/// endpoint's state, and a `forget` or a backup must not sweep up somebody
/// else's bytes.
///
/// # Errors
///
/// Whatever [`default_root`] reports.
pub fn default_host_dir() -> Result<PathBuf, Complaint> {
    let root = default_root()?;
    let mut named = root.clone().into_os_string();
    named.push("-host");
    Ok(PathBuf::from(named))
}

/// This endpoint's signer, created on first use.
///
/// An identity appears the first time one is needed rather than through a setup
/// step, because a setup step is a thing to forget and this one has no decisions
/// in it.
pub(crate) fn signer(site: &Site) -> Result<Signer, Complaint> {
    if let Some(signer) = site.identity()? {
        return Ok(signer);
    }
    // The seed is bound so that it can be erased. Handing `fresh_seed()?`
    // straight to a call leaves the bytes in a temporary that lives to the end
    // of the statement and is never overwritten, which is how an identity seed
    // ends up in a core dump of a process that had finished with it.
    let mut seed = fresh_seed()?;
    let adopted = site.adopt(&seed);
    seed.zeroize();
    Ok(adopted?)
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
    let groups = site
        .groups()?
        .into_iter()
        .map(|roster| Grouping {
            name: roster.name,
            members: roster.members,
        })
        .collect();
    Ok(Outcome::Channels { channels, groups })
}

/// Opens what a locator names, with what the environment supplies to reach it.
///
/// The environment is read here and nowhere else, for the same reason the clock
/// is: a program that picks up configuration in the middle of a call graph is a
/// program whose behaviour cannot be reproduced from its arguments.
///
/// `KUSANAGI_PROXY` sends every request through a SOCKS5 or HTTP CONNECT proxy.
/// kusanagi does not hide an endpoint's IP address and does not claim to
/// (`ARCHITECTURE.md` §3); this is the plug for a network that does, and a
/// mistyped one is refused here rather than silently ignored — a privacy setting
/// that fails open is worse than one that was never offered.
pub(crate) fn open(locator: &str, now: Instant) -> Result<Place, Complaint> {
    let locator: Locator = locator.parse()?;
    let credentials = match (
        std::env::var("KUSANAGI_S3_ACCESS_KEY"),
        std::env::var("KUSANAGI_S3_SECRET_KEY"),
    ) {
        (Ok(access), Ok(secret)) => Some(kusanagi_waypoint::Credentials::new(&access, &secret)),
        _ => None,
    };
    let proxy = match std::env::var("KUSANAGI_PROXY") {
        Ok(text) if !text.trim().is_empty() => Some(Proxy::parse(text.trim())?),
        _ => None,
    };
    let access = Access {
        credentials,
        proxy,
        ..Access::default()
    };
    Ok(Place::open(&locator, &access, now.as_unix_seconds())?)
}

fn doctor(waypoint: &str, now: Instant) -> Result<Outcome, Complaint> {
    let place = open(waypoint, now)?;
    // A fresh secret every run: the probe writes to real addresses on a real
    // host, and addresses nobody can predict are addresses nothing collides with.
    let mut channel = fresh_seed()?;
    let mut author = fresh_seed()?;
    let namespace = Secret::from_bytes(channel).stream(&Signer::from_seed(&author).handle());
    channel.zeroize();
    author.zeroize();
    let certificate = probe::examine(&place, &namespace);
    Ok(Outcome::examined(waypoint, place.kind(), &certificate))
}

/// Where `kusanagi host` listens when nobody says otherwise.
///
/// Loopback, so that starting a host is never by itself a decision to put one on
/// the network. The port is inside the block IANA lists as unassigned
/// (`8955-8979`) and below the dynamic range that begins at 49152, so it is
/// neither a port some other product was given nor one the operating system will
/// hand to an outgoing connection. The value it replaced, 8443, is registered as
/// `pcsync-https` and is in practice held by Tomcat, ingress controllers and
/// development proxies — which made the first command in the documentation fail
/// on a machine that had done nothing wrong.
pub const HOST_ADDRESS: &str = "127.0.0.1:8963";

/// Turns what was typed after `--bind` into an address to listen on.
///
/// A bare number is a port on loopback, so `--bind 9000` is enough and
/// `--bind 0` asks the operating system for any free one. Anything else goes to
/// the operating system as written.
///
/// **The completion is `127.0.0.1` rather than `0.0.0.0`.** Saving five
/// characters must not be the same keystroke as putting a host on the local
/// network; listening beyond this machine stays a sentence somebody writes out.
fn listening(bind: &str) -> String {
    if !bind.is_empty() && bind.bytes().all(|byte| byte.is_ascii_digit()) {
        return format!("127.0.0.1:{bind}");
    }
    bind.to_owned()
}

/// Reports what this machine is doing with what this endpoint holds.
///
/// Every question here is answerable without reaching anything: the point of
/// `doctor --here` is that an operator who cannot get to a host can still find
/// out whether their own side is set up the way the documentation says.
fn here(site: &Site) -> Result<Outcome, Complaint> {
    let root = site.root();
    Ok(Outcome::Here {
        site: root.display().to_string(),
        under_profile: under_profile(root),
        at_rest: kusanagi_site::store(),
        // Whether, never what. A proxy address says which network somebody
        // trusts, which is the kind of fact this report exists to protect.
        proxy: std::env::var("KUSANAGI_PROXY").is_ok_and(|set| !set.trim().is_empty()),
        binary: binary_hash()?,
    })
}

/// Whether the site sits under this user's profile directory.
///
/// `None` where the question has no meaning, which is every platform whose
/// default root is not chosen for the access control list it inherits.
#[cfg(windows)]
fn under_profile(root: &std::path::Path) -> Option<bool> {
    let profile = std::env::var("LOCALAPPDATA").ok()?;
    let here = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let profile = std::path::Path::new(&profile);
    let profile = profile
        .canonicalize()
        .unwrap_or_else(|_| profile.to_owned());
    Some(here.starts_with(profile))
}

#[cfg(not(windows))]
const fn under_profile(_root: &std::path::Path) -> Option<bool> {
    None
}

/// The BLAKE3 of the file this process was started from.
///
/// BLAKE3 rather than SHA-256 because this workspace already hashes with it
/// everywhere else, and a verification step that needs no second tool is worth
/// more than matching what `sha256sum` happens to print.
fn binary_hash() -> Result<String, Complaint> {
    let path = std::env::current_exe().map_err(|source| Complaint::Local {
        action: "find the running binary",
        source,
    })?;
    let bytes = std::fs::read(&path).map_err(|source| Complaint::Local {
        action: "read the running binary",
        source,
    })?;
    Ok(kusanagi_kernel::Hex(blake3::hash(&bytes).as_bytes()).to_string())
}

fn host(bind: &str, directory: &std::path::Path, capacity: u64) -> Result<Outcome, Complaint> {
    let wanted = listening(bind);
    let listener = TcpListener::bind(&wanted).map_err(|source| Complaint::Listening {
        address: wanted.clone(),
        source,
    })?;
    let address = listener
        .local_addr()
        .map_or(wanted, |addr| addr.to_string());

    // Progress goes to stderr so that stdout carries only the result. A host runs
    // until it is killed, so the result arrives long after this line.
    eprintln!(
        "kusanagi host: serving {} on {address}",
        directory.display()
    );

    Server::new(directory, SystemClock)
        .holding(capacity)
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
