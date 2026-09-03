# Confining kusanagi from outside it

**A sandbox has to be imposed by something that is not the thing being
sandboxed.** A process that shuts itself in has only asked itself politely; the
code that would have to be compromised to escape is the same code doing the
shutting. So there is no sandbox inside this program, and this document is the
list of things that can be applied to it from outside — cheapest first.

The design already did the largest part of this. Every verb is a one-shot
command that exits, so there is no resident process to attack between commands,
no long-lived credential in memory, and nothing to keep confined for longer than
one command's runtime.

## What confinement is actually for

**It is containment of a supply-chain failure, not immunity to one.** If a
dependency ships hostile code, the rules below stop that code reaching the rest
of this machine — the browser profile, the SSH keys, the other accounts. They do
not stop it exfiltrating kusanagi's own secrets, because the one channel they
leave open is enough to carry a site away, and the host on the far end of it was
never trusted with anything. Immunity to the supply chain is `just deps`,
`cargo deny`, `just repro`, and the property tests. Confinement is what limits
the damage when those fail.

## 1 Outbound firewall: the proxy is the only destination

Costs nothing at runtime and holds even if the binary is replaced, because the
rule is about a path and is enforced by the kernel.

```powershell
just confine          # allow 127.0.0.1:9050, block everywhere else
just confine 1080     # a different proxy port
just unconfine
```

Both need an administrator. They are idempotent: the rules are removed before
they are added, so running `confine` twice leaves one pair of rules.

**Why the block rule spells out the complement of loopback.** Windows evaluates
block rules before allow rules. A block of `any` would be evaluated first and
would swallow the allow, so the two address ranges either side of `127.0.0.0/8`
are named instead.

**Checking it works**, which is the only reason to believe it does:

```powershell
$env:KUSANAGI_PROXY = "socks5://127.0.0.1:9050"
kusanagi doctor http://example.com:80    # must fail: waypoint.timeout
kusanagi doctor <a waypoint via the proxy>   # must succeed
```

The second command is the one that matters. A rule that blocks everything is
easy; a rule that blocks everything *except* the intended path is what you are
checking.

**What it buys beyond the obvious:** no plaintext DNS. kusanagi rewrites
`socks5://` to `socks5h://` so hostnames are resolved by the proxy rather than
locally, and this rule means a build that stopped doing so could not send the
query anyway.

## 2 A restricted token: no code at all

```powershell
runas /trustlevel:0x20000 "kusanagi read --from -"
```

Runs the command with a basic-user token: no administrative group memberships,
no integrity level above medium. This is not a feature of this program and never
will be — it is one line of somebody else's tool, which is exactly the property
that makes it trustworthy here.

## 3 AppContainer: waiting for a launcher

An AppContainer is applied by whatever starts the process, with
`CreateProcessW` and `PROC_THREAD_ATTRIBUTE_SECURITY_CAPABILITIES`, granting
only the `internetClient` capability. There is nothing to attach it to yet: a
person typing at a shell is not a launcher that can be modified, and the GUI
shell that would be one is `.process/Roadmap.md` §F1. **It is not designed
before then**, because a design with no place to run is a guess.

## 4 Windows Sandbox: measured and rejected

`.wsb` starts a whole virtual machine. Every verb here is a process that exits in
tens of milliseconds, and a virtual machine boot per command is three orders of
magnitude more than the thing it protects. Recorded as not done rather than left
for somebody to rediscover.

## Other platforms

Nothing here is portable, and none of it needs to be: the program is unchanged
by all of it. When a Linux or macOS machine is verified, the equivalents are
`nftables`/`pf` for §1 and a user namespace or `sandbox-exec` for §2, and they
go in this document beside the Windows ones rather than into the program.

---

*This document is licensed under MPL-2.0.*
