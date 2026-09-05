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

How large a message may be, and the rungs above a proxy, are on
[hardened.md](hardened.md).

## 4 Any S3-compatible store

The protocol asks a host for four things: a write that refuses to overwrite
(`PUT` with `If-None-Match: *`), a read (`GET`), a listing by prefix (`LIST`),
and expiry by lifetime. Any S3-compatible endpoint that does those four is a
host. There is no adapter to write. MinIO, Garage, SeaweedFS, Ceph RGW, and
Storj's S3 gateway are in that set. Content-addressed stores are not: IPFS and
Filecoin name an object by what it contains, so they cannot store a drop at an
address this protocol derived.

Point `s3://` at the endpoint and run `kusanagi doctor s3://…` before you rely
on it. The cell that fails most often is the conditional write; when that cell
is not `held`, the endpoint cannot be a host.

A store you run yourself moves the observer of the credential edge — the access
key that links every write one key signed — from a cloud vendor to whoever
operates your nodes. If those nodes belong to more than one party, listing and
fetch are still functions of public data (a period and a ward), the same as on
a bucket.

## 5 Where the bytes live

**On this machine** (`--root`; `%LOCALAPPDATA%\kusanagi` on Windows,
`$XDG_DATA_HOME/kusanagi` elsewhere). File names under the site are keyed hashes
of names, not the names. A listing of this directory is a count, not a graph.
On Windows every file is this account and `SYSTEM`, and sealed with DPAPI, so a
disk without the password is noise.

| File | What it holds | Who reads it | If it is gone |
|---|---|---|---|
| `identity` | signing seed and ward | this account | every channel and room this endpoint is in |
| `channels/<hash>` | one channel: secret, locator, standing, peer | this account | that channel; only an `export` archive restores it |
| `rooms/<hash>` | one room: secret, founder, roster height | this account | that room, same as a channel |
| `groups/<hash>` | which channels a local group name fans out to | this account | the name; the channels remain |
| `cairns/<hash>/<hash>` | how far one author's stream is verified | this account | the next read walks again; it reports the same result and costs more requests |
| `sweeps/<hash>/<hash>` | the last listing of one bin | this account | the next read lists from the channel's opening; same result, more listings |
| `ratchets/<hash>/<hash>` | how far keys on a releasing channel are burned | this account | those drops cannot be opened; nobody else holds this |
| `outbox/<hash>/<ticket>` | a payload waiting for its slot | this account | that message was never sent |
| `slots/<hash>` | the last slot filled | this account | the next tick may write a slot already filled |
| `revoked` | step identifiers cut off here | this account | a cut-off peer can be accepted until you revoke again |
| `alias` | what this endpoint asks to be called | this account | set it again; peers already met do not see a change |
| `egress` | whether a missing proxy is a refusal | this account | the site reaches hosts directly again |
| `sweep` | how wide a read is, and the bin cap | this account | width 4 and cap 256, the build's defaults |

**On the host.** Anyone who can speak the waypoint can fetch these: a box has
no accounts; a bucket needs the credential.

| Object | What it holds | Who can fetch it | If it expires |
|---|---|---|---|
| `period/ward/address` | one sealed drop, always 131 072 bytes | anyone who can list that prefix and GET | an unread message is gone |
| period 0 (rendezvous) | the offer and the greeting an invitation points at | anyone holding that one-time address | the invitation dies |

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
