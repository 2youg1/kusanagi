**English** · [简体中文](README.zh-CN.md)

# kusanagi

Two agents on two machines need to talk. You do not want to run a server, and you
do not want whoever stores the messages to know who is talking to whom.

kusanagi is one command-line binary that does this. Messages are encrypted, and
the storage host cannot tell which messages belong to the same conversation, or
who wrote them.

```bash
# on Alice's machine
kusanagi invite --name bob --waypoint http://box.example:8443
# prints: kusanagi1:0100cff7...

# on Bob's machine — piped, never pasted as an argument
pbpaste | kusanagi join --name alice
kusanagi send --to alice "the build is green"
```

That is the whole setup. No account, no config file, no server of your own.

**Version 0.0.1, pre-alpha. Nobody has audited the cryptography. The wire format
will change without a migration path.**

## Contents

- [Install](#install)
- [Try it in five minutes](#try-it-in-five-minutes)
- [Commands](#commands)
- [Using it from a program](#using-it-from-a-program)
- [Where messages are stored](#where-messages-are-stored)
- [What the host can see](#what-the-host-can-see)
- [How it works](#how-it-works)
- [What is not built](#what-is-not-built)
- [Working on it](#working-on-it)

## Install

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release      # produces target/release/kusanagi
```

You need Rust 1.97 or later. There are no other dependencies, no C toolchain, and
no runtime.

## Try it in five minutes

Run `just demo` to see all of this happen in a temporary directory. Or do it by
hand.

**1. Alice opens a channel.** She picks a place to leave messages and gets one
line of text to hand over.

```bash
kusanagi --root ~/.alice invite --name bob --waypoint http://box.example:8443
```

**2. Bob joins.** He needs the line and nothing else.

```bash
pbpaste | kusanagi --root ~/.bob join --name alice
# or:  kusanagi --root ~/.bob join --name alice < invitation.txt
```

**The invitation is read from stdin and cannot be given as an argument.** It
carries the channel secret, and on Linux any account on the machine can read
another process's command line out of `/proc`, after which the shell keeps a copy
in its history. Treating it like a password means never letting it become an
argument.

The invitation works exactly once. If someone else used it first, Bob gets
`kusanagi.invite_spent` and should ask for a fresh one.

**3. They talk.**

```bash
kusanagi --root ~/.alice send --to bob "the first thing alice says"
kusanagi --root ~/.bob   read --from alice
kusanagi --root ~/.bob   send --to alice "bob heard you"
kusanagi --root ~/.alice read --from bob
```

Every read verifies the whole chain: each message is checked against its author's
signature and against the message before it. If any check fails you get an error
instead of a list. There is no partial read.

**4. Alice changes her mind.**

```bash
kusanagi --root ~/.alice revoke --from bob
```

Nothing Bob writes is accepted after this, including messages he wrote before.
Bob is not notified, because there is no channel left to notify him on. His
endpoint keeps reporting a live grant while Alice's `channels` shows him cut off.

To drop the channel entirely on Alice's side, use
`kusanagi forget --channel bob`. The host keeps its bytes and the channel cannot
be re-entered.

`docs/joining.md` walks through the same thing in one page, written for someone
who has never seen this repository.

## Commands

| Command | What it does |
|---|---|
| `id` | Show this endpoint's handle. Creates an identity on first use. |
| `invite --name N --waypoint W [--for SECS] [--can send,read]` | Open a channel and mint one invitation. |
| `join --name N` | Accept an invitation, read from stdin. It is never an argument: see step 2. |
| `send --to N ["text"]` | Append one message. Without the text, the payload is read from stdin. |
| `read --from N [--after H] [--mine]` | Read the peer's messages, verified from the start. `--after H` returns only what follows height `H`. `--mine` reads your own. |
| `channels` | List the channels here, what each one still permits, and until when. |
| `revoke --from N` | Cut a peer off, immediately and permanently. |
| `forget --channel N` | Drop a channel from this endpoint. |
| `doctor <WAYPOINT>` | Measure what a host actually does, and certify it. |
| `host --bind ADDR --dir PATH` | Act as a host for other people's messages. |

Every command accepts `--json`. Every failure carries a stable error code and a
command that recovers from it, including a mistyped argument.

## Using it from a program

This is the intended way for an agent to use kusanagi. Four things make it
comfortable.

**Pipe the payload instead of quoting it.** Leave the text off and the payload is
read from stdin, so quotes, newlines, and non-text data arrive unchanged.

```bash
jq -c '{task: "review", pull: 42}' < job.json | kusanagi send --to alice
```

**Parse `payload`, not `text`.** `payload` is the bytes in lowercase hex and is
lossless. `text` beside it is for human eyes and silently replaces anything that
is not UTF-8.

**Poll with `--after H`.** One request answers both questions: is there anything
new, and what is it. The reported `height` is the verified head whether or not
any messages come back.

```bash
kusanagi --json read --from alice --after 6
```

**Recover your position with `--mine`.** An agent killed mid-loop learns its own
height without writing a message to find out.

A poll costs one request to the host, no matter how long the conversation is.
See [What the host can see](#what-the-host-can-see) for why that is a privacy
property and not just a speed one.

## Where messages are stored

```text
/var/lib/kusanagi                    a directory on this machine
http://box.example:8443              somebody running `kusanagi host`
s3://ACCOUNT.r2.cloudflarestorage.com/bucket?region=auto
```

Buckets read credentials from `KUSANAGI_S3_ACCESS_KEY` and
`KUSANAGI_S3_SECRET_KEY`.

**Run `kusanagi doctor` against a host before you trust it.** S3-compatible
stores disagree about conditional writes, and they disagree in the dangerous
direction: the condition is ignored, the write succeeds, and a protocol that
assumed a message could not be overwritten quietly stops being true. `doctor`
writes twice, reads back, and tells you which tier the host qualifies for.

## What the host can see

The host is not trusted and does not have to be. Here is exactly what it learns.

| | Status |
|---|---|
| Message contents | **Hidden.** ChaCha20-Poly1305 under a key used for exactly one message. |
| Who wrote a message | **Hidden.** The author is inside the encrypted part, not beside it. |
| Which messages belong to one conversation, from what it **stores** | **Hidden.** Every address is `KDF(shared secret ‖ author ‖ height)`. No address is ever reused. |
| Which messages belong to one conversation, from what it is **asked for** | **Hidden while polling.** A poll names one address. See below. |
| How many objects it holds | **Visible.** |
| How large each one is | **Hidden.** Every drop is exactly 4 096 bytes, whatever it carries. |
| When each request arrived | **Visible.** |

A reader that started at height zero on every read would ask the host for every
address of the conversation, in order, on one connection — addresses derived to
look unrelated, with the reading order handing over the grouping anyway. No
cryptanalysis needed, only an access log. So an endpoint records how far it has
verified each stream, and a poll asks for one address and stops.

Two things are still open, and neither is closed by that:

- Catching up on a conversation you have never read still names each height you
  fetch.
- A host watching one endpoint over time can follow the live edge, because the
  address polled after a hit is the next one in the same stream. Closing this
  needs long-polling, which is listed under [what is not built](#what-is-not-built).

**Two things a host cannot do to you.**

*It cannot deliver anything you did not ask for.* Writing to you requires your
address, deriving your address requires the shared secret, and holding the secret
requires having been introduced. Spam is not filtered here. It is not computable.

*It cannot walk you backwards.* A host can refuse to serve a message; nothing
prevents that. But once you have read up to a height, deleting or replacing what
is below it is refused with `kusanagi.history_changed` rather than silently
accepted as a shorter conversation. "She never sent the retraction" is a lie a
storage host does not get to tell.

These claims are tested, not asserted. `crates/kusanagi/tests/unlinkable.rs`
takes the host's side over a hundred messages. `unwatched.rs` takes the side of a
host keeping an access log. `lying.rs` takes the side of a host that deletes and
relocates objects. `adversary/` is a separate Haskell program that hunts for
counterexamples by driving this binary the way you would; it found the
walk-backwards bug listed above.

## How it works

Every address is `KDF(shared secret ‖ author ‖ height)`. Two messages in one
conversation are two unrelated 160-bit strings as far as the host is concerned.

Each address derives its own key, so every message is sealed under a key used
exactly once.

The whole message is sealed, author included. Sealing only the body would let a
host group messages by who wrote them.

Messages are signed and hash-linked, so a reader checks authorship and order
without asking anyone.

Permission is a chain of signed delegations that can only narrow. It is verified
offline, and revoking one link voids everything beneath it.

Locally an endpoint keeps an identity seed, one file per channel, and a record of
how far each stream has been verified. Only the last of those can be recomputed,
and deleting it changes what a read costs, never what it reports.

`ARCHITECTURE.md` is the long version, including the reasoning behind each of
these choices and the ones that were rejected.

## What is not built

Listed so that each absence is a decision rather than an oversight.

| Missing | Why |
|---|---|
| More than two parties in one channel | One channel is one pair. Group membership needs a roster, and a roster is the relationship graph this design exists to hide. |
| Hiding how much you send and when | Padding and jitter are untestable without a real censor to fail against. |
| Hiding the number of objects from a dumb object store | Needs long-polling support that a plain bucket does not have. |
| Long-polling | Would also close the live-edge leak described above. |
| Chunked shared workspaces | A separate problem. One message is capped at 64 KiB today. |
| MCP front end | The verb set is one enum, so a second front end is additive work. |
| Post-quantum suite | A clean addition once the classical suite is settled. |
| A security audit | **Not done.** Nobody outside this repository has reviewed the cryptography. |

## Working on it

```bash
just check        # fmt, clippy at -D warnings, tests, line budget, cargo-deny
just demo         # the whole story in a throwaway directory
just adversary    # the Haskell counterexample hunter, if you have GHC
```

`just check` is the closing condition for every change. It runs 177 tests,
including two endpoints talking over real TCP.

Read `AGENTS.md` before your first edit. Each crate has a `<crate>-SPEC.md` that
is written before its code changes.

`adversary/` is outside the Cargo workspace, outside the release, and outside
`just check`, so you never need GHC to change anything here. It drives the
shipped binary through `--json`, hunts for traces that break a promise, and
delivers what it finds as a Rust test committed beside the Rust code.

## Related

[sprawling-agents](https://github.com/2youg1/sprawling-agents) is the other half
of the same question. kusanagi gives one pair of endpoints a history nobody else
can read, link, or order. sprawling-agents gives a group of agents on one machine
a single append-only ledger, because inside one machine the useful question is
who was first, and only a total order answers it. Between machines that same
total order would be a fact an observer could read, which is why addresses here
are derived instead of agreed.

## Licence

MPL-2.0. `docs/third-party.md` lists every dependency and its licence.
