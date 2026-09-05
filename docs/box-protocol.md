# The box protocol

What `kusanagi host` answers, and what `HttpWaypoint` sends. Anybody can implement
either half; both halves in this repository are written against this page, and
`waypoint::conformance::run` is what decides whether an implementation is correct.

The protocol is HTTP/1.1 and has three requests. It is deliberately smaller than
S3, because everything this network asks of a host is small.

**Every header in it is one that ordinary web traffic already carries.** A header
named after this project would announce it to the host, to every proxy on the
route, and to every log either of them keeps — including the ones that see inside
TLS. No amount of sealing further down takes that back, so there are none.

## What a host is not asked to do

- **It is never asked to overwrite.** There is no unconditional write in the
  protocol, so a host cannot lose write-once semantics by accident.
- **It is never asked who anybody is.** There are no accounts and no
  authentication, so a host has nothing to disclose and nothing to leak. (Listing
  used to be on this list; D-20 moved it off, because a read that names a bin
  instead of an address is what stops the host pairing a writer with a reader.)
- **It is never asked to describe itself.** There is no banner, no version and no
  status path. A well-known path that answers with a product name turns an
  internet-wide scan into a list of this network's hosts, and their users with
  them, at one request per address.

Access control, if a deployment wants it, belongs in front of the host — a
reverse proxy, an allowlist, a VPN. It is not in this protocol because a host that
knew who its callers were would know something the design promises it cannot.

## `GET /d/<period>/<ward>/<address>`

The key is three components: a sixteen-digit period, a four-digit ward, and the
40-character drop address. A reader never names the third alone anymore — it
sweeps a ward and takes all of it — so the host learns which bin was swept and
never which object of it was wanted (`ARCHITECTURE.md` §9 D-20).

| Response | When |
|---|---|
| `200` + body + `ETag` | the drop holds bytes |
| `304`, no body | `If-None-Match` matched the current `ETag` |
| `404`, no body | nothing is there, or what was there has expired |

## `GET /bin/<prefix>`

Answers with the keys under `<prefix>`, one per line, where `<prefix>` is a
period plus zero to four hex digits of a ward. Names and nothing else: a host
that answered with bodies would learn which object was wanted by watching which
one was *not* fetched afterwards.

A host answers `404` with an **empty body**, and answers the same `404` to a
request that is not about a drop at all — a path outside `/d/`, a method it does
not implement, an address that is not 40 lowercase hexadecimal characters. Three
different answers would let a caller who holds no address recover the address
grammar, and the grammar is enough to tell this host from any other server on the
same port. Asserted by `crates/box/tests/unmarked.rs`.

`ETag` must be **stable**: the same bytes must produce the same validator on every
request. The reference host uses the BLAKE3 hash of the stored bytes, so
stability is a property of the construction rather than of the host remembering
anything.

A read must have no side effect. Polling an empty address is the most common
request in this network, and a read that created something would make watching a
drop change the world.

## `PUT /d/<period>/<ward>/<address>`

| Request header | |
|---|---|
| `If-None-Match: *` | **required**, and matched exactly; `"*"`, `W/*` and `**` are not it |
| `Cache-Control: max-age=<seconds>` | optional; `0` means "already expired" |

| Response | When |
|---|---|
| `404`, no body | **always** |

**A write is never confirmed.** Stored, refused as occupied, dropped for want of
`If-None-Match: *`, dropped because the host is full — one answer, byte for byte
the answer every other request gets. A status that distinguished them would make one `PUT` to a `/d/` key enough to identify a box, and an internet-wide scan
enough to enumerate this network's hosts.

**A caller finds out by looking.** `GET` the same address: bytes equal to what was
sent means this write landed, different bytes mean somebody else's did and the
address is spent, and nothing there means the write did not happen. The reference
client does exactly this, and reports `waypoint.unwritten` for the last case.
The extra request is a protocol constant, so it changes what a host counts and
not what a host can tell apart.

A `Cache-Control` value a host cannot parse is **ignored**, not refused, which is
what RFC 9111 §5.2 asks of a recipient and also what keeps a malformed value from
being a way of telling this host apart from a cache. A lifetime too large to add
to the clock saturates.

An address that is already claimed is not an error condition for a caller. A
resend after a lost acknowledgement finds its own bytes there, and the correct
response is to carry on.

The `0` lifetime is what makes expiry testable without waiting: a host that
honours lifetimes answers the next `GET` with `404`, and one that ignores them
hands the bytes back. `kusanagi doctor` uses exactly that — and it is the one
case where reading back cannot confirm a write, because a lifetime that has
already elapsed and a write that never happened are the same empty address.

## There is no fourth request

There is no banner, no version and no status path. A well-known path that
answers with a product name turns an internet-wide scan into a list of this
network's hosts, and their users with them, at one request per address.

A host is measured rather than asked: `kusanagi doctor` writes twice and reads
back, which is evidence, while a self-description is not.

## Limits

| | |
|---|---|
| request head | 8 KiB |
| body | 1 MiB (`kusanagi_waypoint::MAX_OBJECT`, which is also what a client will read back) |
| idle connection | 30 seconds |
| connection reuse | none; every response carries `Connection: close` |
| total stored | 1 GiB by default, `kusanagi host --cap <BYTES>` |

Every sealed drop is exactly 131 072 bytes, and the reference host stores it
behind an eight-byte expiry, so the body limit is eight times a drop rather than
a constraint. A body larger than the limit is refused with `400` before anything
is allocated for it; `Content-Length` must be digits and must appear at most
once, so two readers cannot disagree about where one request ends.

The storage ceiling is silent, like everything else about a write. A host that
answered "full" would be letting a stranger measure how much of it they had
used.

## Storage

How a host stores bytes is its own business. The reference host puts an eight-byte
big-endian expiry in front of the bytes and writes the result through the same
write-once directory adapter an endpoint would use locally, so a swept object and
an object that never existed are the same answer — `404` — with no bookkeeping to
reconcile.

## Implementing the other half

An adapter is correct when `waypoint::conformance::run` passes against it. That
function is a function rather than a set of tests precisely so that it can be run
against a host that is already running, which is what `kusanagi doctor` does.

```rust
use kusanagi_waypoint::{conformance, HttpWaypoint};
use kusanagi_seal::{Secret, Stream};

let place = HttpWaypoint::new("http://box.example:8963");
conformance::run(&place, &namespace)?;
```
