# AGENTS.md — how work is done in this repository

**For any agent or person about to change this code.** Read this file to the end before the first edit. It is short on purpose, and everything it does not cover is one link away.

kusanagi is a decentralised collaboration network for agents. The substrate is a **dead drop**, not a connection: a sender leaves a segment at an opaque address, a reader collects it later, and the host that holds the bytes is never trusted. Direct delivery is an optimisation of that, not the other way round.

Every rule below exists for one reason that applies to a person just as much: **a contributor who does not remember yesterday still has to produce work that holds.**

## The loop

```bash
just check      # fmt + clippy (-D warnings, --all-targets --all-features) + tests + budget + cargo-deny
```

**A change is finished when `just check` is green.** "I finished it" is a claim; a green run is the evidence.

| Command | What it does |
|---|---|
| `just check` | the closing condition for every change |
| `just fmt` | apply rustfmt |
| `just lint` | clippy alone |
| `just test` | tests alone |
| `just deny` | licences, advisories and banned crates, against `deny.toml` |
| `just demo` | two identities, one host, one verifiable exchange, in a throwaway directory |
| `just budget` | line counts against the budget in `ARCHITECTURE.md` §5 |
| `just dist` | a stripped release binary and its SHA-256 |
| `just adversary` | the Haskell counterexample hunter in `adversary/`. **Not part of `just check`**, needs GHC, and skips itself when GHC is absent |

## Read before you write

1. [`ARCHITECTURE.md`](ARCHITECTURE.md) — what this is, why the dead drop is the substrate, the words, the crate graph, the seams, the line budget, and the decisions already taken.
2. The SPEC of the crate you are touching, `crates/<crate>/<crate>-SPEC.md` — or `adversary/adversary-SPEC.md` outside the workspace. **It is written before the code and changed before the code changes.**
3. The tests next to the code you are about to change.

## One change, five steps

1. **Take one piece of work.** One session, one bounded change; read its context in full before starting.
2. **Write the SPEC first.** Interfaces and decisions land in the crate's SPEC before the code exists. The section order is fixed; see any existing SPEC.
3. **Red.** Write the failing test and **run it once to watch it fail**. That run is what proves the test can bite.
4. **Green.** Implement until it passes, no more. When the implementation wants to differ from the SPEC, **change the SPEC first and say why** — there is a worked example in `kernel-SPEC.md` §8 (`extend` replacing `follows`).
5. **Close.** `just check` green | SPEC and code in step | the line budget still met.

When a session ends with work unfinished, write what is left — where you got to, what blocked you, what comes next — into the SPEC section it belongs to.

## The rules a machine holds

Violating any of these turns the build red.

| Write it this way | Held by |
|---|---|
| Return failure through `Result`. No `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`, bare indexing or slicing in non-test code. Checked arithmetic. No `as` casts. No `unsafe` anywhere. | `[workspace.lints]` in `Cargo.toml`, with `-D warnings` |
| Every suppression carries `reason = "…"`. In non-test code a suppression is allowed **only** for a lint on the allowlist written in `Cargo.toml` — which currently holds exactly one entry, `clippy::disallowed_methods`, at the one function that reads the clock. | `allow_attributes_without_reason = "deny"`, plus review |
| Test code (`#[cfg(test)]`, `tests/`, `benches/`) relaxes freely with a scoped `#[allow(…, reason = "test code")]`. | the tier policy above |
| One module, one file, semantically named. `lib.rs` holds the module index and nothing else. | review |
| Default to private. A `pub trait` is a seam, and **a seam ships with two implementations plus a conformance suite**. One adapter is a hypothetical seam; two make it real. | review, and `waypoint::conformance` as the worked example |
| Start every `.rs` file with the MPL-2.0 notice and the copyright line. | review |
| Take the time as a parameter. The single sampling point is `kusanagi::world::sample`, and randomness has one source beside it. | `clippy.toml` disallowed methods |
| One name per concept, taken from `ARCHITECTURE.md` §4. A word with no implementation does not enter the code. | review |
| Keep every file under **500 lines**, each crate's `src/` under **2,500**, and the whole workspace, tests included, under **25,000**. A file over the limit is split or deleted — the limit is not raised. | `just budget` |
| Anything hashed or signed is encoded by hand. `serde` is for `--json` output and nothing else. | review |

**Two laws that are not lints, and matter more than any of them:**

- **No resident state.** Every verb must work as a one-shot command that exits; killing any process changes no result. The CLI holds this by construction — it discovers a stream's height from the waypoint, never from a local file. Asserted by `a_command_keeps_no_state_that_a_kill_could_lose`.
- **Memory does not grow with the work.** `Verifier` holds one author and one head for a chain of any length; `Segment::extend` takes a `ChainHead`, not a predecessor. A change that buffers a whole chain is a change that broke the design, not one that needs more memory.

## Language

| Where | Language |
|---|---|
| Identifiers, error codes, rustdoc, comments, commit subjects | English |
| `README.md`, `AGENTS.md`, `ARCHITECTURE.md` | English |
| Crate SPECs and design discussion | Chinese, with concept names kept in their English form |
| Pull requests, issues, review comments | **your own language.** A parallel translation is welcome, not required |

Comments are a failure signal by default. Four kinds earn their place: the MPL notice, public interface documentation, a warning about consequences, and a statement of intent the code cannot carry. In rustdoc, write what the signature cannot say — invariants, failure modes, call ordering.

## Where the authorities are

1. The person's ruling.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md).
3. `crates/<crate>/<crate>-SPEC.md`.
4. The code and its tests.

Where a lower level contradicts a higher one, the higher wins and the lower is corrected in the same change. **Where reality contradicts all of them, reality wins and the document is corrected first, with its reason.**
