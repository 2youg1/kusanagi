# Contributing

kusanagi is an asynchronous messaging protocol: sealed fixed-size bytes wait at
one-time addresses on a host nobody trusts, and the host learns nothing from them.
Read `ARCHITECTURE.md` first, then the SPEC of the crate you are touching.

## The one command

```bash
just check
```

A change is finished when it is green: fmt, clippy at `-D warnings`, tests, the
line budget, and cargo-deny.

## Four hard rules

1. Every text file stays under **400 lines**; each crate's `src/` under 5,000.
2. Zero warnings. No `unwrap`, `expect`, `panic!`, bare indexing or slicing in
   non-test code; no `unsafe` outside `site::permissions::windows`.
3. The crate's SPEC changes **before** the code changes.
4. Every failure names the action, the subject, a stable code, and the command
   that recovers. A new code is added to `docs/codes.md` in the same change.

Black-box claims belong in `adversary/` (Haskell, driving the shipped binary);
white-box claims stay in Rust. `just boxes` holds the boundary.

## Every pull request carries its evidence

- What you changed it from and to: before and after screenshots for anything
  visible (real pixels, not reference renders), the failing output for
  anything else. A claim without the red it fixes is a paragraph.
- Output contract changes: a JSON sample of what `--json` now reports.
- Error changes: the command line that reproduces them.

No DCO, no signed commits, no changelog entries are asked for.

Questions, bug reports and disagreements are all welcome — open an issue, or
email me (address on my profile).
