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

**Against whom, first.** Three adversaries, and every claim below names which one
it answers:

- **The host**, which holds the bytes and keeps an access log. Assumed hostile
  from the start; that assumption is the whole of §2.
- **Somebody on the path** — an ISP, a proxy, a middlebox — who sees requests
  leave an endpoint and cannot read what is in them.
- **Somebody scanning**, who holds no address and is looking for hosts and users
  to add to a list.

All three are assumed to hold **this repository**, and that is not the ordinary
Kerckhoffs assumption. A public implementation of a hiding mechanism is a
supervised learning problem with unlimited labelled data: an adversary runs our
binary as often as they like, labels both sides, and trains a classifier. Any
residual difference that is not cryptographically negligible will be found.

Two consequences run through everything below. **Mimicry is not a defence** —
imitating a protocol means imitating its error handling, its retries and its
quirks, and the adversary needs one discrepancy (Houmansadr et al., *The Parrot
Is Dead*, IEEE S&P 2013). And **an argument that we are indistinguishable is
worth nothing next to a measurement**, which is why law 6 in §7 exists.

**And assume the compute is unbounded.** That is the design posture, and it has a
precise consequence rather than a rhetorical one: **every computational guarantee
here fails against it.** ChaCha20-Poly1305, BLAKE3's key derivation and Ed25519
are all computationally secure and nothing else. An unbounded adversary recovers
a channel secret by search, and *verifies* the guess against two addresses, so
even unlinkability falls. No stronger cipher repairs this; only a one-time pad
would, and a one-time pad needs as much key material as traffic plus a consumed
position on disk, which is a different network from this one.

What survives an unbounded adversary is the class of observables that are **not a
function of any secret**. Not hard to invert — not a function at all. A drop is
4 096 bytes whatever it carries, so the mutual information between what a host
measures and how much was said is exactly zero, and it stays zero for an
adversary of any size. That is an argument, not an experiment, and it is stronger
than anything the discriminator can report.

So the table below says which kind each property is, and the two kinds are ranked
deliberately: **unbounded compute is useless against bytes nobody collected.** The
work of the information-theoretic rows is to deny an adversary the ciphertext on
which their compute would otherwise be spent — which is why the row numbered 0
comes first.

Seven separate properties. Claiming them as one word is how privacy claims
become false.

| # | Property | Kind | Today | By what |
|---|---|---|---|---|
| 0 | That this network is in use at all | information-theoretic once built | **leaks to a path observer, closed against a scanner** | a host answers a stranger exactly as a static file server does, and no request or response names this project; but an endpoint's own traffic is still traffic nobody else generates |
| 1 | Content confidentiality | computational | **held until the compute is unbounded** | ChaCha20-Poly1305 under a key used for exactly one message |
| 2a | Who talks to whom, in what the host **stores** | computational | **held until the compute is unbounded** | every address is `KDF(secret ‖ author ‖ height)`; no address is ever reused |
| 2b | Who talks to whom, in what the host is **asked for** | computational | **held for a poll**, leaks on a catch-up | a reader resumes from a cairn, so a poll names one address instead of the stream |
| 3 | Network size | information-theoretic once built | leaks the number of objects | nothing yet |
| 4a | Traffic analysis — **how large** | **information-theoretic** | **held** | every sealed drop is exactly 4 096 bytes, whatever it carries, so the observation is a constant |
| 4b | Traffic analysis — **when, and how often** | information-theoretic once built | **leaks** | nothing; cover traffic is §9 and is the largest thing still missing |

Rows 0, 3 and 4b are marked "once built" because the mechanism that closes each
of them makes an observation independent of the secret rather than expensive to
invert. Filling every slot whether or not anybody is talking makes the count and
the rhythm functions of the clock alone — and note what follows: **the schedule
should then be public and deterministic, not secret.** Hiding a schedule is a
computational defence that an unbounded adversary strips away; filling it is an
information-theoretic one that survives knowing everything.

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

Property 4a is asserted from the other side as well, and that assertion is the
only one here that takes the adversary's own method. `adversary/` builds paired
worlds — four where a byte is said and four where three thousand are — measures
ten features of what the host is left holding, and fails if any single threshold
separates them by more than their own spread. A stump that separates two groups
*is* the rule a censor deploys, so a run with no such stump is the claim, and a
run with one prints it as a sentence. Before the fixed-size envelope the same
experiment separated the worlds on `size.largest` at 528 against 3 600.

The same experiment states property 3 rather than describing it: the features
allowed to separate a silent channel from a busy one are written down as a list
of exactly two, both of them the object count in different units, and the
assertion is an equality. A new feature that starts separating is a regression; a
declared one that stops is somebody closing a leak and having to come and say so.

**What is still open, stated so that it cannot be mistaken for closed.** A reader
catching up on a stream it has never read names every height it fetches, and a
host watching one endpoint over time can follow the live edge as it advances,
because the address polled after a hit is the successor of that hit. And an
endpoint emits requests only when there is something to say or to fetch, so the
*rhythm* of a conversation is visible even when none of its content, size or
parties are. That last one is 4b, it is the largest gap in this table, and
nothing in the code addresses it today.

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
objects it holds and the time of every request. It no longer learns the size of
any of them — they are all the same size — and it never learns content,
authorship, relationships, or the existence of a relationship.

**What an endpoint's own disk gives up** is a separate question with a separate
answer. A site holds the identity seed and every channel secret and cannot avoid
holding them, so the claim is narrower and checkable: it holds **no message**
(`crates/kusanagi/tests/at_rest.rs`), and on a Unix host nothing it writes is
readable by any other account (`crates/site/tests/unreadable.rs`). Windows has no
equivalent because mode bits have none; that gap is §9.

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
| **Veil** | the one size every sealed drop has: 4 096 bytes, a checked pad, no exceptions | how much was said stops being a thing anybody holds |

`Veil` was reserved until this version and is now half of what it was reserved
for. It named padding, jitter and pluggable transports together; padding exists,
so the word enters the code meaning exactly that, and the other two stay in §9
under their own description rather than borrowing a name that is now taken.

Reserved for work not yet done, and therefore **not** in the code: `Bell`,
`Cohort`, `Depot`.

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
| **400 lines** `.rs` and `.hs`, **500** `.md`, **300** anything else | every file in the repository | how much has to be in your head to judge one line |
| **2,500 lines** | each crate's `src/` | how large one idea may grow |
| **25,000 lines** | the workspace, tests included | how much there is to read at all |

The first limit is per kind because the kinds fail differently, and this table
said a flat 500 for two versions after `just budget` had stopped agreeing with
it. The gate was right and the document was wrong, which is the case §0 says
this document loses.

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
| box | 538 | `just budget` |
| chain | 704 | |
| grant | 1,055 | |
| kernel | 1,396 | |
| kusanagi | 2,268 | |
| seal | 694 | |
| site | 1,161 | |
| waypoint | 2,141 | |
| **workspace, tests included** | **13,854 / 25,000** | |
| **largest single file** | **388** (`kernel/src/segment.rs`) | |

**Three crates have been split at this line, and the budget is what made each
decision arrive on time.** The third was `crates/kusanagi/tests/unwatched.rs`,
which sat one line over for two versions with the gate red and nobody looking;
what came out of it is `resuming.rs`, holding the two assertions about a cairn
being recomputable rather than the three about what a poll costs.


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
6. **Indistinguishability is measured, not argued.** §8 already holds that hosts
   are measured rather than believed. Once this repository is public the same
   applies to us: an adversary can generate labelled examples of our own traffic
   without limit, so any claim that two situations look alike has to be an
   experiment somebody ran, not a paragraph somebody wrote. `adversary/` runs it
   — paired worlds, ten features, best single-threshold rule — and the leaks that
   remain open are a written list it compares against, so both a new leak and a
   closed one turn the run red.

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
  partition. The invitation carries a suite byte and refuses one it does not
  know, so changing the suite is a network-wide flip rather than a negotiation —
  which is the migration path for everything below.
- **Ed25519 stays, and ML-DSA is refused with numbers rather than with taste.**
  The signature here is *inside* the sealed envelope: placing bytes at a drop
  requires the channel secret, so an adversary who breaks Ed25519 without that
  secret can compute no address and produce nothing that opens. What a signature
  actually defends is a co-member forging their peer's stream, or a grant holder
  forging a wider grant — both of which require already being inside the channel.
  Against `MAX_SEGMENT = 4 076`:

  | suite | public key | signature | segment overhead | payload left |
  |---|---|---|---|---|
  | Ed25519 | 32 | 64 | 141 | **3 935** |
  | ML-DSA-44 | 1 312 | 2 420 | 3 777 | **299** |
  | ML-DSA-65 | 1 952 | 3 309 | 5 306 | does not fit one drop |
  | Falcon-512 | 897 | 666 | 1 608 | 2 468 |
  | SLH-DSA-128s | 32 | 7 856 | 7 933 | needs a 16 KiB drop |

  ML-DSA costs 92% of a message, and a 1 312-byte `Handle` takes the invitation
  from about 600 hexadecimal characters to over ten thousand — which ends the
  one-line invitation §2 is built on — and takes a cairn's filename from 64
  characters to 2 624. Falcon-512 is the only one that survives the current
  envelope, and FIPS 206 is not final, its signing is floating-point, and
  constant-time implementations are a known hazard. SLH-DSA keeps identities at
  32 bytes, which would leave every filename and every derivation untouched, at
  the price of a 16 KiB drop — and drop size multiplies straight into the cost of
  the cover traffic §9 still owes.

  **The symmetric layer needs nothing.** A 256-bit ChaCha20 key leaves 128 bits
  under Grover and BLAKE3's preimage resistance the same, so the part of this
  design that carries confidentiality is already adequate against a quantum
  adversary. The place a post-quantum decision will actually matter is the day a
  key exchange is introduced, because there is none today — and on that day it
  must be hybrid (X25519 with ML-KEM-768) from the first commit rather than
  added afterwards.
- **Scale is layered, not flat.** Cohorts of about a thousand, joined by
  transitive grants. Flat global reachability needs a globally resolvable name
  table, and that table is a relationship graph.
- **Bell is a waypoint capability, not a protocol requirement.** A host that can
  long-poll needs no Bell and leaks nothing; the cost of the alternative is paid
  only by whoever chose a dumb object store.
- **Every sealed drop is one size, and the size is not a parameter.** 4 096 bytes,
  always, with `MAX_PAYLOAD` derived from what is left rather than chosen. A
  ladder of buckets was rejected: it still tells a host which bucket, every
  boundary in it is a number somebody picked, and two builds that picked
  differently split their users into two distinguishable populations. PADMÉ
  (Nikitin et al., PETS 2019) is the published answer where sizes span orders of
  magnitude and is the wrong one here, because a segment is capped at one drop in
  the first place and the whole range fits in a single bucket.
- **The pad is checked, not skipped.** Unchecked padding is a perfect covert
  channel — inside the authenticated envelope, exactly as long as the message is
  short, and never looked at again. A patched build could ship an identity seed
  out through it at a kilobyte a message with every test still green. A non-zero
  pad is refused.
- **A host describes nothing.** `GET /health` and its capability banner are
  deleted, every refusal has an empty body, and a caller who holds no address
  gets one identical `404` to every question. The banner had never been evidence
  — `doctor` measures — and its only caller in the workspace was the test that
  asserted its text, so what it cost was a one-request answer to "is this a
  kusanagi host", which is the most useful thing a scanner could be handed.
- **Only headers ordinary traffic already carries.** `X-Kusanagi-Ttl` became
  `Cache-Control: max-age`, and the client sends no `User-Agent` at all. A header
  named after this project announces it to every proxy and log on the route,
  including the ones inside TLS. Sending a browser's agent string instead was
  rejected: a browser header above a TLS handshake that is plainly not a
  browser's is a worse tell than silence.
- **The invitation is not an argument.** It carries the channel secret and a
  signing key, and on Linux any account can read another process's command line
  out of `/proc`, after which the shell keeps a copy. `join` reads it from stdin
  and there is no second way in — two ways would mean the leaking one stays the
  default.
- **A site is readable by its owner and nobody else.** `0600` on every file,
  `0700` on every directory, set at creation and again after each write so that
  an older build's file is corrected rather than inherited. The attacker this
  answers is not a nation state; it is a second account on a shared build machine.
- **Secrets erase themselves, and cannot be compared.** `Secret`, `Stream` and
  `Key` are `ZeroizeOnDrop`, so a channel secret does not outlive its value in
  freed memory, a core dump or a swap file. They also lost `PartialEq`: nothing
  compares two secrets, and a derived comparison would run in a time that depends
  on how many leading bytes match. `zeroize` was already in the tree beneath
  `ed25519-dalek`, so taking it directly added nothing to audit.
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
| **Cover traffic** — property 4b | the largest gap in §3 and the one the rest now depends on. An endpoint emits requests only when there is something to say, so the rhythm of a conversation survives everything else here. What is needed is traffic that does not depend on whether anybody is talking. Constant rate is *not* the answer and was rejected while being designed: a lone endpoint emitting a fixed beat is more conspicuous than one that says nothing, because cover only works when the cover distribution is the ambient one |
| **Riding a carrier** | the way property 0 closes for a path observer, and the way 4b closes with it. Drops written into a store that already receives opaque high-entropy blobs on a schedule — an encrypted backup repository, a container registry — by invoking the real client rather than imitating it. Nothing here imitates a protocol today, and nothing should start: see §3 on why mimicry loses |
| **TLS fingerprint** | a `rustls` handshake is identifiable as one (JA3/JA4). Closing it needs handshake mimicry with no clean answer in the Rust ecosystem, and it is second in line behind the carrier, which would make the handshake a real client's |
| **Forward secrecy** | one static channel secret decrypts everything, forever, for whoever takes a site. A per-epoch ratchet is the answer and needs a decision about law 1 first: a ratchet that has moved cannot re-read what it advanced past |
| **On-disk deniability** | mode bits stop another account; they do not stop somebody holding the disk, who finds a 32-byte identity, 73-byte cairns and fixed-offset channel records whether or not the files are named |
| **Windows file permissions** | `0600` has no counterpart; restricting a file there means writing an ACL, which needs an API this workspace cannot reach without `unsafe` or a crate that brings one |
| `Bell` | **no longer only an optimisation.** A reader that polls names the address it waits on, then the next one after a hit, so a host watching one endpoint can follow the live edge; a host that can be asked to wait is told one address instead. Riding a carrier that bulk-syncs would close the same leak more completely and make this unnecessary; whichever lands first decides whether the other is built |
| `Cohort` — rosters and epochs | needs multi-node test infrastructure; two parties do not need a roster |
| `Depot` — chunked content | now load-bearing rather than optional: a drop carries 3 935 bytes, so anything larger needs chunking rather than a bigger envelope |
| `port` — local socket and MCP front ends | the verb set is one enum, so a second front end is additive |
| Post-quantum signatures | **the data path is already post-quantum confidential**, and this is worth stating rather than leaving as an absence: confidentiality rests on a pre-shared secret through BLAKE3 and ChaCha20-Poly1305, with no public-key exchange anywhere in it, so harvest-now-decrypt-later does not apply. What is classical is Ed25519 authorship, and §8 records with numbers why replacing it today would cost 92% of a message and the one-line invitation. Revisit when FIPS 206 is final or when a drop is large enough that SLH-DSA fits |

---

*This document is licensed MPL-2.0.*
