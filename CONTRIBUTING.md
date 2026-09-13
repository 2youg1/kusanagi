# Contributing

kusanagi is an asynchronous messaging protocol: sealed fixed-size bytes wait at
one-time addresses on a host nobody trusts, and the host learns nothing from them.
Read `ARCHITECTURE.md` first, then `GLOSSARY.md`, then the SPEC of the crate you
are touching. `AGENTS.md` is the working rulebook, for people as much as for agents.

## The one command

```bash
just check
```

A change is finished when it is green: fmt, clippy at `-D warnings`, tests, the
line budget, the test boundary, the glossary, and cargo-deny. Before any push,
`cargo fmt --all -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`
must already pass on the tree being pushed; they are the first two steps of the
check lane, and a push that fails them is a red run somebody waits on.

## Five hard rules

1. Every text file stays under **400 lines** (440 for Markdown); each crate's
   `src/` under 5,000.
2. Zero warnings. No `unwrap`, `expect`, `panic!`, bare indexing or slicing in
   non-test code; no `unsafe` outside `vault::windows`.
3. The crate's SPEC changes **before** the code changes.
4. Every failure names the action, the subject, a stable code, and the command
   that recovers. A new code is added to `docs/codes.md` in the same change.
5. One name per concept, from `GLOSSARY.md`. A new concept enters that table in
   the same change as its declaration; `just glossary` refuses a synonym.

Black-box claims belong in `adversary/` (Lean, driving the shipped binary);
white-box claims stay in Rust. `just boxes` holds the boundary.

## Commits

- The subject is `<area>: <what>`, for example
  `npm/README: record npm trust, which replaces the npmjs.com settings page`.
- The body records **what you found**, since what you did is already in the
  diff: a gate that changed your design, a red-to-green transition that exposed
  a real defect, or a choice between two approaches whose reason the result
  does not show. It carries no working record — see `AGENTS.md` §Privacy.
- A commit that loosens a gate it is failing carries a `Verdict: user-approved`
  trailer. The wording of the ruling stays with the person; the trailer records
  that there was one.
- No DCO, no signed commits, no changelog entries are asked for.

## Every pull request carries its evidence

- What you changed it from and to: before and after screenshots for anything
  visible (real pixels, not reference renders), the failing output for
  anything else. A claim without the red it fixes is a paragraph.
- Output contract changes: a JSON sample of what `--json` now reports.
- Error changes: the command line that reproduces them.

Write the pull request in Chinese and English side by side, in the words of
`GLOSSARY.md`. Two versions let a reader take the faster one, and a
mistranslation is visible instead of silent.

Questions, bug reports and disagreements are all welcome — open an issue, or
email me (address on my profile).
