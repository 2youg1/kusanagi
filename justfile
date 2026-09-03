# The closing condition for every change.
default: check

check: fmt lint test budget deny

# The inner loop, for while a change is still being written.
#
# **Not a substitute for `check`, and it is not allowed to become one.** It skips
# the two gates whose cost does not depend on what was edited: `cargo deny` walks
# the whole dependency tree whatever changed, and `budget` walks every tracked
# file. Both are seconds, and both belong to the closing run rather than to a
# loop somebody runs forty times an hour.
#
# Pass the crates you touched, and their dependents will come with them because
# cargo rebuilds what it must:
#
#     just quick kusanagi-kernel
#     just quick "kusanagi-seal kusanagi-chain"
#
# With no argument it holds to the crates whose files git reports as changed,
# which is the common case and needs no thought at the keyboard.
quick crates="":
    #!/usr/bin/env bash
    set -euo pipefail
    picked="{{ crates }}"
    if [ -z "$picked" ]; then
        picked=$(git status --porcelain | awk '{print $NF}' | grep -o '^crates/[^/]*' \
            | sort -u | sed 's|crates/|kusanagi-|' | tr '\n' ' ')
        # `crates/kusanagi` is the one whose package name is not prefixed.
        picked=${picked//kusanagi-kusanagi/kusanagi}
    fi
    if [ -z "$picked" ]; then
        echo "nothing under crates/ has changed; run \`just check\`"
        exit 0
    fi
    selected=$(for crate in $picked; do printf -- '-p %s ' "$crate"; done)
    echo "quick: $picked"
    cargo fmt --all
    cargo clippy $selected --all-targets --all-features -- -D warnings
    cargo test $selected --all-features

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-features

# The supply chain is the one part of this design no amount of cryptography
# defends, so the set of crates that can reach a user is gated like any other rule.
deny:
    cargo deny check

# How many distinct crates can put code into the shipped binary.
#
# `cargo deny` answers "is any of this forbidden"; this answers "how much of it
# is there", which is the question a supply chain actually turns on. **Every
# crate here is somebody who can write code into this program**, so the number
# going up is a decision and the number going down needs no justification.
#
# Normal edges only: build scripts and dev-dependencies do not reach a user.
# Recorded in `.process/Roadmap.md` §J3 at each change, so a jump is visible in
# the diff rather than in somebody's memory.
deps:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo tree --workspace --edges normal --prefix none | sort -u | grep -c .


# Line counts against the budget in ARCHITECTURE.md. The budget is the mechanical
# form of "one context window is enough to read all of it": each crate's
# implementation must stay small enough to hold in mind, and the workspace as a
# whole — tests included, because a reader reads those too — must stay small
# enough to hold at all.
#
# The first gate is the strictest and the most local. A file is the unit somebody
# opens — a reviewer, an editor, a model reading one — so a file that no longer
# fits in one reading is split or deleted, and the limit is not raised.
#
# One number for every kind of file, because a limit you cannot apply from memory
# is one you meet at the gate instead of at the keyboard. See ARCHITECTURE.md §5
# for what the ladder of per-kind limits cost and what it bought.
budget:
    #!/usr/bin/env bash
    set -euo pipefail

    # `Cargo.lock` and `LICENSE` are outside the count because their contents are
    # not written here: cargo generates one, and the other is the licence text
    # verbatim. `awk END{NR}` rather than `wc -l` so that a file whose last line
    # has no newline is counted, not rounded down.
    limit=400
    over=$(git ls-files | grep -Ev '^(Cargo\.lock|LICENSE)$' | while IFS= read -r file; do
        lines=$(awk 'END { print NR }' "$file")
        if [ "$lines" -gt "$limit" ]; then printf '  %5s / %-4s %s\n' "$lines" "$limit" "$file"; fi
    done)
    if [ -n "$over" ]; then
        printf 'over the per-file limit. Split it or delete it; the limit does not move.\n%s\n' "$over"
        exit 1
    fi

    total=0
    printf '%-12s %8s %8s %8s\n' crate src limit all
    for dir in crates/*/; do
        crate=$(basename "$dir")
        src=$(find "$dir/src" -name '*.rs' -print0 | xargs -0 wc -l | tail -1 | awk '{print $1}')
        all=$(find "$dir" -name '*.rs' -print0 | xargs -0 wc -l | tail -1 | awk '{print $1}')
        total=$((total + all))
        printf '%-12s %8s %8s %8s' "$crate" "$src" 4000 "$all"
        if [ "$src" -gt 4000 ]; then printf '  OVER\n'; exit 1; else printf '  ok\n'; fi
    done
    printf '%-12s %8s %8s %8s' TOTAL "" 25000 "$total"
    if [ "$total" -gt 25000 ]; then printf '  OVER\n'; exit 1; else printf '  ok\n'; fi

# Two identities, one host, one verifiable exchange — in a directory that is
# deleted afterwards. The same story over TCP is asserted by
# `cargo test --test across_tcp`, which is where the network claim is actually
# checked rather than demonstrated.
demo:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --quiet
    ground=$(mktemp -d)
    trap 'rm -rf "$ground"' EXIT
    bin=target/debug/kusanagi
    alice="$bin --root $ground/alice"
    bob="$bin --root $ground/bob"

    # Every name and every message goes in on stdin, and `-` is what says so.
    # A command line is public: any account on the machine reads another
    # process's arguments while it runs, and the shell keeps them afterwards.
    # An invitation leaks one chance to enter one channel; `--to bob` leaks who
    # is talking to whom, on every message. So neither is an argument here.
    echo '--- alice opens a channel and mints one line ---'
    invite=$(printf 'bob\n' | $alice --json invite --name - --waypoint "$ground/host" | grep -o 'kusanagi2:[0-9a-f]*')
    echo "${invite:0:72}…"
    echo
    echo '--- bob joins with nothing but that line, piped in ---'
    printf 'alice\n%s' "$invite" | $bob join --name -
    echo
    for line in "the first thing alice says" "the second" "the third"; do
        printf 'bob\n%s' "$line" | $alice send --to - > /dev/null
    done
    printf 'alice\nbob heard you' | $bob send --to - > /dev/null
    echo '--- bob reads alice, verified from genesis ---'
    printf 'alice\n' | $bob read --from -
    echo
    echo '--- alice reads bob ---'
    printf 'bob\n' | $alice --json read --from -
    echo
    echo '--- and this is everything the host can see ---'
    ls "$ground/host"/*/* | head -4
    echo '(opaque addresses, sealed bytes, no author anywhere)'

# The Haskell counterexample hunter in `adversary/`.
#
# Deliberately not part of `check`. It needs GHC, and a contributor who is
# changing Rust must be able to finish without installing a second toolchain —
# so this skips itself, successfully, when cabal is not there. The binary it
# drives is built here and passed in, which is why nothing inside `adversary/`
# has to know where a target directory lives.
#
# **Release, not debug.** Every property here spawns the binary tens of times,
# and ML-DSA-87 is a lattice scheme: unoptimised it is several times slower at
# the one thing every spawn does. Measured on this machine, `send` costs 149ms
# in debug and 43ms in release. The suite is not testing the optimiser, so the
# slow build buys nothing and costs minutes on every run.
adversary:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cabal > /dev/null 2>&1; then
        echo "skipped: cabal is not installed. See adversary/adversary-SPEC.md §2."
        exit 0
    fi
    built=$(cargo build --release --message-format json 2>/dev/null \
        | grep -o '"executable":"[^"]*kusanagi[^"]*"' | tail -1 | cut -d'"' -f4 | sed 's|\\\\|/|g')
    [ -n "$built" ] || { echo "cargo did not report an executable"; exit 1; }
    cd adversary
    KUSANAGI_BIN="$built" cabal test --test-show-details=direct

# Let this program reach the proxy and nothing else. Needs an administrator.
#
# **A sandbox has to be imposed from outside the thing being sandboxed.** A
# process cannot credibly shut itself in, so this is a Windows Firewall rule
# about a path, applied by somebody who is not that path. What it buys: a
# compromised kusanagi cannot reach a host directly, cannot send a plaintext DNS
# query, and cannot exfiltrate anywhere except through the proxy — which is the
# one destination its own design already assumes is watching.
#
# What it does not buy is written in `docs/confine.md`, and it is the important
# half: the allowed channel is enough to carry a site away, and the host was
# never trusted with anything in the first place.
#
# Idempotent: the rules are deleted before they are added.
confine port="9050":
    #!/usr/bin/env bash
    set -euo pipefail
    binary=$(cd "$(dirname "$0")" && pwd)/target/release/kusanagi.exe
    [ -f "$binary" ] || { echo "build it first: just dist"; exit 1; }
    just unconfine
    # Windows evaluates block rules before allow rules, so the block cannot be
    # `remoteip=any` or it would swallow the allow. The complement of loopback is
    # spelled out instead.
    powershell -NoProfile -Command "\
      New-NetFirewallRule -DisplayName 'kusanagi-proxy-only-allow' -Direction Outbound \
        -Program '$binary' -RemoteAddress 127.0.0.1 -RemotePort {{ port }} \
        -Protocol TCP -Action Allow | Out-Null; \
      New-NetFirewallRule -DisplayName 'kusanagi-proxy-only-block' -Direction Outbound \
        -Program '$binary' -RemoteAddress 0.0.0.0-126.255.255.255,128.0.0.0-255.255.255.255 \
        -Action Block | Out-Null"
    echo "kusanagi.exe may now reach 127.0.0.1:{{ port }} and nothing else."
    echo "Check it: \`kusanagi doctor http://example.com:80\` must fail with waypoint.timeout."

# Take the confinement off.
unconfine:
    #!/usr/bin/env bash
    set -euo pipefail
    powershell -NoProfile -Command "\
      Get-NetFirewallRule -DisplayName 'kusanagi-proxy-only-*' -ErrorAction SilentlyContinue \
        | Remove-NetFirewallRule"
    echo "the confinement is off."

# Whether this machine builds the same bytes twice.
#
# A checksum on a release page says "this file is this file". It only says
# anything worth having if the source produces that file again, so this builds
# twice from a forced rebuild and compares. What it cannot prove alone is that
# *another* machine gets the same answer; CI runs the same recipe, and two
# independent machines agreeing is the claim `docs/VERIFY.md` actually makes.
repro:
    #!/usr/bin/env bash
    set -euo pipefail
    built() {
        cargo build --release --locked --message-format json 2>/dev/null \
            | grep -o '"executable":"[^"]*kusanagi[^"]*"' | tail -1 | cut -d'"' -f4
    }
    cargo build --release --locked --quiet
    # GNU coreutils prefixes the line with a backslash when the file name holds
    # one, which every Windows path does.
    first=$(sha256sum "$(built)" | cut -d' ' -f1 | tr -d '\\')
    # A rebuild of the leaf crate relinks the binary, which is where a timestamp
    # or an absolute path would show up if one had got in.
    touch crates/kusanagi/src/main.rs
    cargo build --release --locked --quiet
    second=$(sha256sum "$(built)" | cut -d' ' -f1 | tr -d '\\')
    if [ "$first" != "$second" ]; then
        printf 'two builds of the same source differ:\n  %s\n  %s\n' "$first" "$second"
        exit 1
    fi
    printf 'reproducible: %s\n' "$first"

# What a release ships: a stripped release binary and its checksum.
dist:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --release --locked
    mkdir -p dist
    triple=$(rustc -vV | sed -n 's/^host: //p')
    # Ask cargo where the binary is rather than guessing an extension: on Windows
    # a bare `test -f target/release/kusanagi` succeeds for `kusanagi.exe`, and
    # guessing produced two copies of one binary under two names.
    built=$(cargo build --release --locked --message-format json 2>/dev/null \
        | grep -o '"executable":"[^"]*kusanagi[^"]*"' | tail -1 | cut -d'"' -f4)
    [ -n "$built" ] || { echo "cargo did not report an executable"; exit 1; }
    name="kusanagi-$triple"
    case "$built" in *.exe) name="$name.exe";; esac
    cp "$built" "dist/$name"
    (cd dist && sha256sum "$name" > "$name.sha256")
    echo "dist/$name"
    cat "dist/$name.sha256"
    # The binary's own answer to the same question. Whoever receives this file
    # can check it without installing anything: run it. See `docs/VERIFY.md`.
    "dist/$name" doctor --here --json | grep '"binary"'
    rustc -vV | sed -n 's/^release: /rustc /p'
