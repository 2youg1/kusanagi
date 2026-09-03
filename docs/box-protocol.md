# The box protocol

What `kusanagi host` answers, and what `HttpWaypoint` sends. Anybody can implement
either half; both halves in this repository are written against this page, and
`waypoint::conformance::run` is what decides whether an implementation is correct.

The protocol is HTTP/1.1 and has two requests. It is deliberately smaller than
S3, because everything this network asks of a host is small.

**Every header in it is one that ordinary web traffic already carries.** A header
named after this project would announce it to the host, to every proxy on the
route, and to every log either of them keeps — including the ones that see inside
TLS. No amount of sealing further down takes that back, so there are none.

## What a host is not asked to do

- **It is never asked to overwrite.** There is no unconditional write in the
  protocol, so a host cannot lose write-once semantics by accident.
- **It is never asked to list.** A caller who does not already know an address
  learns nothing from the host, which is what makes unlinkable addressing worth
  anything.
- **It is never asked who anybody is.** There are no accounts and no
  authentication, so a host has nothing to disclose and nothing to leak.
- **It is never asked to describe itself.** There is no banner, no version and no
  status path. A well-known path that answers with a product name turns an
  internet-wide scan into a list of this network's hosts, and their users with
  them, at one request per address.

Access control, if a deployment wants it, belongs in front of the host — a
reverse proxy, an allowlist, a VPN. It is not in this protocol because a host that
knew who its callers were would know something the design promises it cannot.

## `GET /d/<address>`

`<address>` is exactly 40 lowercase hexadecimal characters.

| Response | When |
|---|---|
| `200` + body + `ETag` | the drop holds bytes |
| `304`, no body | `If-None-Match` matched the current `ETag` |
| `404`, no body | nothing is there, or what was there has expired |

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

## `PUT /d/<address>`

| Request header | |
|---|---|
| `If-None-Match: *` | **required**, and matched exactly; `"*"`, `W/*` and `**` are not it |
| `Cache-Control: max-age=<seconds>` | optional; `0` means "already expired" |

| Response | When |
|---|---|
| `201`, no body | the address was empty and now holds these bytes |
| `412`, no body | the address was already claimed; the stored bytes are untouched |
| `428`, no body | `If-None-Match: *` was missing |

A `Cache-Control` value a host cannot parse is **ignored**, not refused, which is
what RFC 9111 §5.2 asks of a recipient and also what keeps a malformed value from
being a way of telling this host apart from a cache. A lifetime too large to add
to the clock saturates.

`412` is not an error condition for a caller. A resend after a lost
acknowledgement lands here, and the correct response is to carry on.

The `0` lifetime is what makes expiry testable without waiting: a host that
honours lifetimes answers the next `GET` with `404`, and one that ignores them
hands the bytes back. `kusanagi doctor` uses exactly that.

## There is no third request

Earlier versions answered `GET /health` with `kusanagi-box/1 write-once=yes
conditional-read=yes expiry=yes`. It is gone, and nothing replaced it.

The banner was never evidence — `kusanagi doctor` has always ignored it and
measured the host instead, by writing twice and reading back. What it was, was a
one-request test for "is this a kusanagi host", answerable by anybody, which is
the single most useful thing a scanner could have been given. Removing it cost
nothing that was in use: across the whole workspace the only caller was the test
that asserted the banner's own text.

## Limits

| | |
|---|---|
| request head | 8 KiB |
| body | 1 MiB |
| idle connection | 30 seconds |
| connection reuse | none; every response carries `Connection: close` |

Every sealed drop is exactly 4 096 bytes, and the reference host stores it behind
an eight-byte expiry, so the body limit is margin by three orders of magnitude
rather than a constraint. A body larger than the limit is refused with `400`
before anything is allocated for it.

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

let place = HttpWaypoint::new("http://box.example:8443");
conformance::run(&place, &namespace)?;
```
