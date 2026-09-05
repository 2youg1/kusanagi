// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! How one command line becomes one [`Request`].
//!
//! `verbs.rs` is the shape clap parses; this is the reading of that shape into
//! the one enum every door shares. Apart so that the shape stays a declaration
//! and every check a flag needs — which side of a pair was given, whether a
//! number fits — is a function here with a name.

use core::num::NonZeroU32;

use kusanagi::{Cadence, Complaint, Habit, Naming, Request, Retention, Whose};
use kusanagi_grant::{Abilities, Ability};

use crate::intake;
use crate::verbs::Verb;

/// Reads `send,read` into a set of abilities.
///
/// An unknown word is refused rather than ignored: an invitation that silently
/// granted less than it was asked for would be discovered by the person it was
/// given to, days later, as a failure they cannot explain.
/// Turns the two flags a channel is opened with into the value they mean.
///
/// Absent is the default in both cases, and the default is the one that promises
/// nothing: write when asked, keep everything.
fn habit(every: Option<NonZeroU32>, release: bool) -> Habit {
    Habit {
        cadence: every.map_or(Cadence::OnDemand, |period| Cadence::Slotted { period }),
        retention: if release {
            Retention::ReleaseOnAck
        } else {
            Retention::Keep
        },
    }
}

fn abilities(text: &str) -> Result<Abilities, Complaint> {
    let mut abilities = Abilities::NONE;
    for word in text
        .split(',')
        .map(str::trim)
        .filter(|word| !word.is_empty())
    {
        match word {
            "send" => abilities = abilities.with(Ability::Send),
            "read" => abilities = abilities.with(Ability::Read),
            other => {
                return Err(Complaint::Argument {
                    what: "--can",
                    reason: format!("does not know the ability `{other}`"),
                    instead: "pass a comma-separated list of send and read",
                });
            }
        }
    }
    Ok(abilities)
}

/// Two flags into one answer: `--require` records, `--optional` lifts, neither reads.
const fn stance(require: bool, optional: bool) -> Option<bool> {
    match (require, optional) {
        (true, _) => Some(true),
        (_, true) => Some(false),
        _ => None,
    }
}

/// A sweep width a ward can have, or the argument that asked for more.
fn width(digits: Option<u8>) -> Result<Option<u8>, Complaint> {
    match digits {
        Some(more) if more > kusanagi_site::MOST_DIGITS => Err(Complaint::Argument {
            what: "--digits",
            reason: format!("a ward has four hex digits, not {more}"),
            instead: "pass a number from 0 (every ward on the host) to 4 (your ward alone)",
        }),
        other => Ok(other),
    }
}

/// The floor and ceiling a sweep cap a person asks for never leaves.
///
/// Narrower than `kusanagi_site`'s own record, and the record is where the
/// refusal to go wider happens. `clap` cannot compare a candidate to the width
/// the caller did *not* give in one option, so out-of-band values clamp rather
/// than refuse: the answer still reports what the disk actually says.
const fn cap_floor() -> u16 {
    32
}

const fn cap_ceiling() -> u16 {
    4096
}

/// `--as` sets, `--clear` clears, neither asks; `-` after `--as` reads stdin.
fn naming(alias: Option<String>, clear: bool) -> Result<Naming, Complaint> {
    Ok(match alias {
        Some(alias) => Naming::Set(intake::channel(alias)?),
        None if clear => Naming::Clear,
        None => Naming::Ask,
    })
}

/// Two destinations, and the command line carries at most one of them.
///
/// clap refuses both at once; the case it cannot express is neither, and saying
/// so here gives that a stable code and a way out.
fn sending(
    name: Option<String>,
    group: Option<String>,
    text: Option<String>,
) -> Result<Request, Complaint> {
    match (name, group) {
        (Some(name), _) => {
            let (name, payload) = intake::addressed(name, text)?;
            Ok(Request::Send { name, payload })
        }
        (None, Some(group)) => {
            let (group, payload) = intake::addressed(group, text)?;
            Ok(Request::Fanout { group, payload })
        }
        (None, None) => Err(Complaint::Argument {
            what: "send",
            reason: "was given nobody to send to".to_owned(),
            instead: "pass --to NAME for one channel, or --to-group NAME for a group",
        }),
    }
}

/// Reads a room verb into the request it names.
///
/// One `HANDLE=HEIGHT` floor of a room read.
fn floored(floor: &str) -> Result<(String, u64), Complaint> {
    floor
        .split_once('=')
        .and_then(|(handle, height)| {
            height
                .trim()
                .parse()
                .ok()
                .map(|height| (handle.trim().to_owned(), height))
        })
        .ok_or_else(|| Complaint::Argument {
            what: "--after",
            reason: format!("`{floor}` is not HANDLE=HEIGHT"),
            instead: "write the author's handle, `=`, then the height you hold",
        })
}

/// Apart from `request` because that dispatch is at its line limit: five more
/// arms would push it past what one function may hold.
fn room_request(verb: Verb) -> Result<Request, Complaint> {
    Ok(match verb {
        Verb::Room { name, waypoint } => Request::Room {
            name: intake::channel(name)?,
            waypoint,
        },
        Verb::RoomInvite { name, lifetime } => Request::RoomInvite {
            name: intake::channel(name)?,
            lifetime,
        },
        Verb::RoomJoin { name } => {
            let (name, invite) = intake::invited(name)?;
            Request::RoomJoin { invite, name }
        }
        Verb::RoomSend { name, text } => {
            let (name, payload) = intake::addressed(name, text)?;
            Request::RoomSend { name, payload }
        }
        Verb::RoomRead { name, after } => {
            // `--after -` reads the floors from stdin, one per line after the
            // name: thirty-two of them do not belong on a command line.
            let (name, floors) = if after.as_slice() == [intake::ON_STDIN] {
                intake::enrolled(name)?
            } else {
                (intake::channel(name)?, after)
            };
            Request::RoomRead {
                name,
                after: floors
                    .iter()
                    .map(|floor| floored(floor))
                    .collect::<Result<_, _>>()?,
            }
        }
        _ => {
            return Err(Complaint::Argument {
                what: "verb",
                reason: "is not a room verb".to_owned(),
                instead: "this is a bug: report it",
            });
        }
    })
}

pub(crate) fn request(verb: Verb) -> Result<Request, Complaint> {
    Ok(match verb {
        Verb::Id => Request::Identity,
        Verb::Channels => Request::Channels,
        Verb::Invite {
            name,
            waypoint,
            lifetime,
            can,
            every,
            release,
        } => Request::Invite {
            name: intake::channel(name)?,
            waypoint,
            lifetime,
            abilities: abilities(&can)?,
            habit: habit(every, release),
        },
        Verb::Join {
            name,
            every,
            release,
        } => {
            let (name, invite) = intake::invited(name)?;
            Request::Join {
                invite,
                name,
                habit: habit(every, release),
            }
        }
        Verb::Tick { name } => Request::Tick {
            name: intake::channel(name)?,
        },
        Verb::Send { name, group, text } => sending(name, group, text)?,
        Verb::Doctor { waypoint, here } => match waypoint {
            Some(waypoint) => Request::Doctor { waypoint },
            // `required_unless_present` has already refused the empty case, so
            // reaching here means `--here` was given.
            None if here => Request::Here,
            None => {
                return Err(Complaint::Argument {
                    what: "doctor",
                    reason: "was given nothing to measure".to_owned(),
                    instead: "pass a waypoint to measure a host, or --here for this machine",
                });
            }
        },
        Verb::Group { name } => {
            let (name, members) = intake::enrolled(name)?;
            Request::Group { name, members }
        }
        Verb::Room { .. }
        | Verb::RoomInvite { .. }
        | Verb::RoomJoin { .. }
        | Verb::RoomSend { .. }
        | Verb::RoomRead { .. } => room_request(verb)?,
        Verb::Read { name, after, mine } => Request::Read {
            name: intake::channel(name)?,
            after,
            // The flag is a flag because that is what a command line has; the
            // enum starts here so that nothing below carries an unnamed bool.
            whose: if mine { Whose::Mine } else { Whose::Peer },
        },
        Verb::Proxy { require, optional } => Request::Proxy {
            require: stance(require, optional),
        },
        Verb::Sweep { digits, cap } => Request::Sweep {
            digits: width(digits)?,
            cap: cap.map(|cap| usize::from(cap.clamp(cap_floor(), cap_ceiling()))),
        },
        Verb::Name { alias, clear } => Request::Name {
            naming: naming(alias, clear)?,
        },
        Verb::Revoke { name } => Request::Revoke {
            name: intake::channel(name)?,
        },
        Verb::Forget { name } => Request::Forget {
            name: intake::channel(name)?,
        },
        Verb::Port => Request::Port,
        Verb::Export => Request::Export,
        Verb::Import => {
            let (recovery, archive) = intake::restored()?;
            Request::Import { recovery, archive }
        }
        Verb::Host {
            bind,
            directory,
            capacity,
        } => Request::Host {
            bind,
            directory: match directory {
                Some(named) => named,
                None => kusanagi::default_host_dir()?,
            },
            capacity,
        },
    })
}
