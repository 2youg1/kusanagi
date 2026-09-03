# ARCHITECTURE

The authority for what kusanagi is and why it is shaped this way. Where this
document and the code disagree, the code is wrong; where reality disagrees with
this document, this document is corrected first, with its reason. Crate detail
lives in `crates/<crate>/<crate>-SPEC.md`, working rules in `AGENTS.md`.

---

## 1 The claim

**Two agents on two machines exchange authorised, mutually unlinkable messages
through a host that neither of them trusts and neither of them runs.**

Every design decision below answers to that sentence, and a change that does not
serve it changes the project rather than the code.

## 2 The substrate is a dead drop, not a connection

A sender leaves a segment at an opaque address and a reader collects it later:
no session, no presence, no rendezvous.

**Why this rather than an overlay network.** Three reasons, the first decisive.

1. **The host already exists.** Anyone who wants a shared workspace has already
   accepted an object store or a company server, so building NAT traversal and a
   membership protocol is paying for a world you are not in.
2. **Offline is the normal state.** An agent thinks for tens of seconds and may
   then be idle for hours. In a dead-drop model "the other side is offline" is
   the resting state rather than a case to handle, and costs no code at all.
3. **One mechanism, two speeds.** Direct delivery is demoted, not deleted: it is
   a waypoint whose implementation happens to be the peer's own process, so the
   fast path is a special case of the slow path rather than a second protocol.

The price is a latency floor of one polling interval, paid by a consumer that thinks for seconds.

## 3 What "private" means here, exactly

**Against whom, first.** Four adversaries, and every claim below names which one
it answers: **the host**, which holds the bytes and keeps an access log, assumed
hostile from the start — that assumption is the whole of §2; **somebody on the
path**, an ISP or a proxy or a middlebox, who sees requests leave an endpoint and
cannot read what is in them; **somebody scanning**, who holds no address and is
looking for hosts and users to add to a list; and **somebody who acts**, who blocks
an address, serves an order on a host, knocks on a door, and takes the disk
afterwards. The fourth is not a stronger version of the other three: they read, while
this one changes what is reachable and reaches the person, and most of what answers it
is outside this program.

All four are assumed to hold **this repository**, which is not the ordinary
Kerckhoffs assumption: a public implementation of a hiding mechanism is a
supervised learning problem with unlimited labelled data, where an adversary runs
our binary as often as they like, labels both sides, and trains a classifier. Any
residual difference that is not cryptographically negligible will be found. Two
rules follow. **Nothing here imitates a protocol**, because imitation must also
imitate error handling, retries and quirks, and one discrepancy is enough. And
**indistinguishability is measured rather than argued**, which is law 6 in §7.

**And assume the compute is unbounded.** The consequence is precise rather than
rhetorical: **every computational guarantee here fails against it.**
ChaCha20-Poly1305, BLAKE3's key derivation and the signature scheme are
computationally secure and nothing else, so an unbounded adversary recovers a channel
secret by search and *verifies* the guess against two addresses — even unlinkability
falls. Only a one-time pad repairs that, at as much key material as traffic plus a
consumed position on disk, which is a different network. What survives is the class of
observables that are **not a function of any secret** — not hard to invert, not a
function at all: every drop is one size, so the mutual information between what a host
measures and how much was said is zero for an adversary of any size. That is why the
`Kind` column is ranked as it is — **unbounded compute is useless against bytes nobody
collected**, and the information-theoretic rows exist to deny an adversary the
ciphertext their compute would be spent on.

Seven separate properties. Claiming them as one word is how a privacy claim becomes false.

| # | Property | Kind | Today | By what | Asserted by |
|---|---|---|---|---|---|
| 0 | That this network is in use at all | information-theoretic once built | **leaks to a path observer, closed against a scanner** | a host answers a stranger exactly as a static file server does, and no request or response names this project; but an endpoint's own traffic is still traffic nobody else generates | `box/tests/unmarked.rs`, `waypoint/tests/unannounced.rs` |
| 1 | Content confidentiality | computational | **held until the compute is unbounded** | ChaCha20-Poly1305 under a key used for exactly one message | `seal`'s envelope tests |
| 2a | Who talks to whom, in what the host **stores** | computational | **held until the compute is unbounded** | every address is `KDF(secret ‖ author ‖ height)`; no address is ever reused | `kusanagi/tests/unlinkable.rs` |
| 2b | Who talks to whom, in what the host is **asked for** | computational | **held for a poll**, leaks on a catch-up | a reader resumes from a cairn, so a poll names one address instead of the stream | `kusanagi/tests/unwatched.rs` |
| 3 | Network size | information-theoretic once built | leaks the number of objects | nothing yet | `adversary/`, as an equality against a declared list |
| 4a | Traffic analysis — **how large** | **information-theoretic** | **held** | every sealed drop is one size whatever it carries, so the observation is a constant | `adversary/`, and `seal`'s envelope tests |
| 4b | Traffic analysis — **when, and how often** | information-theoretic once built | **leaks** | nothing; cover traffic is §9 and is the largest thing still missing | nothing |

Rows 0, 3 and 4b are marked "once built" because the mechanism that closes each makes
an observation independent of the secret rather than expensive to invert. Filling every
slot whether or not anybody is talking makes the count and the rhythm functions of the
clock alone — and **that schedule should then be public and deterministic**, because
hiding it is a computational defence an unbounded adversary strips away, while filling
every slot survives being fully known.

Property 2 is the one this project exists for, and splitting it in two corrects a real
error rather than refining one. Addresses derived to be unrelated stop being unrelated
the moment one connection asks for them in ascending order, back to back — which is
what a reader that began at height zero on every read did, once per poll, for the whole
history. **The derivation was sound and the reading path gave the answer away.** A host
needed no cryptanalysis, only an access log.

The last column is the point of the section, because a privacy claim nobody runs is a
paragraph. `adversary/` builds paired worlds — four where a byte is said, four where
three thousand are — measures ten features of what the host is left holding, and fails
when any threshold separates them by more than their own spread, because a stump that
separates two groups *is* the rule a censor deploys. Exactly two features may separate
a silent channel from a busy one, both the object count in different units, and the
assertion is an equality: one that starts separating is a regression, one that stops is
a leak somebody closed and has to say so.

**What no property above protects, named so it cannot be mistaken for covered.** The host
learns the **IP** of every endpoint that reaches it; a path observer reads the **DNS
query** and the **TLS SNI** before the connection. No address derivation touches those,
and the answer to them is a network built for the question plus an interface to it, not a
mechanism of our own (§8). Deleting old drops is carried out **by the host**, so it is
hygiene and the guarantee needs the ratchet in §9. **Coercion is out of scope**: this
denies proof of what somebody said, not somebody held until they produce a key.

Two consequences of property 2 that were not designed for:

- **Unsolicited delivery is uncomputable, not filtered.** Writing to somebody needs their
  address, deriving it needs the secret, and holding the secret needs an introduction:
  there is no door to knock on.
- **A one-time invitation needs no bookkeeping.** The greeting an invitee writes lands at
  a write-once address, so the *host* refuses the second acceptance.

**What the host still learns**, stated so it cannot grow quietly: the number of objects
it holds, the time of every request, and the IP that made it. It never learns their
size, their content, their authorship, a relationship, or that a relationship exists.

**What an endpoint's own disk gives up** is a separate question. A site cannot avoid holding
the identity seed and every channel secret, so the claim is checkable rather than broad: it
holds **no message** (`kusanagi/tests/at_rest.rs`), and **no channel is stored under the name
of the person it is with** — a record is filed under a keyed hash of that name, so a listing
gives up a count and not a graph (`site/tests/site.rs`). On Unix nothing it writes is readable
by another account (`site/tests/unreadable.rs`); on Windows nothing does that yet.

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
| **Trail** | one author's private sequence of one-time proofs for one stream: each segment shows the current proof and commits to the next | a peer can check who wrote a message and can never prove it to anybody else |

Reserved for work not yet done, and therefore **not** in the code: `Bell`, `Cohort`, `Depot`.

## 5 The crates

```
kernel      identifiers, identity, signed segments, canonical bytes, seams
  chain     the rules a sequence of segments must satisfy
  seal      address derivation and content sealing
  grant     issue, attenuate, verify, revoke
  waypoint  directory / memory / HTTP / S3, conformance, probe — how to reach a host
    box     the box server — how to be one
  site      one endpoint's own disk: identity, channel records, invitations
  door      what a verb answers and how a failure recovers
kusanagi    the verbs and the one assembly point
```

Dependencies point one way: `kernel` depends on nothing of ours, `kusanagi` on all.

**Budget.** Three limits, one purpose, all held by `just budget`:

| Limit | Applies to | What it bounds |
|---|---|---|
| **400 lines** | every file in the repository, whatever kind it is | how much has to be in your head to judge one line |
| **2,500 lines** | each crate's `src/` | how large one idea may grow |
| **25,000 lines** | the workspace, tests included | how much there is to read at all |

The file is the unit that actually gets opened — a reviewer opens one, an editor
jumps into one, a model reads one — so the first limit is the strictest and the one
that decides whether this code can be read. **A file over 400 lines is split or
deleted; the number is not raised**, and splitting the assertions about a rule away
from the rule is a legitimate answer, because the workspace total counts tests and
nothing is hidden by moving it. The limit was a ladder until it was one number —
400 for `.rs` and `.hs`, 500 for `.md`, 300 for the rest — whose 300 rung never
once refused a file and whose 500 rung had exactly one effect: it let this document
run to 479 lines. A limit nobody applies from memory is a limit met at the gate.
`Cargo.lock` and `LICENSE` stay outside the count because their contents are not
written here, and that list is closed; what each crate spends is what `just budget`
prints, deliberately not copied here, because a number kept in two places drifts.

Three crates exist because the budget forced a split, each recorded where it
happened. `kusanagi` gave up the disk formats to `site` (`site-SPEC.md` §3) and the
output contract to `door` (`door-SPEC.md` §4) — **a `SiteError` says what failed on
the disk, and the door says what it is called and how to recover from it.**
`waypoint` gave up the server to `box`, **overturning `waypoint-SPEC.md` §7**, which
had ruled that the two halves of the box protocol share a crate: the 2,500-line
limit did not exist when that was taken, and separating two jobs — *reaching* a host
and *being* one — beat separating two implementations of one seam. What that
decision feared, the halves drifting apart, is held by a test: the box's own tests
drive the shipped client against the shipped server and run `conformance::run` on it.

**Outside the workspace.** `adversary/` is a Haskell counterexample hunter — not a
crate, not a dependency, not part of the release, not counted here. §8 records why
it may exist and what stops it becoming a second authority.

## 6 The seams

A seam with one implementation is a description of that implementation. Every one
of these ships with a second, and with a contract both must satisfy.

| Seam | Declared in | Production | Second | The contract |
|---|---|---|---|---|
| `Waypoint` | `kernel::waypoint` | S3 bucket, HTTP box | directory, memory | `waypoint::conformance::run` — a function, so `doctor` can run it against a live host |
| `Conditional` | `waypoint::conditional` | HTTP box, S3 | directory answers "not offered" | `waypoint::probe::examine` |
| `Clock` | `kernel::clock` | `kusanagi::world::SystemClock` | `kernel::FixedClock` | one sampling point, held by `clippy.toml` |

**Deliberately not seams.** `chain`, `grant` and `seal` have one implementation
each because they *are* the rules, and an interface over a rule invites a second
authority for it.

## 7 The laws

1. **No resident state, and no local fact a host could not confirm.** Every verb is a
   one-shot command that exits. An endpoint's height comes from the waypoint, and the
   cairn beside it moves where a walk *starts* without deciding what the walk *concludes*
   — the first segment read must link to the cairn's head, so a resumed walk proves its
   own join. Deleting every cairn therefore changes what a read costs and never what it
   reports, asserted by `losing_every_cairn_changes_what_a_read_costs_and_nothing_else`
   and, for the rest, by `a_command_keeps_no_state_that_a_kill_could_lose`. **The one
   exception is the point of having a cairn**: against a host that withholds or replaces a
   drop, an endpoint that has read before refuses what contradicts what it read and a
   first-time reader cannot, so two readers disagree and the one with a memory is right.
2. **Memory does not grow with the work.** `Verifier` holds one author and one head for a
   chain of any length, and `Segment::extend` takes a `ChainHead` rather than a
   predecessor. A change that buffers a chain has broken the design.
3. **One authority per rule.** Hex has one parser, time has one sampling point, randomness
   has one source, the verb set has one definition.
4. **A segment that exists was signed.** Both constructors sign and the decoder verifies,
   so no unverified-segment state exists for a caller to forget.
5. **Failures are typed and carry a way out.** Every error names the action, the subject,
   a stable code, and the command that recovers.
6. **Indistinguishability is measured, not argued.** §8 holds that hosts are measured
   rather than believed, and a public repository turns the rule on us: any claim that two
   situations look alike is an experiment somebody ran (§3).

## 8 Decisions on record

Reopening one of these requires a reason that did not exist when it was taken.

- **Identity is a hash, not a key.** `Handle` is `BLAKE3(public key)`, so address
  derivation, cairn filenames and the segment layout do not depend on which signature
  scheme is in use. **A name therefore checks nothing**, and the key travels only where a
  signature is checked: a grant carries the issuer's key in every step, because a
  credential must convince a stranger; a channel record carries the peer's key, because a
  stream need only convince the endpoint introduced to its author. A segment carries no
  key, so `Segment::from_canonical_bytes` is told whose signature it expects, which is why
  a stranger holding the ciphertext can verify nothing.
- **The signature scheme is ML-DSA-87 (FIPS 204).** Integer-only arithmetic, so a
  constant-time implementation is reachable, and a final standard. The parameter set is
  the strongest rather than the cheapest, by ruling, and `DROP` is sized to whatever it
  costs — so the price falls on an artefact nobody types: an invitation is a file, not a
  line to paste. **Not a QR code either**: that carrier holds 2 953 bytes, one signature
  is 4 627, and a measured one-hop invitation is 10 009, so no engineering brings the
  artefact within reach of it while this scheme stands. What makes an invitation checkable
  face to face is separating the 64 bytes that are secret from the 9 945 that are not — a
  change to the invitation, not to how it is carried.
- **Every segment after the first is authenticated by a Trail rather than a signature, and
  the genesis signature covers the author and the first commitment but not the payload.**
  A signature is transferable: a peer who is compromised or coerced holds not merely
  knowledge of what was said but proof of it that convinces anybody, forever, without the
  author's participation. So segment *i* reveals `secret_i` and commits to
  `H(secret_{i+1})`, a reader accepts it only when the reveal hashes to the previous
  commitment, and forging a segment or racing to a height before its author needs a
  preimage — while anybody who has read the stream can afterwards fabricate a different
  one that verifies exactly as well, which makes a quotation an assertion rather than
  evidence. The genesis signature says that this author opened this stream: enough to stop
  a peer racing to height zero, not enough to convict anybody of a sentence. A reader
  loses nothing, because the bytes arrive sealed under the channel secret at an address
  derived from it, so the only party who could put different words at a height is the peer
  who already read them — exactly the party who must not be able to prove what they were.
  `crates/chain/tests/deniable.rs` forges a whole transcript rather than asserting that
  one could be forged.
- **The sealed form is the whole segment, not its payload.** A segment carries its author
  in the clear, so sealing only the payload would let a host group drops by author and
  property 2 would be worth nothing.
- **Attenuation is a lattice meet, not a check.** Asking for more than you hold yields
  what you hold, so widening is unrepresentable rather than rejected. Sampled by
  `crates/grant/tests/attenuation.rs`; the `kani` harness proving it against real MIR is
  in `src/chain.rs` under `#[cfg(kani)]`.
- **Hosts are measured, not believed.** Conditional-write support diverges between
  S3-compatible hosts and the divergences fail *open*, condition ignored and write
  succeeding, so `kusanagi doctor` writes twice and reads back before issuing a
  certificate naming a tier.
- **`Tier::AckFirstSeen` is enforced by the cairn, not merely named.** The tier said both
  sides must remember what they first saw at an address and refuse anything later; for two
  versions nothing did, so the tier was a label and the cairn is now that memory. A host
  can still refuse to serve a drop, but it can no longer make a reader who has already
  read believe the stream is shorter — the difference between an outage and a retraction
  somebody never sent.
- **A head may come from this endpoint's own record, not only from a segment in hand.**
  `ChainHead::recorded` is a weaker provenance than holding the segment, admitted on one
  argument, and it goes if that argument ever fails: **every use of a head is a
  comparison, so a false head can only cause a refusal, never an acceptance.** Keeping the
  last segment instead needs no new constructor and was rejected anyway, because it would
  leave every channel's most recent message on disk forever — the line
  `crates/kusanagi/tests/at_rest.rs` holds.
- **The invitation is a bearer token.** Whoever writes one cannot know who will accept it,
  so it carries a one-time key the acceptor delegates to their own handle.
- **One cipher suite, mandatory.** Two endpoints with different derivations cannot compute
  each other's addresses, so a pluggable suite is a partition rather than a degradation.
  The invitation carries a suite byte and refuses one it does not know, making a suite
  change a network-wide flip — the migration path for everything below. **The byte moved
  to 1 when the signature scheme became ML-DSA-87**, because a build that still believed
  it knew suite 0 would have accepted and then failed on a key length, which is the one
  failure the byte exists to prevent.
- **Scale is layered, not flat.** Cohorts of about a thousand joined by transitive grants,
  because flat global reachability needs a globally resolvable name table and that table
  is a relationship graph.
- **Bell is a waypoint capability, not a protocol requirement.** A host that can long-poll
  needs no Bell and leaks nothing, so the alternative's cost falls only on whoever chose a
  dumb object store.
- **Every sealed drop is one size, and the size is not a parameter.** `DROP` is derived
  rather than picked: the smallest power of two holding the largest artefact this protocol
  can produce — an introduction, being a full-depth grant plus the newcomer's key under a
  genesis segment — with `MAX_PAYLOAD` whatever is left. Fewer, larger objects also leave
  a host fewer things to count and anybody on the path fewer requests to time. A size that
  varies is a measurement a host takes without cryptanalysis; a ladder of buckets is that
  measurement one step coarser, with boundaries that are parameters, and two builds
  holding different parameters are two distinguishable populations.
- **The pad is checked, not skipped, and checked in constant time.** Unchecked padding is
  a covert channel: inside the authenticated envelope, as long as the message is short,
  never looked at again. A non-zero pad is refused.
- **A host describes itself to nobody.** No banner, no status path, an empty body on every
  refusal, one identical `404` to every question from a caller who holds no address. A
  well-known path answering with a product name turns an internet-wide scan into a list of
  this network's users, so `doctor` measures a host rather than asking it.
- **Only headers ordinary traffic already carries.** `If-None-Match` for the conditional
  write, `Cache-Control: max-age` for a lifetime, no `User-Agent`. A header named after
  this project announces it to every proxy and log on the route, including those inside
  TLS, and a borrowed browser header above a handshake that is plainly not a browser's is
  a worse tell than silence.
- **Nothing that identifies anybody is an argument.** A command line is public: on Linux
  any account reads another process's out of `/proc`, and the shell keeps a copy. An
  invitation carries the channel secret, so `join` reads it from stdin and has no second
  way in; a channel name is worse, leaking who talks to whom on every message, so every
  flag that takes one accepts `-` and reads it from the first line of stdin instead.
- **A site is readable by its owner and nobody else, and no file in it is named after
  anybody.** `0600` on every file and `0700` on every directory — on Unix; Windows is the
  gap in §9 — established at creation and never adjusted after, because `set_permissions`
  follows symbolic links and a build that chmods a file it did not create can be aimed at
  one it did not choose; a replacement stages beside its target and renames over it. A
  channel is filed under a keyed hash of its name, so a listing gives up a count and not a
  graph. The attacker this answers is a second account on a shared machine, not a nation
  state.
- **Secrets erase themselves and cannot be compared.** `Secret`, `Stream` and `Key` are
  `ZeroizeOnDrop`, so a channel secret does not outlive its value in freed memory, a core
  dump or a swap file. None implements `PartialEq`, because a derived comparison runs in a
  time that depends on how many leading bytes match; every fixed-width identifier compares
  in constant time.
- **The adversary is out of the workspace and speaks only through the door a user has.**
  `adversary/` drives the shipped binary with `--json` and asserts *relations between
  traces*, never an expected output, because restating a rule is what a second authority
  is. Haskell earns the place because a lying host is a choice over a strategy space and a
  directed attack is "any prefix, then this, then any suffix", where uniform random
  generation is a fuzzer rather than an adversary. Four properties keep it from drifting:
  it enters through the door a person enters through, it states relations rather than
  behaviour, it never gates the Rust build, and **what it delivers is a Rust regression
  test**. Delete the directory and the network is unchanged.

**Deliberately not adopted.** Fuzzy message detection and oblivious message retrieval
solve delivery *without* a shared secret, and we always have one; MLS is a later `seal`
adapter, not a substrate; DHT or blockchain discovery publishes the relationship graph
this design exists to hide; mixnets and PIR conflict with the cost target — **so an
endpoint's IP goes to a network built for it, through a socket we are handed rather
than a network we would run.** `KUSANAGI_PROXY` names a SOCKS5 or HTTP CONNECT proxy,
a value that is not one is refused rather than ignored, and §3 states that boundary.

## 9 Not built

Named so that their absence is a decision rather than an oversight, each its own version.

| Missing | Why it waits |
|---|---|
| **Cover traffic** — property 4b | the largest gap in §3. An endpoint emits requests only when there is something to say, so the rhythm of a conversation survives everything else here. What closes it is traffic independent of whether anybody is talking — and it has to be traffic whose distribution is the ambient one, because a lone endpoint emitting a fixed beat is more conspicuous than one that says nothing |
| **Riding a carrier** | the way property 0 closes for a path observer, and the way 4b closes with it. Drops written into a store that already receives opaque high-entropy blobs on a schedule — an encrypted backup repository, a container registry — by invoking the real client rather than imitating it. Nothing here imitates a protocol today, and nothing should start: see §3 on why mimicry loses |
| **TLS fingerprint** | a `rustls` handshake is identifiable as one (JA3/JA4). Closing it needs handshake mimicry with no clean answer in the Rust ecosystem, and it is second in line behind the carrier, which would make the handshake a real client's |
| **Forward secrecy** | one static channel secret decrypts everything, forever, for whoever takes a site. A per-epoch ratchet is the answer and needs a decision about law 1 first: a ratchet that has moved cannot re-read what it advanced past |
| **On-disk deniability** | mode bits stop another account; they do not stop somebody holding the disk, who finds a 32-byte identity, 73-byte cairns and fixed-offset channel records — no longer under anybody's name, but still recognisable as what they are |
| **Windows file permissions** | `0600` has no counterpart; restricting a file there means writing an ACL, which needs an API this workspace cannot reach without `unsafe` or a crate that brings one. Both crates that would bring one are unmaintained, so which of them to trust is a supply-chain decision and not an implementation detail |
| `Bell` | a privacy mechanism rather than a latency tweak. A reader that polls names the address it waits on, then the next one after a hit, so a host watching one endpoint can follow the live edge; a host that can be asked to wait is told one address instead. Riding a carrier that bulk-syncs closes the same leak more completely, and whichever lands first decides whether the other is built |
| `Cohort` — rosters and epochs | needs multi-node test infrastructure; two parties do not need a roster |
| `Depot` — chunked content | optional again. `DROP` is sized to hold the largest artefact this protocol produces, so the signature swap did not need chunking after all; what still needs it is user content larger than one drop |
| `port` — local socket and MCP front ends | the verb set is one enum, so a second front end is additive |

---

*This document is licensed MPL-2.0.*
