# The box protocol

What `kusanagi host` answers, and what `HttpWaypoint` sends. Anybody can implement
either half; both halves in this repository are written against this page, and
`waypoint::conformance::run` is what decides whether an implementation is correct.

The protocol is HTTP/1.1 and has three requests. It is deliberately smaller than
S3, because everything this network asks of a host is small.

## What a host is not asked to do

- **It is never asked to overwrite.** There is no unconditional write in the
  protocol, so a host cannot lose write-once semantics by accident.
- **It is never asked to list.** A caller who does not already know an address
  learns nothing from the host, which is what makes unlinkable addressing worth
  anything.
- **It is never asked who anybody is.** There are no accounts and no
  authentication, so a host has nothing to disclose and nothing to leak.

Access control, if a deployment wants it, belongs in front of the host — a
reverse proxy, an allowlist, a VPN. It is not in this protocol because a host that
knew who its callers were would know something the design promises it cannot.

## `GET /d/<address>`

`<address>` is exactly 40 lowercase hexadecimal characters.

| Response | When |
|---|---|
| `200` + body + `ETag` | the drop holds bytes |
| `304`, no body | `If-None-Match` matched the current `ETag` |
| `404` | nothing is there, or what was there has expired |

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
| `If-None-Match: *` | **required** |
| `X-Kusanagi-Ttl: <seconds>` | optional; `0` means "already expired" |

| Response | When |
|---|---|
| `201` | the address was empty and now holds these bytes |
| `412` | the address was already claimed; the stored bytes are untouched |
| `428` | `If-None-Match: *` was missing |
| `400` | the lifetime was not a whole number of seconds |

`412` is not an error condition for a caller. A resend after a lost
acknowledgement lands here, and the correct response is to carry on.

The `0` lifetime is what makes expiry testable without waiting: a host that
honours lifetimes answers the next `GET` with `404`, and one that ignores them
hands the bytes back. `kusanagi doctor` uses exactly that.

## `GET /health`

Returns a plain-text banner naming the implementation and what it offers:

```text
kusanagi-box/1 write-once=yes conditional-read=yes expiry=yes
```

The banner is a courtesy, **not evidence**. `doctor` ignores it and measures.

## Limits

| | |
|---|---|
| request head | 8 KiB |
| body | 1 MiB |
| idle connection | 30 seconds |
| connection reuse | none; every response carries `Connection: close` |

A segment is capped at 64 KiB by the protocol above this one, so the body limit is
margin rather than a constraint.

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
