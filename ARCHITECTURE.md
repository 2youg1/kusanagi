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
| 2a | Who talks to whom, in what the host **stores** | **held** | every address is `KDF(secret ‖ author ‖ height)`; no address is ever reused |
| 2b | Who talks to whom, in what the host is **asked for** | **held for a poll**, leaks on a catch-up | a reader resumes from a cairn, so a poll names one address instead of the stream |
| 3 | Network size | leaks the number of objects | nothing yet; the Bell in §9 is the answer |
| 4 | Traffic analysis — when, how large | **leaks** | nothing yet; padding and jitter are §9 |

Property 2 is the one this project exists for, and splitting it in two is the
correction of a real error rather than a refinement. Addresses derived to be
unrelated stop being unrelated the moment one connection asks for them in
ascending order, back to back — and a reader that began at height zero on every
read did exactly that, once per poll, for the whole history. **The derivation was
sound and the reading path gave the answer away.** A host needed no cryptanalysis,
only an access log.

Both halves are asserted rather than described.
`crates/kusanagi/tests/unlinkable.rs` puts a hundred segments through a host, then
takes the host's side and fails if any two records can be linked to each other or
to a person. `crates/kusanagi/tests/unwatched.rs` takes the side of a host that
keeps an access log, and fails if a poll names more than the one address it is
waiting on — or if that cost grows with the length of the conversation.

**What is still open, stated so that it cannot be mistaken for closed.** A reader
catching up on a stream it has never read names every height it fetches, and a
host watching one endpoint over time can follow the live edge as it advances,
because the address polled after a hit is the successor of that hit. Closing the
second needs the Bell in §9: a host asked to wait learns one address rather than a
sequence.

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
| **Site** | what one endpoint keeps on its own disk: a seed, a file per channel, a cairn per stream, a revocation list | the only state there is; anything else would be state a kill could lose |
| **Cairn** | how far one author's stream has been verified: a handle and a head, 73 bytes | a reader resumes instead of re-naming a stream, and cannot be talked back down below it |
| **Box** | a host somebody runs: it holds sealed bytes at opaque addresses and refuses to overwrite one | the untrusted half is a program, not a promise |

Reserved for work not yet done, and therefore **not** in the code: `Bell`,
`Cohort`, `Depot`, `Veil`.

## 5 The crates

```
kernel      identifiers, identity, signed segments, canonical bytes, seams
  chain     the rules a sequence of segments must satisfy
  seal      address derivation and content sealing
  grant     issue, attenuate, verify, revoke
  waypoint  directory / memory / HTTP / S3, conformance, probe — how to reach a host
    box     the box server — how to be one
  site      one endpoint's own disk: identity, channel records, invitations
kusanagi    the verbs and the one assembly point
```

Dependencies point one way only: `kernel` depends on nothing of ours, and
`kusanagi` depends on everything.

**Budget.** Three limits, one purpose, all held by `just budget`:

| Limit | Applies to | What it bounds |
|---|---|---|
| **500 lines** | every file in the repository | how much has to be in your head to judge one line |
| **2,500 lines** | each crate's `src/` | how large one idea may grow |
| **25,000 lines** | the workspace, tests included | how much there is to read at all |

The file is the unit that actually gets opened: a reviewer opens one, an editor
jumps into one, a model reads one. So the limit that decides whether this code can
be read is the limit on a single file, and it is the strictest of the three. **A
file over 500 lines is split or deleted; the number is not raised.** Splitting the
assertions about a rule away from the rule is a legitimate answer — the workspace
total counts tests, so nothing is hidden by moving it.

Two files are outside the count because their contents are not written here:
`Cargo.lock`, which cargo generates, and `LICENSE`, which is the licence verbatim.
That exclusion list is closed; a third entry is a decision, not an oversight.

| crate | src | measured by |
|---|---|---|
| box | 482 | `just budget` |
| chain | 704 | |
| grant | 1,055 | |
| kernel | 1,369 | |
| kusanagi | 2,224 | |
| seal | 393 | |
| site | 1,069 | |
| waypoint | 2,138 | |
| **workspace, tests included** | **12,436 / 25,000** | |
| **largest single file** | **381** (`kusanagi/src/complaint.rs`) | |

**Both crates that were close to the line have been split, and the budget is what
made each decision arrive on time.**

`kusanagi` gave up the disk formats to `site`. The boundary that made it possible
is recorded in `crates/site/site-SPEC.md` §3 — **a `SiteError` says what failed on
the disk, and the door says what it is called and how to recover from it.** Merging
the two would put the words `kusanagi channels` inside a crate that has no verbs.

`waypoint` gave up the server to `box`, which **overturned a decision recorded in
`waypoint-SPEC.md` §7** — that the two halves of the box protocol should share a
crate. The reason that did not exist when it was taken is the 2,500-line limit
itself, and the choice it forced: separating implementations of one seam would
have been worse than separating two different jobs, *reaching* a host and *being*
one. What the old decision feared, the halves drifting apart, is held by a test
rather than by a directory — the box's own tests drive the shipped client against
the shipped server over a real socket and run `conformance::run` against it.

**Outside the workspace.** `adversary/` is a Haskell counterexample hunter. It is not a
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

1. **No resident state, and no local fact a host could not confirm.** Every verb
   is a one-shot command that exits. An endpoint's height comes from the
   waypoint, and the cairn beside it moves where a walk *starts* without ever
   deciding what the walk *concludes* — the first segment read must link to the
   cairn's head, so a resumed walk proves its own join. Deleting every cairn
   therefore changes what a read costs and never what it reports, which is what
   `losing_every_cairn_changes_what_a_read_costs_and_nothing_else` asserts and
   what `a_command_keeps_no_state_that_a_kill_could_lose` asserts for the rest.

   **The one exception is deliberate and is the point of having a cairn.** Against
   a host that withholds or replaces a drop, an endpoint that has read before
   refuses what contradicts what it read, while an endpoint reading for the first
   time cannot. Two readers therefore disagree, and the one with a memory is the
   one that is right.
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
- **`Tier::AckFirstSeen` is enforced by the cairn, not merely named.** The tier
  said both sides must remember what they first saw at an address and refuse
  anything that arrives later; for two versions nothing did, so the tier was a
  label. The cairn is that memory. A host can still refuse to serve a drop —
  nothing prevents that — but it can no longer make a reader who has already read
  believe the stream is shorter, which is the difference between an outage and a
  retraction somebody never sent.
- **A head may come from this endpoint's own record, not only from a segment in
  hand.** `ChainHead` had one constructor because a head is a witness: holding one
  meant having held the segment. `ChainHead::recorded` adds a second provenance
  and is the weaker one. It is admitted on one argument, and if that argument ever
  fails the constructor goes: **every use of a head is a comparison, so a false
  head can only cause a refusal, never an acceptance.** The alternative — keeping
  the last segment itself, which re-verifies its own signature and needs no new
  constructor — was rejected because it would put a copy of every channel's most
  recent message on disk forever, and `crates/kusanagi/tests/at_rest.rs` is what
  holds that line.
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
| `Bell` | **no longer only an optimisation.** A reader that polls names the address it waits on, then the next one after a hit, so a host watching one endpoint can follow the live edge; a host that can be asked to wait is told one address instead. It is still unbuilt, and now it is a privacy mechanism whose absence is measured in §3 rather than a latency tweak |
| `Veil` — padding, jitter, pluggable transports | untestable without a real censor to fail against |
| `Cohort` — rosters and epochs | needs multi-node test infrastructure; two parties do not need a roster |
| `Depot` — chunked workspaces | a separate problem; 64 KiB carries the whole protocol today |
| `port` — local socket and MCP front ends | the verb set is one enum, so a second front end is additive |
| Post-quantum hybrid | a clean addition once the classical suite is right |

---

*This document is licensed MPL-2.0.*
