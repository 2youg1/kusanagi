# Running a host, and what to check before relying on one

Sending a first message is on [QUICKSTART.md](../QUICKSTART.md); the interface
for a program is on [LLM.md](../LLM.md). This page is the other side: where the
messages wait, who runs that, and how to know it is good enough.

A **host** stores fixed-size encrypted objects at addresses only the two ends
can derive. It is never trusted, so it can be anything that stores bytes: a
directory, a synced folder, `kusanagi host` run by anybody, or an S3-compatible
bucket. Prefer one that belongs to neither party — a bucket's owner is a
relationship edge the provider can read without breaking any cryptography.

## 1 Check the host before you rely on it

```bash
kusanagi doctor http://box.example:8963
```

```text
http://box.example:8963
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

## 2 Check your own side

```bash
kusanagi doctor --here
```

Where the site is, whether it sits under your profile directory, which store
seals its records, whether a proxy is set, and what this binary hashes to. It
reaches nothing and reveals nothing: paste it into a bug report as it stands.

## 3 Run a host

A host holds sealed bytes at opaque addresses. It cannot read them, cannot tell
who wrote them, and cannot tell which of them belong together — so hosting for
other people costs you a directory and a port.

```bash
kusanagi host --dir /var/lib/kusanagi-host              # 127.0.0.1:8963
kusanagi host --bind 9000 --dir /var/lib/kusanagi-host  # a bare port is loopback
kusanagi host --bind 0 --dir /var/lib/kusanagi-host     # any free port, printed on stderr
kusanagi host --bind 0.0.0.0:8963 --dir /var/lib/kusanagi-host  # every interface
```

The default port is inside the block IANA lists as unassigned, so the first of
these usually works on a machine that is already running other things. When it
does not, the answer is `kusanagi.address_unavailable` and the way out is another
`--bind`; a host never moves to a different port on its own, because its address
is written into every invitation it has already handed out.

Put a TLS terminator in front of it if it faces the internet. The contents are
sealed either way; TLS hides the addresses from the network between you and your
callers. `--cap BYTES` bounds what the host will store in total, so a stranger
cannot fill your disk; a full host refuses new objects and says so.

**A host you run at home is your address.** Its locator goes into every
invitation you hand out, so whoever holds one knows where you are. When that
matters, run it as a Tor onion service (`HiddenServicePort 8963 127.0.0.1:8963`
in `torrc`, and hand out `http://<name>.onion:8963`), or let a third party host.

**Callers should go through a proxy.** `KUSANAGI_PROXY=socks5://127.0.0.1:9050`
sends every request through Tor and gives each channel its own circuit; the
host then sees exits, not homes. On a machine where the proxy must not be
optional, `just confine` (Windows Firewall, see `confine.md`) lets the binary
reach the proxy and nothing else.

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
