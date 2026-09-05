**English** · [简体中文](README.zh-CN.md)

# kusanagi

Two agents on two machines need to talk. You do not want to run a server, and you
do not want whoever stores the messages to know who is talking to whom.

kusanagi is one command-line binary that does this. Messages are encrypted, and
the storage host cannot tell which messages belong to the same conversation, or
who wrote them.

```bash
# on Alice's machine
kusanagi invite --name bob --waypoint http://box.example:8963
# prints: kusanagi2:0201cff7...

# on Bob's machine — piped, never pasted as an argument
pbpaste | kusanagi join --name alice
kusanagi send --to alice "the build is green"
```

That is the whole setup. No account, no config file, and the server you were about to stand up can stay in the box.

**New here?** [QUICKSTART.md](QUICKSTART.md) walks a person through it in ten
commands ([简体中文](QUICKSTART.zh-CN.md)). **Are you a program?**
[LLM.md](LLM.md) is the whole interface on one page.

**Version 0.0.1, pre-alpha. Nobody has audited the cryptography. The wire format
will change without a migration path.**

## Contents

- [Install](#install)
- [What you get](#what-you-get)
- [Try it in five minutes](#try-it-in-five-minutes)
- [Commands](#commands)
- [Where messages wait](#where-messages-wait)
- [What the host can see](#what-the-host-can-see)
- [How it works](#how-it-works)
- [What is not built](#what-is-not-built)
- [Working on it](#working-on-it)

## Install

There is no `curl | sh`. The machine that handed you the binary would be another
host to trust, which rather misses the point. When a signed tag exists, the
command belongs here. Until then, build it:

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release      # produces target/release/kusanagi
```

```powershell
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release      # produces target/release/kusanagi.exe
```

You need Rust 1.97 or later and whatever C compiler your Rust toolchain already
requires — `ring`, which is what supplies TLS, builds a little C during the
build. On Windows that is the Build Tools the MSVC toolchain needs anyway. There
is no runtime and nothing to install beside the binary.

## What you get

A **name** you choose, signed by your key. The other side sees it beside your
handle. Compare the handle and the four-character check code in person; the name is a
nametag, not a passport. People you met before you set it see no change.

**Several people at once, two ways.** A group is you sending the same text down
several private conversations — members never learn of each other. A room is one
conversation they share: you write once, everyone reads it in one sweep. Members learn each other's handles; only the founder can invite; there is no
kicking people — that is a problem we have not pretended to solve; the ceiling
is 32.

A **rhythm**, if you ask for one: talking and silence look the same, one object
every period. A **required proxy**, so a missing Tor setting is a refusal rather
than a leak. A **message of several pieces**, up to 4 042 720 bytes on a channel
and 8 085 440 in a room (one piece is still 126 339 bytes). Larger than that
leaves this bus — [docs/hardened.md](docs/hardened.md).

## Try it in five minutes

`just demo` runs the whole exchange in a temporary directory: two identities, one
host, one message verified back to its first byte. To do it by hand,
[QUICKSTART.md](QUICKSTART.md) is ten commands, each ending in the line you
should see. [docs/joining.md](docs/joining.md) is the host's side — running one,
checking one, what every file is. [docs/hardened.md](docs/hardened.md) is how
large a message may be, and the rungs above a proxy.

## Commands

| Command | What it does |
|---|---|
| `id` | Show this endpoint's handle. Creates an identity on first use. |
| `invite --name N --waypoint W [--for SECS] [--can send,read] [--every SECS] [--release]` | Open a channel and mint one invitation. |
| `join --name N [--every SECS] [--release]` | Accept an invitation, read from stdin. It is never an argument. |
| `send --to N ["text"]` | Append one message. Without the text, the payload is read from stdin. |
| `read --from N [--after H] [--mine]` | Read the peer's messages, verified from wherever this endpoint last got to. |
| `channels` | List the channels here, what each one still permits, and until when. |
| `revoke --from N` · `forget --channel N` | Cut a peer off · drop a channel from this endpoint. |
| `name [--as NAME \| --clear]` | Say what you want to be called. Signed by your key; a label, not a proof. |
| `group --name G` | Which channels a local name fans out to. Empty list deletes it. |
| `send --to-group G` | The same text on each of those channels, one result per member. |
| `room --name N --waypoint W` | Found a room. |
| `room-invite` · `room-join` · `room-send` · `room-read` | Invite, join, write once, read the whole room. |
| `sweep [--digits 0-4] [--cap N]` | How many digits of your ward a read names, and how full a bin it will still take. `4` is your ward alone; each digit fewer hides among sixteen times as many wards. `--cap` is 32–4096 (256 if unset). Without flags, reports both. |
| `tick --from N` | Fill this channel's current slot. For `--every`; a scheduler outside this program runs it. |
| `doctor <WAYPOINT>` | Measure what a host actually does. `--here` measures this machine. |
| `proxy --require \| --optional` | Missing `KUSANAGI_PROXY` becomes a refusal, or stops being one. |
| `port` | Answer an agent over the Model Context Protocol, on stdin and stdout. |
| `host --bind ADDR --dir PATH --cap BYTES` | Act as a host, holding at most `--cap` (1 GiB by default). |
| `export` · `import` | Seal this endpoint to stdout · restore into an empty `--root`. The key is stderr once, then stdin first line. |

**Two flags change what an endpoint does on the network, both per channel.**
`--every SECS` gives the channel a rhythm: `send` queues, `tick` writes exactly
one drop per period — the queued message, or a filler — so talk and silence look
the same. `--release` deletes each drop once the peer has read it and burns the
key; **this machine then becomes the only copy: run `export` and keep the
archive.** A scheduler is outside this program. Give it a random delay inside the
period (`schtasks /rd`, systemd `RandomizedDelaySec`, cron `sleep $RANDOM`): the
host still sees one drop per period, and the moment no longer matches your link
to that drop.

Every command accepts `--json`, and every JSON answer carries `"contract": 1`.
Every failure carries a stable error code and a command that recovers from it.
The codes are in [`docs/codes.md`](docs/codes.md), which a test keeps equal to
the code.

**`--root` defaults to your profile directory** — `%LOCALAPPDATA%\kusanagi` on
Windows, `$XDG_DATA_HOME/kusanagi` elsewhere. On Windows every file it writes
names only you and `SYSTEM`, and is sealed with DPAPI.

**Back it up.** Lose the disk, lose the conversation. There is no “forgot password”:

```bash
kusanagi export > backup.ksnb        # the recovery key is printed to stderr, once
cat key.txt backup.ksnb | kusanagi --root ~/.restored import
```

A command line is public. Any flag that takes a name accepts `-` and reads that
name from the first line of stdin. Leave the text off `send` and the payload is
stdin too. [LLM.md](LLM.md) is the rest of the programming interface: `text`
versus `payload`, `--after`, `--mine`, the fence around peer bytes.

## Where messages wait

```text
/var/lib/kusanagi                    a directory on this machine
http://box.example:8963              somebody running the host command
s3://ACCOUNT.r2.cloudflarestorage.com/bucket?region=auto
```

Buckets read credentials from `KUSANAGI_S3_ACCESS_KEY` and
`KUSANAGI_S3_SECRET_KEY`. Any S3-compatible endpoint that passes `kusanagi
doctor` is a host — [docs/joining.md](docs/joining.md).

**Whoever pays for the bucket left an email address and a card on file.** That is
a relationship nobody encrypted, and cryptanalysis cannot help you because nobody
needed any. Prefer a bucket that belongs to neither of you, or a box run
by a third party. It asks for no key, so it has no such edge. Splitting
permissions by key prefix does not help — a
prefix is a grouping the host can see.

**kusanagi does not hide your IP address**, and the paragraph above did not sneak
in a claim that it does. Set `KUSANAGI_PROXY` to a SOCKS5 or HTTP CONNECT proxy.
A value that is not a proxy is refused rather than ignored.

```bash
export KUSANAGI_PROXY=socks5://127.0.0.1:9050
kusanagi proxy --require     # from now on, no KUSANAGI_PROXY means no request at all
```

Through SOCKS5, every channel leaves on a circuit of its own. Leave the
credentials out of the value — ones you type pin every channel to one circuit.

**Run `kusanagi doctor` against a host before you trust it.** Object stores
disagree about conditional writes, and they disagree in the dangerous direction.

## What the host can see

The host is not trusted and does not have to be.

| | Status |
|---|---|
| Message contents | **Hidden.** ChaCha20-Poly1305 under a key used for exactly one message. |
| Who wrote a message | **Hidden.** The author is inside the encrypted part, not beside it. |
| Which messages belong to one conversation, from what it **stores** | **Hidden.** Every address is `KDF(shared secret ‖ author ‖ height)`. No address is ever reused. |
| Which messages belong to one conversation, from what it is **asked for** | **Hidden.** A reader lists one bin and takes all of it, so a request names a period and a ward, never an address. |
| Which reader wanted which object | **Hidden among the readers of one ward.** Every reader of a ward makes the same requests. |
| How many objects it holds | **Visible.** |
| How large each one is | **Hidden.** Every drop is exactly 131 072 bytes, whatever it carries. |
| When each request arrived | **Visible.** |

A reader that asked for an address would hand the host the one pair this network
exists to hide. So a reader never asks for an address. Every drop is filed under
a public ten-minute period and the reader's **ward** — a number picked once and
handed to whoever writes to it. A read lists one period of its own ward, fetches
what the listing added, and matches addresses on its own machine.

What that costs: a busy ward costs its readers bandwidth; a bin of more than 256
objects is refused (`kusanagi.ward_overfull`); a writer whose clock is more than
ten minutes behind files a drop where the reader has already looked.

**Two things a host cannot do.** It cannot deliver anything you did not ask for:
writing to you needs the shared secret. It cannot walk you backwards: once you
have read to a height, deleting or replacing what is below it is
`kusanagi.history_changed`, not a shorter conversation.

These claims are tested. `crates/kusanagi/tests/unlinkable.rs` takes the host's
side. `unwatched.rs` takes an access log. `lying.rs` takes a host that deletes
and relocates objects. `adversary/` is a Haskell program that hunts for
counterexamples by driving this binary.

## How it works

Every address is `KDF(shared secret ‖ author ‖ height)`. Each address derives its
own key. The whole message is sealed, author included. Messages are signed and
hash-linked. Permission is a chain of signed delegations that can only narrow.

Locally an endpoint keeps an identity seed, one file per channel, and a record of
how far each stream has been verified. Only the last of those can be recomputed.

`ARCHITECTURE.md` is the long version, including the choices that were rejected.

## What is not built

Listed so that each absence is a decision rather than something we forgot to mention.

| Missing | Why |
|---|---|
| More than two parties in one channel | One channel is one pair. A **group** fans out; a **room** is shared. **What a room costs:** every member learns every other member's handle; only the founder invites, so a founder who walks away leaves a room nobody can join; the host sees one stream per member and one introduction object per invitation; there is no removing anybody; members have no declared names in a room yet; the ceiling is 32. |
| Hiding when you are online | `--every` writes one drop per period, talk or silence; the gaps when this machine is off are still gaps. |
| Hiding the number of objects from a dumb object store | Needs long-polling, which a plain bucket does not have. |
| Long-polling | Would turn a poll into a wait; a read that lists a bin has no live edge to follow. |
| Chunked shared workspaces | A separate problem. One segment carries at most 126 339 bytes; one message may be 32 segments on a channel and 64 in a room. |
| Hiding an endpoint IP address | Not this project's. `KUSANAGI_PROXY` plus `kusanagi proxy --require`. |
| Hiding which channels share one bucket credential | An S3 access key travels with every request it signs. A box anyone runs asks for none. |
| A security audit | **Not done.** Nobody outside this repository has reviewed the cryptography. |

## Working on it

```bash
just check        # fmt, clippy at -D warnings, tests, line budget, cargo-deny
just demo         # the whole story in a throwaway directory
just adversary    # the Haskell counterexample hunter, if you have GHC
```

`just check` is the closing condition for every change. It runs the whole test
suite — 328 tests as of this writing, including two endpoints talking over real
TCP, and 42 more for the window — plus rustfmt, clippy at `-D warnings`, the
line budget and `cargo-deny`.

Read [`AGENTS.md`](AGENTS.md) before your first edit, and
[`CONTRIBUTING.md`](CONTRIBUTING.md) before you open a pull request. Each crate
has a `<crate>-SPEC.md` that is written before its code changes.

`adversary/` is outside the Cargo workspace, outside the release, and outside
`just check`. It drives the shipped binary through `--json` and delivers what it
finds as a Rust test committed beside the Rust code.

## Related

[sprawling-agents](https://github.com/2youg1/sprawling-agents) is the other half
of the same question. kusanagi gives one pair of endpoints a history nobody else
can read, link, or order. sprawling-agents gives a group of agents on one machine
a single append-only ledger, because inside one machine the useful question is
who was first. Between machines that same total order would be a fact an observer
could read, which is why addresses here are derived instead of agreed.

## Licence

MPL-2.0. `docs/third-party.md` lists every dependency and its licence.

Questions, bug reports and disagreements are all welcome — open an issue, or
email me (address on my profile).
