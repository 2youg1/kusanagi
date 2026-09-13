# AGENTS.md — how work is done in this repository

**For any agent or person about to change this code.** Read it to the end before the first edit; what it does not cover is one link away. An agent that wants to *use* kusanagi rather than change it reads [`LLM.md`](LLM.md) instead.

Kusanagi is a decentralized collaboration network for agents, built for privacy. The substrate is a **dead drop**, not a connection: a sender leaves a segment at an opaque address, a reader collects it later, and the host that holds the bytes is never trusted. Direct delivery is an optimisation of that, not the other way round.

Every rule below has one reason, and it applies to a person just as much: **a contributor who does not remember yesterday still has to produce work that holds.**

Speak to the person in the language they use. To the other contributors — a pull request, an issue, a review comment — write Chinese and English side by side, in the words of [`GLOSSARY.md`](GLOSSARY.md), short enough to read once; you answer for the two versions saying the same thing.

## The loop

```bash
cargo install just cargo-nextest --locked   # once; the toolchain installs itself from rust-toolchain.toml
just check                                  # fmt + clippy (-D warnings) + tests + budget + boxes + glossary + cargo-deny
```

**A change is ready for the person when `just check` is green.** "I finished it" is a claim; a green run is the evidence — and only the agent's evidence about its own work. **Agent-green is not done**: the tests prove the change holds, not that the feature works for a person. A feature counts as implemented after the person has run it by hand and said so; until then report it as *agent-green*, never as *done*.

| Command | What it does |
|---|---|
| `just check` | the closing condition for every change |
| `just quick [crates]` | fmt, clippy and tests for the crates git says you touched — the inner loop, never the closing one |
| `just test` | the workspace under `cargo nextest` |
| `just budget` | the line budget, which is `scripts/budget.sh` and nothing else |
| `just boxes` | where a test may stand, and what may follow it into a release |
| `just glossary` | one name per concept, which is `scripts/glossary.sh` and nothing else |
| `just deny` / `just deps` | the supply chain |
| `just demo` | the whole story in a throwaway directory |
| `just adversary` | the Lean counterexample hunter; never a gate, and a no-op without `lake` |
| `just repro` / `just dist` | the reproducible build, and the deliverable |
| `just unconfine` | undo what `docs/confine.md` set up |

**Inner loop.** `cargo fmt --all` and workspace-wide clippy at `-D warnings` after every step, because a warm cache answers both in seconds; `cargo nextest run` and `cargo build` scoped to the crate you edited and the dependents `cargo tree --workspace -i` reports, then whole once before closing. `just quick` is that loop for the crates git reports changed. `grant`'s two proptests take about two minutes and run only when `grant` changed or at closing; `cargo nextest run --workspace -E 'not package(kusanagi-grant)'` is everything else, in about thirty seconds.

**Nothing is pushed that fails `cargo fmt --all -- --check` or `cargo clippy --all-targets --all-features -- -D warnings`.** They are the first two steps of the check lane, and a push that fails them is a red run a reviewer waits on. Run both on the exact tree being pushed, a work-in-progress branch included.

- One tool call lasts 150 s: what fits in one call runs in the foreground; release builds, the full adversary suite and `grant`'s proptests go to the background.
- Be patient with a Rust command and never kill it by PID. The lock makes it slow; that is expected.
- Time a build that feels slow before tuning it. The seconds sit in particular compilation units, not in the breadth of the command.
- The window is judged by driving it — `native test`, the automation server, `glass/real.ps1` for real pixels — never by reading `.native` source and imagining the result.

**Three lanes, each red for exactly one reason.** `ci.yml` is the gate: red means *this change* broke something, so it blocks the merge, and nothing that can go red for another reason may enter it. `release-ready.yml` answers whether `main` can be tagged — the third platform, `cargo-deny`, the reproducible build, Nix, the window — and blocks a tag but never a merge, because a contributor fixing Rust cannot be asked to own a GUI toolchain. `sentinel.yml` runs on the clock against code nobody changed, so its red is the world moving underneath us; it opens one issue and updates it, because a red run that recurs every morning is a red run nobody reads.

**No tag before `release-ready.yml` is green on `main`** and the open issues and pull requests are triaged to zero or deferred with a reason. A red lane or an untriaged list means the tag waits.

## Read before you write

1. [`ARCHITECTURE.md`](ARCHITECTURE.md) — what this is, why the dead drop is the substrate, the crate graph, the seams, the laws, the line budget, and the decisions already taken.
2. [`GLOSSARY.md`](GLOSSARY.md) — the words, the one declaration behind each, and the names a thing is not called by.
3. The SPEC of the crate you are touching, `crates/<crate>/<crate>-SPEC.md` — `adversary/adversary-SPEC.md` and `glass/glass-SPEC.md` outside the workspace. **It is written before the code and changed before the code changes.**
4. [`docs/codes.md`](docs/codes.md) — every stable failure code. A new code is added there in the same change.
5. The tests next to the code you are about to change.
6. **The official documentation of a tool before you use it** — a language feature, a crate, a CLI. Load the vendor's own agent guide or skill when one exists.

## One change, five steps

1. **Take one piece of work.** One session, one bounded change; read its context in full before starting.
2. **Write the SPEC first.** Interfaces and decisions land in the crate's SPEC before the code exists. The section order is fixed; copy it from any existing SPEC.
3. **Red.** Write the failing test and **run it once to watch it fail**. That run is what proves the test can bite.
4. **Green.** Implement until it passes, no more. When the implementation wants to differ from the SPEC, **change the SPEC first and say why** — `kernel-SPEC.md` §8 has the worked example (`extend` replacing `follows`).
5. **Close.** `just check` green | SPEC and code in step | the line budget still met | the commit written as [`CONTRIBUTING.md`](CONTRIBUTING.md) says.

Unless the change is mechanical, keep the diff small enough to review in one sitting. When it is larger, find the smallest coherent stage that can land on its own and say what the remaining stages are — from the actual diff and its call sites, not from a guess about what looks separable.

## Rust

- Return failure through `Result`. No `unwrap`, `expect`, `panic!`, `todo!`, `unreachable!`, bare indexing or slicing in non-test code.
- Use checked arithmetic and `TryFrom`. No `as` casts.
- No `unsafe` outside `vault::windows`, which hands the operating system a security descriptor: one FFI call per block, one `// SAFETY:` line each. A `SAFETY:` line gives **the precondition that makes the call sound**; test it by asking whether it could be false. *"We call `SetSecurityInfo`"* cannot be false, so it is a restatement; *"the descriptor outlives the call and no other handle aliases it"* can be, so it is a precondition.
- Do not erase a failure: no `let _ =` on a `Result`, no `unwrap_or_default` standing in for a decision, no `.ok()` that drops the reason a caller needed.
- Suppress a lint with `#[expect(reason = "…")]` at the narrowest scope, never `#[allow]`. An `expect` that stops firing fails the build, which is how the suppression cleans itself up.
- Make `match` exhaustive; avoid wildcard arms. Prefer an enum to a `bool` parameter, and a typestate or newtype that makes the invalid state unrepresentable to a runtime check that rejects it.
- Give an error the failed action, the subject, a stable code, and a recovery the caller can act on.
- Take the time as a parameter. The single sampling point is `kusanagi::world::sample`, and randomness has one source beside it.
- Anything hashed or signed is encoded by hand. `serde` is for `--json` output and nothing else.
- Inline `format!` arguments: `format!("{name}")`, not `format!("{}", name)`.
- A comment is a failure signal by default, in every language here. Four kinds earn their place: the MPL notice, public interface documentation, a warning about consequences, and a statement of intent the code cannot carry. In rustdoc, write what the signature cannot say — invariants, failure modes, call ordering.

## Lean

`adversary/` is the only Lean in the tree, and Lean is the only language beside Rust that states a claim about this program. It is a Lake package outside the Cargo workspace, outside the release, and outside `just check`.

- **It takes no dependency outside the Lean toolchain.** The JSON reader, the subprocess, the temporary directory, the monotonic clock, the splittable generator and the TCP socket all ship with Lean, so there is no registry to reach and nothing to resolve; the generator, the shrinker, the state model and the dynamic logic are in-tree for the same reason — `Kusanagi/Check.lean` and `Kusanagi/Dynamic.lean`. **Never add a `require`.**
- Total functions only. No `panic!`, no `!` indexing, no `Option.get!`. Use `xs[i]?` and handle the `none`.
- `lakefile.toml` sets `warningAsError = true`. A warning is a failure.
- Start every `.lean` file with the MPL-2.0 notice and the copyright line, like every `.rs` file.
- The suite names the version it needs in `adversary/lean-toolchain`, and `elan` reads that file. Do not pin a toolchain anywhere else.

## Where code goes

| Language | Where it may go |
|---|---|
| Rust | all domain logic, every crate |
| Lean | black-box claims in `adversary/` only — no linking, no FFI, one subprocess and two streams |
| Zig | the `glass/` UI state machine only — zero domain rules; every action is one `fx.spawn` of a CLI verb |

- One module, one file, semantically named. `lib.rs` holds the module index and nothing else; `adversary/Kusanagi.lean` is the same index for the Lean side.
- Keep every text file under **400** lines (**440** for Markdown, since a SPEC that records one more decision is not split the way a module is), each crate's `src/` under **5,000**, and the sum of every crate's `src/` under **25,000**. A binary carries no lines. Tests sit outside the workspace total, which is why a black-box claim belongs in `adversary/` and a white-box one in Rust; every test file still answers to the per-file limit.
- A file over the limit is split or deleted; **the limit is not raised.**
- No `utils`, `helpers`, or `common` module. Name a module for what it owns.
- Do not create a helper referenced once, or a wrapper that only renames what it calls.
- Default to private. A `pub trait` is a seam, and **a seam ships with two implementations plus a conformance suite**. One adapter is a hypothetical seam; two make it real — `waypoint::conformance` is the worked example.
- **Give every rule, state transition and decision exactly one authority.** Two places that decide the same thing is a defect while they still agree.
- Finish a migration inside one change-set: move every reader and writer, exercise the production path, then delete the old authority and its adapters.
- Trace callers, data flow, invariants and failure paths before you touch a shared interface.
- One name per concept, taken from [`GLOSSARY.md`](GLOSSARY.md). A new concept enters that table in the same change as its declaration, a word with no declaration does not enter the code, and the Lean and Zig sides coin no name of their own.

## Tests

- **A black-box claim is written in Lean; a white-box claim stays in Rust.** A Rust test links the library, so a black-box claim written there is one refactor away from quietly reaching inside and still being called a test of the door. `adversary/` cannot reach inside by construction: no linking, no FFI, no shared type, one subprocess and two streams.
- **A test never reaches the build artefact.** A `mod tests` carries `#[cfg(test)]`, a crate that exists only to test with never appears on a normal dependency edge, and the shipped file carries no such crate's name.
- Test code (`#[cfg(test)]`, `tests/`, `benches/`) relaxes lints freely with a scoped `#[allow(…, reason = "test code")]`. Production code carries them as written.
- Prefer comparing whole objects to comparing fields one at a time. Do not test a statically defined value.
- **A test that is known to fail never goes in a gate.** It costs every contributor the same red on every run until they learn to skip past it, and the day it fails for a second reason nobody looks. Carry the gap as `#[ignore]` with the reason, and put what would settle it in the SPEC.
- **When the adversary finds a defect, the knowledge migrates.** The minimised trace is rendered as a Rust test under `crates/kusanagi/tests/`, and the Lean side does not keep it: `adversary/` quantifies over traces, and is not a second home for a fact.
- `just check` on a machine without Lean behaves byte for byte as it does where `adversary/` is absent. Never make `just check` depend on it.

## The rules a machine holds

Violating any of these turns the build red.

| Rule | Held by |
|---|---|
| The Rust rules above: no panics, checked arithmetic, no `as` casts, `unsafe` only in `vault::windows`. | `[workspace.lints]` in `Cargo.toml`, with `-D warnings` |
| Every suppression carries `reason = "…"`. In non-test code a suppression is allowed **only** for a lint on the allowlist written above `[workspace.lints]` in `Cargo.toml`, each entry at the one site it names. | `allow_attributes_without_reason = "deny"`, plus review |
| The clock and the random source reached from their one address each. | `clippy.toml` disallowed methods |
| A black-box claim in Lean under `adversary/`; a white-box claim in Rust beside the code it judges. | `just boxes`, which refuses `CARGO_BIN_EXE` anywhere under `crates/` |
| Nothing written for a test compiled into the binary a person downloads. | `just boxes` |
| Per-file 400 (440 Markdown), per-crate 5,000, workspace 25,000. | `just budget`, which is `scripts/budget.sh` and the only authority for those numbers |
| One name per concept: every word in `GLOSSARY.md` declared where it says and as a type nowhere else; a rejected or reserved name declared nowhere. | `just glossary`, which is `scripts/glossary.sh` and the only authority for it |
| A number a document states — the size of a drop, the largest message, the default port, the suite byte — is the number the code gives. | `crates/kusanagi/tests/documented.rs` |
| The MPL-2.0 notice then the copyright line at the top of every `.rs` and `.lean` file. | review |

**Adding a row to the suppression allowlist requires an explicit ruling from the person, recorded as a `Verdict:` trailer.** That is the one escape hatch, and this is what closes it.

**Two laws that are not lints, and matter more than any of them:**

- **No resident state.** Every verb must work as a one-shot command that exits; killing any process changes no result. The CLI holds this by construction — it discovers a stream's height from the waypoint, never from a local file. Asserted by `a_command_keeps_no_state_that_a_kill_could_lose`.
- **Memory does not grow with the work.** `Verifier` holds one author and one head for a chain of any length; `Segment::extend` takes a `ChainHead`, not a predecessor. A change that buffers a whole chain is a change that broke the design, not one that needs more memory.

## SPECs and Markdown

- **Write each revision as the document's first draft.** A SPEC states what is true now, not how it became true. The check is one sentence: *a reader opening this file for the first time meets no sentence that only someone who read the previous version can use.*
- Keep the reason a rule has its current shape; the next change depends on it. Drop the record of what the rule used to be — that is what git is for.
- A decision belongs in the SPEC's decisions section as a numbered entry giving the decision, its reason, and the alternative it beat. It does not appear elsewhere as narration about when it changed.
- Do not write a changelog, a session log or a date into a SPEC's interface sections.
- **An open question in the tree is a claim about the code, never a message to a person.** *"Awaiting a ruling"* and *"the call is yours"* are turns in a conversation: a reader outside cannot act on them, and they go stale first. Delete an open question the moment it is answered, and put the answer where it is enforced.
- A rule that excludes an architecture states the parameter that made it right, beside the rule. That is not history; it is what the next reader needs in order to re-argue it.
- When a session ends with work unfinished, write what is left into the SPEC section it belongs to — as the current state of that interface, not as a note about your session.
- Write for engineers across many countries and many levels of English. Prefer the concrete word to the abstract one, carry each point in one clause, and put what matters most at the end of the sentence.

## Privacy

**A working record is noise to a contributor and to a model, and signal to an attacker.** It is the same sentences either way: what was tried and abandoned, which machine could not verify what, when somebody is away, and the words a decision arrived in. A reader skips them; someone looking for a way in reads them closely. No gate reaches a pull request description, a commit body or an issue, so this one is yours to hold.

- **Ship the decision, not the occasion.** The decision, the reason it beat the alternative, and the parameter that would re-open it belong in the tree. Who said it, when, on whose machine, and in what words do not.
- Report a measurement against the machine class it came from — *"43 ms on a warm cache, release build"*, not *"43 ms on this machine"*. A toolchain version belongs in `rust-toolchain.toml` or `adversary/lean-toolchain`, not in prose.
- Name a file by its repo-relative path, never an absolute one carrying a home directory.
- Do not paste raw terminal output, a credential, a token, a signed URL, or your own reasoning trace. Quote the lines that carry the finding; the diff says what you did.
- People's names, e-mail addresses, machine names and LAN addresses do not appear. A commit's author identity is the only place a person's name belongs.
- A screenshot shows the application. Crop out the desktop, the task bar and the window title.

**Nothing local leaves this machine.** The tree, a pull request, an issue and a published artefact are the only things that go out, and each is read before it goes. Everything else here is the person's: the home directory, the other checkouts, the shell history, the value of any environment variable, the instructions and preferences that configure you (`~/.pi` and its skills, `~/.cargo`, the harness itself), and the person's own words to you. None of it is material for a commit, an issue, a search query, a fetch prompt, a translation request, or a subagent that runs outside this machine. Such a call may carry what is already public in this repository and nothing more: a path in it is repo-relative, a measurement names a machine class. When it is unclear whether a byte is local, it stays.

## Where the authorities are

1. The person's ruling.
2. [`ARCHITECTURE.md`](ARCHITECTURE.md), with [`GLOSSARY.md`](GLOSSARY.md) for the words.
3. `crates/<crate>/<crate>-SPEC.md`, or `adversary/adversary-SPEC.md` and `glass/glass-SPEC.md` outside the workspace.
4. The code and its tests.

Where a lower level contradicts a higher one, the higher wins and the lower is corrected in the same change. **Where reality contradicts all of them, reality wins and the document is corrected first, with its reason.**
