# ARCHITECTURE

The authority for what kusanagi is and why it is shaped this way. Where this
document and the code disagree, the code is wrong — except where reality
disagrees with this document, in which case this document is corrected first,
with its reason.

Crate detail lives in `crates/<crate>/<crate>-SPEC.md`. Working rules live in
`AGENTS.md`.

---

## 1 The claim

**Two agents on two machines exchange authorised, mutually unlinkable messages
through a host that neither of them trusts and neither of them runs.**

Every design decision below is answerable to that sentence. A change that does
not serve it is a change to the project, not to the code.

## 2 The substrate is a dead drop, not a connection

A sender leaves a segment at an opaque address; a reader collects it later. There
is no session, no presence, no rendezvous.

**Why this rather than an overlay network.** Three reasons, and the first is the
decisive one.

1. **The host already exists.** Anyone who wants a shared workspace has already
   accepted an object store or a company server. Building NAT traversal and a
   membership protocol for the hostless case is paying for a world you are not
   in.
2. **Offline is the normal state.** An agent thinks for tens of seconds and then
   may be idle for hours. In a dead-drop model "the other side is offline" is not
   a case to handle — it is the resting state, and costs no code at all.
3. **One mechanism, two speeds.** Direct delivery is not deleted; it is demoted
   to a waypoint whose implementation happens to be the peer's own process. The
   fast path is a special case of the slow path rather than a second protocol
   beside it.

The price is that latency has a floor of one polling interval. That price is paid
by a consumer that thinks for seconds, so it is not observable.

## 3 What "private" means here, exactly

Four separate properties. Claiming them as one word is how privacy claims become
false.

| # | Property | Today | By what |
|---|---|---|---|
| 1 | Content confidentiality | **held** | ChaCha20-Poly1305 under a key used for exactly one message |
| 2 | Who talks to whom, as seen by the host | **held** | every address is `KDF(secret ‖ author ‖ height)`; no address is ever reused |
| 3 | Network size | leaks the number of objects | nothing yet; the Bell in §9 is the answer |
| 4 | Traffic analysis — when, how large | **leaks** | nothing yet; padding and jitter are §9 |

Property 2 is the one this project exists for, and it is asserted rather than
described: `crates/kusanagi/tests/unlinkable.rs` puts a hundred segments through a
host, then takes the host's side and fails if any two records can be linked to
each other or to a person.

Two consequences follow from property 2 that were not designed for and are worth
naming:

- **Unsolicited delivery is uncomputable, not filtered.** Writing to somebody
  requires their address; deriving their address requires the shared secret;
  holding the secret requires having been introduced. Spam is not rejected at the
  door — there is no door to knock on.
- **A one-time invitation needs no bookkeeping.** The greeting an invitee writes
  lands at a write-once address, so the *host* refuses the second acceptance.
  Nothing tracks whether an invitation has been used.

**What the host still learns**, stated so it cannot grow quietly: the number of
objects it holds, the exact size of each, and the time of every request. It never
learns content, authorship, relationships, or the existence of a relationship.

## 4 The words

One name per concept. A word with no implementation does not enter the code.

| Word | What it is | What it fixes |
|---|---|---|
| **Segment** | the only thing that travels: signed, hash-linked bytes | there is no separate "message"; a segment is the event |
| **Drop** | an opaque address that receives exactly one segment | addresses never repeat, so relationships never appear |
| **Stream** | one author's sequence of drops inside a channel | two people sharing one secret never contend for an address |
| **Waypoint** | anything that stores bytes under a key | the store is never trusted; everything is checked against a hash |
| **Grant** | offline-verifiable authority that can only narrow | permission exists in this form and no other |
| **Channel** | one conversation: a secret, a locator, a standing, a peer | the unit an endpoint joins, lists, and revokes |
| **Standing** | why somebody is allowed on a channel — root, or granted | "the authority holds no grant" is a fact, not a missing value |

Reserved for work not yet done, and therefore **not** in the code: `Bell`,
`Cohort`, `Depot`, `Veil`.

## 5 The crates

```
kernel      identifiers, identity, signed segments, canonical bytes, seams
  chain     the rules a sequence of segments must satisfy
  seal      address derivation and content sealing
  grant     issue, attenuate, verify, revoke
  waypoint  directory / memory / HTTP box / S3, the box server, conformance, probe
kusanagi    the verbs, the local site, and the one assembly point
```

Dependencies point one way only: `kernel` depends on nothing of ours, and
`kusanagi` depends on everything.

**Budget.** Each crate's `src/` stays under **2,500** lines; the workspace
including tests stays under **25,000**. `just budget` fails the build otherwise.
The per-crate limit counts implementation because it is a rule about how large one
idea may grow; the total counts tests too, because the reason for the budget is
that a newcomer — human or model — can read all of it, and tests are read.

| crate | src | measured by |
|---|---|---|
| chain | 438 | `just budget` |
| grant | 1,347 | |
| kernel | 1,479 | |
| kusanagi | **2,424** | |
| seal | 392 | |
| waypoint | **2,448** | |
| **workspace, tests included** | **9,944 / 25,000** | |

**Two crates are close to the line, and that is information rather than a
problem.** The next substantive change to `kusanagi` or to `waypoint` begins by
splitting it — the budget exists to make that decision arrive on time, instead of
arriving as a feature bent into the space left over.
`crates/kusanagi/kusanagi-SPEC.md` §7 records which seam was examined and why it
is not free.

**Outside the workspace.** `adversary/` is a Haskell property oracle. It is not a
crate, not a dependency, not part of the release, and not counted here. §8 records
why it is allowed to exist and what stops it becoming a second authority.

## 6 The seams

A seam with one implementation is a description of that implementation. Every one
of these ships with a second, and with a contract both must satisfy.

| Seam | Declared in | Production | Second | The contract |
|---|---|---|---|---|
| `Waypoint` | `kernel::waypoint` | S3 bucket, HTTP box | directory, memory | `waypoint::conformance::run` — a function, so `doctor` can run it against a live host |
| `Conditional` | `waypoint::conditional` | HTTP box, S3 | directory answers "not offered" | `waypoint::probe::examine` |
| `Clock` | `kernel::clock` | `kusanagi::world::SystemClock` | `kernel::FixedClock` | one sampling point, held by `clippy.toml` |

**Deliberately not seams.** `chain`, `grant` and `seal` have one implementation
each because they *are* the rules. An interface over a rule is an invitation to a
second authority for that rule.

## 7 The laws

1. **No resident state.** Every verb is a one-shot command that exits. An
   endpoint's height comes from the waypoint, never from a local file, so killing
   any process between any two commands changes no result. Asserted by
   `a_command_keeps_no_state_that_a_kill_could_lose`.
2. **Memory does not grow with the work.** `Verifier` holds one author and one
   head for a chain of any length; `Segment::extend` takes a 40-byte `ChainHead`,
   not a predecessor. A change that buffers a chain has broken the design.
3. **One authority per rule.** Hex has one parser, time has one sampling point,
   randomness has one source, the verb set has one definition.
4. **A segment that exists was signed.** Both constructors sign and the decoder
   verifies, so there is no unverified-segment state for a later caller to
   forget.
5. **Failures are typed and carry a way out.** Every error names the action, the
   subject, a stable code, and the command that recovers.

## 8 Decisions on record

Reopening one of these requires a reason that did not exist when it was taken.

- **Handles are public keys, and segments carry signatures.** Stage 0 derived a
  handle from a name, which named a writer without proving one. Grants name
  subjects by handle, so without signatures a grant would restrict only the
  software that chose to obey it. The wire format is `kusanagi.segment.v2`.
- **The sealed form is the whole segment, not its payload.** A segment carries its
  author in the clear; sealing only the payload would let a host group drops by
  author and property 2 would be worth nothing.
- **Attenuation is a lattice meet, not a check.** Asking for more than you hold
  does not fail — it yields what you hold. Widening is unrepresentable rather than
  rejected. Sampled by `crates/grant/tests/attenuation.rs`; the `kani` harness for
  proving it against real MIR is committed in `src/chain.rs` under `#[cfg(kani)]`
  and runs where `kani` is installed.
- **Hosts are measured, not believed.** Conditional-write support diverges between
  S3-compatible hosts, and the divergences fail *open* — the condition is ignored
  and the write succeeds. `kusanagi doctor` writes twice and reads back, then
  issues a certificate naming a tier.
- **The invitation is a bearer token.** Whoever writes one cannot know who will
  accept it, so it carries a one-time key that the acceptor immediately delegates
  to their own handle.
- **One cipher suite, mandatory.** Two endpoints with different derivations cannot
  compute each other's addresses; a pluggable suite is not a degradation but a
  partition.
- **Scale is layered, not flat.** Cohorts of about a thousand, joined by
  transitive grants. Flat global reachability needs a globally resolvable name
  table, and that table is a relationship graph.
- **Bell is a waypoint capability, not a protocol requirement.** A host that can
  long-poll needs no Bell and leaks nothing; the cost of the alternative is paid
  only by whoever chose a dumb object store.
- **The adversary is out of the workspace and speaks only through the door a user
  has.** `adversary/` drives the shipped binary with `--json` and asserts
  *relations between traces* — never an expected output, because restating a rule
  is what a second authority is. Haskell earns the place because a lying host is a
  choice over a strategy space and a directed attack is "any prefix, then this,
  then any suffix": uniform random generation is a fuzzer, not an adversary. Four
  properties keep it from drifting: it enters through the same door a person
  enters through, it states relations rather than behaviour, it never gates the
  Rust build, and **what it delivers is a Rust regression test**. Haskell finds,
  Rust remembers, so knowledge only ever moves toward the shipped language. Delete
  the directory and the network is unchanged.

**Deliberately not adopted.** Fuzzy message detection and oblivious message
retrieval solve delivery *without* a shared secret, and we always have one. MLS
is a later `seal` adapter, not a substrate. DHT or blockchain discovery publishes
the relationship graph this design exists to hide. Mixnets and PIR conflict with
the cost target; a SOCKS adapter outsources that problem to networks built for it.

## 9 Not built

Named so that their absence is a decision rather than an oversight. Each is a
version of its own.

| Missing | Why it waits |
|---|---|
| `Bell` | an optimisation, and there is no traffic yet to measure it against |
| `Veil` — padding, jitter, pluggable transports | untestable without a real censor to fail against |
| `Cohort` — rosters and epochs | needs multi-node test infrastructure; two parties do not need a roster |
| `Depot` — chunked workspaces | a separate problem; 64 KiB carries the whole protocol today |
| `port` — local socket and MCP front ends | the verb set is one enum, so a second front end is additive |
| Post-quantum hybrid | a clean addition once the classical suite is right |

---

*This document is licensed MPL-2.0.*
