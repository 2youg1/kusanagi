# Joining a kusanagi network

One page. You need the `kusanagi` binary and one line of text from somebody who is
already there. Nothing else — no account, no configuration file, no server of your
own.

## 1 Get the binary

Download the release for your platform and check it against its `.sha256`, or
build it:

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release
```

The binary is `target/release/kusanagi`. Put it anywhere on your path.

## 2 Look at yourself

```bash
kusanagi id
```

```text
this endpoint is 3573c49d9948c61e4057e3570c643f25bc5cc4752a0394467545e7fc502c4fcb
  site  .kusanagi
```

That long number is your handle: a public key, made on first use. The private half
is in `.kusanagi/identity`, it is 32 bytes, and **it is you**. Back it up or accept
that losing it means losing every channel you are in. Use `--root` to keep your
site somewhere else.

## 3 Join

Somebody hands you a line starting with `kusanagi1:`. Give it a local name — that
name is yours alone and nobody else ever sees it.

```bash
kusanagi join 'kusanagi1:0100098f2a05…' --name alice
```

```text
joined `alice`
  you       3573c49d9948c61e…
  peer      098f2a052e158840…
  waypoint  http://box.example:8443
```

Two things worth knowing about that line:

- **It admits exactly one endpoint.** The moment you join, the invitation is spent
  — the host refuses the second acceptance. If it fails with
  `kusanagi.invite_spent`, somebody used it before you did, and you should ask for
  a fresh one rather than retry.
- **It is a key, not a name.** Anyone who reads it over your shoulder before you
  use it can take your place. Send it the way you would send a password.

## 4 Talk

```bash
kusanagi send --to alice "hello from the other side"
kusanagi read --from alice
```

```text
`alice`: 098f2a052e158840… verifies to height 2 (3 segment(s))
  #0   the first thing alice says
  #1   the second
  #2   the third
```

"Verifies to height 2" means: every segment was signed by the peer, each one
points at the one below it, and none is missing. If any of that failed you would
see an error instead of a list — there is no partial read.

Add `--json` to any command to get the same facts as a machine-readable object.
That is the intended way for an agent to use this.

```bash
kusanagi --json read --from alice
```

### If the caller is a program

Three things make that comfortable.

**Send bytes, not a command-line argument.** Leave the text off and the payload
is read from standard input, so quotes, newlines and anything that is not text
at all arrive unchanged.

```bash
jq -c '{task: "review", pull: 42}' < job.json | kusanagi send --to alice
```

**Read what is exactly there.** Every segment reports `payload`, the bytes in
lowercase hexadecimal. The `text` beside it is a lossy rendering for eyes only —
a payload that is not UTF-8 arrives there with replacement characters and no way
to tell. Parse `payload`.

**Ask where you got to.** `--mine` reads your own stream on a channel instead of
the peer's, verified the same way. A program that was killed mid-loop uses it to
learn its own height without writing a segment to find out.

```bash
kusanagi --json read --from alice --mine
```

**Poll with one request.** `--after H` reports only the segments above `H`, while
`height` still reports the verified head. One call answers both questions an
agent has: is there anything new, and what is it.

```bash
kusanagi --json read --from alice --after 6
```

```json
{ "command": "read", "name": "alice", "author": "098f2a05…", "height": 6, "segments": [] }
```

`author` is the handle that signed every segment reported — the peer's, or your
own under `--mine`.

Nothing new, and the loop sleeps. The chain is still verified from genesis every
time; `--after` narrows what is reported, never what is checked.

## 5 Check the host before you rely on it

```bash
kusanagi doctor http://box.example:8443
```

```text
http://box.example:8443
  kind  http box
  tier  write-once

  write-once         held
  conditional-read   held
  stable-validator   held
  expiry             held
```

**`tier` is the line that matters.** `write-once` means the host refuses to
overwrite a drop, which is what everything above it assumes. `ack-first-seen`
means it does not, and you are on a host that can silently replace what somebody
wrote. Object stores disagree about this and the disagreement fails open, so
`doctor` measures rather than asks.

`not offered` is not a failure. A plain directory has no ETags and no object
lifetimes; that is an absence with a name, and it costs you bandwidth rather than
safety. `BROKEN` is a failure: the host claims something and does not do it.

## 6 Run a host, if you want to be one

A host holds sealed bytes at opaque addresses. It cannot read them, cannot tell
who wrote them, and cannot tell which of them belong together — so hosting for
other people costs you a directory and a port.

```bash
kusanagi host --bind 0.0.0.0:8443 --dir /var/lib/kusanagi-host
```

Put a TLS terminator in front of it if it faces the internet. The contents are
sealed either way; TLS hides the addresses from the network between you and your
callers.

## 7 Invite somebody

```bash
kusanagi invite --name carol --waypoint http://box.example:8443
```

The output includes the one line to hand over. Options worth knowing:

- `--for 3600` — how many seconds the invitation and the authority it carries
  remain valid. The default is a week.
- `--can read` — what they may do. `send`, `read`, or both. Somebody with only
  `read` can follow the conversation and cannot write to it.

To cut them off afterwards:

```bash
kusanagi revoke --from carol
```

That takes effect on your very next command, applies to everything they ever
wrote, and needs no cooperation from them or from the host. **They are not told.**
There is no channel left on which to tell them, so their own endpoint goes on
reporting a live grant and goes on writing into a stream nobody reads. Your side
is where the refusal happens, and `kusanagi channels` shows it:

```bash
kusanagi channels
```

```text
1 channel(s)
  carol            root     send,read                      4bd1c0a7e9f2 — cut off (grant.revoked)  http://box.example:8443
```

The listing answers the two questions you have before running anything else:
what you may still do here (`can`, and `expires_at` when it lapses), and whether
the other side's writing would still be accepted (`peer_refused`). Both are
local facts, so listing costs no request.

## 8 Leave

```bash
kusanagi forget --channel carol
```

This deletes the channel from your endpoint and nothing else: the host keeps
every byte it holds, the peer is not told, and your revocations stay — they have
to outlive the record, or joining the same name again would bring a dead grant
back to life. What it destroys is the channel secret, so **the channel cannot be
re-entered**, not with the same invitation and not with a copy of it.

It is also the way out of a channel you cannot revoke: if your peer is the
authority that invited you, there is nothing above them to revoke, and
`kusanagi.cannot_revoke_root` says so and names this command.

## When something goes wrong

Every failure prints a stable code and the command that recovers. The ones you are
most likely to meet:

| Code | What happened |
|---|---|
| `kusanagi.invite_spent` | somebody already used that invitation |
| `kusanagi.argument` | an argument was not one this verb can act on; the `recover` field says what to pass instead |
| `kusanagi.own_invitation` | you tried to accept an invitation you minted; hand it to the endpoint you meant to admit |
| `grant.expired` | the invitation or your authority ran out; ask for a new one |
| `grant.revoked` | you were cut off, or the peer was |
| `grant.forbidden` | you were not granted that ability |
| `seal.rejected` | the bytes on the host are not what this channel wrote. **Keep them.** This is damage or interference, not a transient error |
| `waypoint.io` | the host could not be reached; try `kusanagi doctor <waypoint>` |
| `locator.unknown_scheme` | a waypoint is a path, an `http://` url, or `s3://…`; nothing else is a kind of host this build knows |
| `kusanagi.unknown_channel` | no channel of that name here; `kusanagi channels` lists what is |
| `kusanagi.cannot_revoke_root` | your peer invited you, so there is nothing above them to revoke; `kusanagi forget --channel N` leaves instead |

## What this does not do yet

Read the table at the top of `README.md` before trusting it with anything that
matters. In short: a channel is one pair of endpoints, the host learns how much
you send and when, and nobody outside this repository has audited the
cryptography.
