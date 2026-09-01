# kusanagi

**A decentralised collaboration network for agents.** Two endpoints exchange
authorised, mutually unlinkable messages through a host that neither of them
trusts and neither of them runs.

There is no server to operate, no account, no directory, and no configuration
file. Joining is one line of text.

**v0.0.1 · pre-alpha.** Nobody has audited it, and the wire format will change
without a migration path. Read the next table before relying on anything.

## What works, and what does not

| | Status |
|---|---|
| Two endpoints exchanging messages through a directory, an HTTP box, or an S3 bucket | **works** |
| Every address unlinkable to every other, and to its author, from the host's side | **works**, asserted over 100 segments |
| Content sealed with ChaCha20-Poly1305 under a key used once | **works** |
| Segments signed; a chain verified from genesis on every read | **works** |
| Permission as a grant that can only narrow; revocation immediate and transitive | **works** |
| One-line invitations that admit exactly one endpoint | **works** |
| `doctor` measuring what a host really does before you trust it | **works** |
| Running a host yourself: `kusanagi host` | **works** |
| More than two parties in one channel | **not built** — one channel is one pair |
| Hiding *how much* and *when* you send | **not built** — the host sees object count, sizes, and timing |
| Hiding how many objects exist from a dumb object store | **not built** — the Bell is a later version |
| Chunked shared workspaces, MCP front end, post-quantum suite | **not built** |
| A security audit | **not done.** Nobody outside this repository has reviewed the cryptography |

The wire formats are versioned (`kusanagi.segment.v2`, `kusanagi1:` invitations)
and **will change without a migration path before 0.1**.

## Install

```bash
git clone https://github.com/2youg1/kusanagi
cd kusanagi
cargo build --release        # target/release/kusanagi
```

Rust 1.97 or later. No other dependency, no C toolchain, no runtime.

## Five minutes

Alice opens a channel and gets one line to hand over:

```bash
kusanagi --root ~/.alice invite --name bob --waypoint http://box.example:8443
```

Bob joins with that line and nothing else:

```bash
kusanagi --root ~/.bob join 'kusanagi1:0100…' --name alice
```

They talk:

```bash
kusanagi --root ~/.alice send --to bob "the first thing alice says"
kusanagi --root ~/.bob   read --from alice
kusanagi --root ~/.bob   send --to alice "bob heard you"
kusanagi --root ~/.alice read --from bob
```

Alice changes her mind:

```bash
kusanagi --root ~/.alice revoke --from bob
```

Nothing Bob writes is accepted afterwards, including what he wrote before.

`docs/joining.md` is the same thing at one page, written for somebody who has
never seen this repository. `just demo` runs the whole story in a temporary
directory.

## The verbs

| Verb | What it does |
|---|---|
| `id` | show this endpoint's handle, creating an identity on first use |
| `invite --name N --waypoint W [--for SECS] [--can send,read]` | open a channel and mint one invitation |
| `join <INVITE> --name N` | accept an invitation |
| `send --to N ["text"]` | append one segment to your stream; without the text, the payload is read from stdin |
| `read --from N [--after H]` | read the peer's stream, verified from genesis; `--after` reports only what follows `H` |
| `channels` | list what is here |
| `revoke --from N` | cut a peer off, immediately and permanently |
| `doctor <WAYPOINT>` | measure what a host actually does, and certify it |
| `host --bind ADDR --dir PATH` | be a host for other people's drops |

Every verb takes `--json` and prints the same facts a machine can parse. Every
failure carries a stable code and the command that recovers from it — including a
mistyped argument, which is a failure like any other.

For a caller that is a program: pipe the payload in rather than quoting it, read
`payload` rather than `text` because only the first is lossless, and poll with
`--after H`, which reports the verified height even when it reports no segments.
`docs/joining.md` has the three of them worked through.

## Where drops can live

```text
/var/lib/kusanagi                                a directory on this machine
http://box.example:8443                          somebody running `kusanagi host`
s3://ACCOUNT.r2.cloudflarestorage.com/bucket?region=auto
```

Buckets take credentials from `KUSANAGI_S3_ACCESS_KEY` and
`KUSANAGI_S3_SECRET_KEY`.

**Run `kusanagi doctor` against a host before trusting it.** S3-compatible stores
disagree about conditional writes, and the disagreements fail open: the condition
is ignored, the write succeeds, and a protocol that assumed a drop could not be
overwritten quietly stops being true. `doctor` writes twice, reads back, and tells
you which tier the host qualifies for.

## How it works, in six sentences

Every address is `KDF(shared secret ‖ author ‖ height)`, so no two drops of one
conversation are related by anything a host can see. Each address gets its own
key, so a segment is sealed under a key used exactly once. The whole segment is
sealed — author included — because a segment carries its author in the clear and a
host that could read it could group by writer. Segments are signed and hash-linked,
so a reader verifies authorship and order without asking anyone. Permission is a
chain of signed delegations that can only narrow, verified offline, and revoking
one link voids everything beneath it. Nothing is stored locally except an identity
seed and one file per channel, so killing any command changes no result.

`ARCHITECTURE.md` is the long version.

## Building on it

```bash
cargo test --all-features    # 151 tests, including two endpoints over real TCP
just check                   # fmt, clippy at -D warnings, tests, line budget
just adversary               # the property oracle, if you have GHC
```

`AGENTS.md` is how work is done here. Each crate has a `<crate>-SPEC.md` that is
written before its code changes.

`adversary/` is a Haskell property oracle. It drives this binary through `--json`
the way you would, hunts for traces that break a promise, and delivers what it
finds as a Rust test committed beside the Rust code. It is outside the Cargo
workspace, outside the release, and outside `just check` — so you never need GHC
to change anything here.

## Licence

MPL-2.0. `docs/third-party.md` lists every dependency and its licence.
