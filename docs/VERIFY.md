# Verifying a build

**What this document is for.** A release is a file somebody hands you. Nothing
about a file says who made it or what it was made from, so this is the procedure
for finding out — and, just as importantly, the statement of what it does *not*
establish.

kusanagi is a program whose whole design assumes the host is hostile. That
assumption is worth nothing if the binary talking to the host is not the one the
source describes. The supply chain is the part of this design no cryptography in
it defends; this document, `just deps`, and `cargo deny` are what defend it.

## 1 What you are checking against

Every release is a signed git tag. The tag message carries three things:

| | |
|---|---|
| The SHA-256 of each platform's artefact | what you downloaded should hash to one of these |
| The BLAKE3 the binary reports about itself | the same fact, checkable without a second tool |
| The exact toolchain | `rustc` version, and on Windows the `link.exe` and Windows SDK versions |

The third one is there because a reproducible build is only reproducible against
a named toolchain. `rust-toolchain.toml` pins the Rust compiler for everybody
automatically. It cannot pin the Microsoft linker, so that version is recorded by
hand at release time and you need the same one to reproduce a Windows artefact
byte for byte.

## 2 The cheapest check: ask the binary

```bash
kusanagi doctor --here
```

The `binary` line is the BLAKE3 of the file the process was started from,
computed by the process itself. Compare it with the tag message.

**What this catches:** a file that was altered in transit, on a mirror, or on
disk. **What it does not catch:** a binary that was built to lie about its own
hash. A program that has been replaced can say anything. This check is worth
running because it costs one command and catches the ordinary failure; it is not
the check that closes the interesting one.

## 3 The check that does not trust the binary

```bash
sha256sum kusanagi-x86_64-pc-windows-msvc.exe
```

Compare with the tag message. This is computed by a tool that is not the file
being checked, so a hostile binary has no say in the answer.

## 4 The check that does not trust the release either

Build it yourself and compare.

```bash
git verify-tag v0.0.1          # the tag is signed
git checkout v0.0.1
just repro                     # builds twice, refuses if the two differ
just dist                      # the artefact and its SHA-256
```

`just repro` proves that *this machine* produces the same bytes twice.
Reproducing the *released* bytes needs the toolchain from the tag message: the
pinned `rustc` comes from `rust-toolchain.toml`, and on Windows the linker does
not, so a different Visual Studio installation will produce a different — and
equally correct — binary. When the hashes match, you have established that the
published file is what the published source compiles to, without trusting
whoever published it.

**Two independent machines agreeing is the real claim.** CI runs `just repro` on
a machine nobody here controls; your run is a third. A single machine reproducing
its own output proves only that the build is deterministic, which is a necessary
step and not the conclusion.

## 5 What is deliberately not here

- **A transparency log.** Sigstore and rekor would remove the step where you have
  to already hold the right public key. They are an option later, not a claim
  now, and pretending otherwise would be worse than the gap.
- **A Nix build.** Nix does not run natively on Windows and cannot pin MSVC, so
  it would replace the toolchain that actually ships. It becomes the right answer
  the day a Linux artefact exists; `.process/Roadmap.md` §J4 holds the condition.
- **Any claim about the dependencies' authors.** `just deps` prints how many
  people can put code into this binary and `cargo deny` gates which; neither of
  them is a claim that any of those people is trustworthy. Fewer is better, and
  the number going up is a decision somebody has to defend.

---

*This document is licensed under MPL-2.0.*
