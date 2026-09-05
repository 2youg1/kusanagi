// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.
// Copyright (c) 2026 2youg1 and the kusanagi contributors

//! Everything a verb takes in that is not an argument.
//!
//! A command line is public. On Linux any account on the machine reads another
//! process's arguments out of `/proc`, and the shell keeps a copy afterwards.
//! `ARCHITECTURE.md` §8 already ruled on that for the invitation; this module is
//! the same ruling applied to the rest of its kind.
//!
//! **A channel name leaks more than an invitation does.** An invitation leaks
//! one chance to enter one channel. `send --to bob` leaks who is talking to
//! whom, on every message — the relationship graph that the derived addresses of
//! `ARCHITECTURE.md` §3 properties 2a and 2b exist to hide.
//!
//! So every name argument accepts [`ON_STDIN`], and then the whole of what the
//! verb needs arrives on the pipe: the first line is the name, and the rest is
//! whatever that verb would have read from stdin anyway.

use std::io::{IsTerminal, Read};

use kusanagi::Complaint;
use kusanagi_kernel::PART_ROOM;

/// What a name argument says when the name itself arrives on stdin.
pub const ON_STDIN: &str = "-";

/// How much of stdin a name may take before it is not a name.
///
/// What a name may contain is `kusanagi_site`'s rule and is not repeated here.
/// This is how much this door buffers before deciding that what arrived is not
/// a name at all.
const NAME_ROOM: u64 = 64;

/// Room for an invitation carrying a long locator and a deep grant chain.
///
/// Derived from the widest one that can exist rather than picked: a verifying
/// key is 2 592 bytes, the channel secret and the bearer seed 32 each, and an
/// eight-hop ML-DSA-87 grant 58 345 — about 61 000 bytes, and twice that as
/// hexadecimal. 256 KiB leaves room for a long locator and still refuses to
/// buffer anything that is not an invitation.
///
/// **An invitation is no longer a line somebody pastes.** At this size it is a
/// file, which is the price of a post-quantum signature and is recorded in
/// `ARCHITECTURE.md` §8.
const INVITATION_ROOM: u64 = 262_144;

/// Reads stdin once, bounded, refusing a terminal rather than waiting at one.
///
/// The bound is the point: a door that buffers whatever arrives has handed the
/// caller a way to spend this process's memory. Reading one byte past what the
/// rule allows lets the rule that owns the limit report the failure.
fn piped(what: &'static str, instead: &'static str, most: u64) -> Result<Vec<u8>, Complaint> {
    let input = std::io::stdin();
    if input.is_terminal() {
        return Err(Complaint::Argument {
            what,
            reason: "is read from stdin, and stdin is a terminal".to_owned(),
            instead,
        });
    }
    let mut bytes = Vec::new();
    input
        .take(most)
        .read_to_end(&mut bytes)
        .map_err(|source| Complaint::Local {
            action: "read from stdin",
            source,
        })?;
    Ok(bytes)
}

/// Splits the first line off as a name, leaving the rest for the verb.
///
/// Trailing whitespace goes because a pipe is fed by a shell, a clipboard or a
/// file, and all three add a newline the caller did not type. `\r` goes with it.
fn split_name(fed: &[u8]) -> Result<(String, Vec<u8>), Complaint> {
    let mut lines = fed.splitn(2, |byte| *byte == b'\n');
    let line = lines.next().unwrap_or_default();
    let rest = lines.next().unwrap_or_default().to_vec();
    let name = String::from_utf8(line.to_vec())
        .map_err(|_| Complaint::BadName {
            name: String::from_utf8_lossy(line).into_owned(),
            reason: "the first line of stdin is not text".to_owned(),
        })?
        .trim()
        .to_owned();
    if name.is_empty() {
        return Err(Complaint::BadName {
            name,
            reason: "the first line of stdin, where the name was to be, is empty".to_owned(),
        });
    }
    Ok((name, rest))
}

/// A channel name, from the command line or from the first line of stdin.
///
/// This is the shape for the verbs that read nothing else: `read`, `revoke`,
/// `forget` and `invite`.
pub fn channel(given: String) -> Result<String, Complaint> {
    if given != ON_STDIN {
        return Ok(given);
    }
    let fed = piped(
        "the channel name",
        "pipe it in: echo NAME | kusanagi <verb> - ",
        NAME_ROOM,
    )?;
    let (name, _) = split_name(&fed)?;
    Ok(name)
}

/// A channel name and the bytes one segment will carry.
///
/// Three ways in, and only the third leaves nothing on the command line:
/// the name and the text as arguments, the name as an argument with the text
/// piped, or the name and the text piped together.
///
/// **A text argument alongside [`ON_STDIN`] is refused.** Hiding the name while
/// the message itself stays on the command line is half a fix, and half a fix
/// that reads as a whole one is worse than none.
pub fn addressed(given: String, text: Option<String>) -> Result<(String, Vec<u8>), Complaint> {
    // The ceiling of a room message in 64 parts: the largest send this door
    // takes, so the check past it happens in `divide` with the exact limit of
    // the venue rather than here. One byte past it is still read, so the door
    // knows it is past rather than guessing from a truncated pipe.
    let most = u64::from(PART_ROOM).saturating_mul(64).saturating_add(1);
    if given != ON_STDIN {
        let payload = match text {
            Some(text) => text.into_bytes(),
            None => piped(
                "the text to send",
                "pass it as an argument, or pipe it in: echo hello | kusanagi send --to NAME",
                most,
            )?,
        };
        return Ok((given, payload));
    }
    if text.is_some() {
        return Err(Complaint::Argument {
            what: "the text to send",
            reason: "was given as an argument while `--to -` says the whole send arrives on stdin"
                .to_owned(),
            instead: "pipe both: printf 'NAME\\ntext' | kusanagi send --to -",
        });
    }
    let fed = piped(
        "the channel name and the text to send",
        "pipe both: printf 'NAME\\ntext' | kusanagi send --to -",
        most.saturating_add(NAME_ROOM),
    )?;
    split_name(&fed)
}

/// How much of stdin a roster may take before it is not a roster.
///
/// A name is at most 32 characters and a newline, so this is room for about a
/// hundred and twenty members — far past the size at which fanning out one drop
/// per member is the right shape at all. `ARCHITECTURE.md` §8 sends a thousand
/// people to a group key scheme, not to this.
const ROSTER_ROOM: u64 = 4_096;

/// A group name and the channels it stands for, both from stdin.
///
/// The members arrive on the pipe for the same reason every other name does: a
/// roster **is** the relationship graph — one command line would name everybody
/// this endpoint talks to at once, to every account on the machine and to the
/// shell's history file.
///
/// # Errors
///
/// [`Complaint::Argument`] when stdin is a terminal, and [`Complaint::BadName`]
/// when the first line is missing while the group name was to come from there.
pub fn enrolled(given: String) -> Result<(String, Vec<String>), Complaint> {
    let fed = piped(
        "the group name and its members",
        "pipe the members in, one per line: printf 'alice\\nbob' | kusanagi group --name NAME",
        ROSTER_ROOM,
    )?;
    let (name, rest) = if given == ON_STDIN {
        split_name(&fed)?
    } else {
        (given, fed)
    };
    let members = String::from_utf8(rest)
        .map_err(|_| Complaint::BadName {
            name: name.clone(),
            reason: "the member list is not text".to_owned(),
        })?
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    Ok((name, members))
}

/// How large an archive this door will read.
///
/// A site of a hundred channels is a few hundred kilobytes: a channel record
/// carries a 2 592-byte key, and a cairn is 105. Eight megabytes is two
/// thousand channels, which is more than a person has and still refuses to
/// buffer a file somebody handed over by mistake.
const ARCHIVE_ROOM: u64 = 8 * 1_048_576;

/// The recovery key and the archive it opens, both from stdin.
///
/// **The key is never an argument**, for the reason every other secret here is
/// not one: a command line is public while the process runs and is written to a
/// history file afterwards. The first line is the key in hexadecimal, and the
/// rest of stdin is the archive.
///
/// # Errors
///
/// [`Complaint::Argument`] when stdin is a terminal or holds no first line, and
/// [`Complaint::BadRecovery`] when the first line is not 64 hexadecimal digits
/// — the same refusal a key of the right shape that opens nothing gets, because
/// the caller's next step is the same: check the key.
pub fn restored() -> Result<([u8; 32], Vec<u8>), Complaint> {
    let fed = piped(
        "the recovery key and the archive",
        "pipe both: cat key.txt backup.ksnb | kusanagi import --root NEW",
        ARCHIVE_ROOM.saturating_add(NAME_ROOM),
    )?;
    let (line, archive) = split_name(&fed)?;
    let bytes = kusanagi_kernel::unhex(line.trim()).map_err(|_| Complaint::BadRecovery)?;
    let key = <[u8; 32]>::try_from(bytes.as_slice()).map_err(|_| Complaint::BadRecovery)?;
    Ok((key, archive))
}

/// A channel name and the invitation that opens it.
///
/// The invitation has arrived only on stdin since `ARCHITECTURE.md` §8 ruled it
/// out of the command line. With [`ON_STDIN`] the name joins it there, and the
/// first line is read as the name exactly as it is for `send`.
pub fn invited(given: String) -> Result<(String, String), Complaint> {
    let fed = piped(
        "the invitation",
        "pipe it in: kusanagi join --name NAME < invitation.txt",
        INVITATION_ROOM,
    )?;
    let (name, rest) = if given == ON_STDIN {
        split_name(&fed)?
    } else {
        (given, fed)
    };
    let invite = String::from_utf8(rest).map_err(|_| Complaint::BadInvitation {
        reason: "what arrived on stdin is not text".to_owned(),
    })?;
    Ok((name, invite))
}
