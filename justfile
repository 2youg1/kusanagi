# The closing condition for every change.
default: check

check: fmt lint test budget deny

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
# One limit per kind, because the kinds fail differently. See ARCHITECTURE.md §5
# for why each number is the number it is.
budget:
    #!/usr/bin/env bash
    set -euo pipefail

    # `Cargo.lock` and `LICENSE` are outside the count because their contents are
    # not written here: cargo generates one, and the other is the licence text
    # verbatim. `awk END{NR}` rather than `wc -l` so that a file whose last line
    # has no newline is counted, not rounded down.
    over=$(git ls-files | grep -Ev '^(Cargo\.lock|LICENSE)$' | while IFS= read -r file; do
        case "$file" in
            *.rs) limit=400;;
            *.hs) limit=400;;
            *.md) limit=500;;
            *)    limit=300;;
        esac
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
        printf '%-12s %8s %8s %8s' "$crate" "$src" 2500 "$all"
        if [ "$src" -gt 2500 ]; then printf '  OVER\n'; exit 1; else printf '  ok\n'; fi
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

    echo '--- alice opens a channel and mints one line ---'
    invite=$($alice --json invite --name bob --waypoint "$ground/host" | grep -o 'kusanagi1:[0-9a-f]*')
    echo "${invite:0:72}…"
    echo
    echo '--- bob joins with nothing but that line, piped in ---'
    # Piped, not passed: the line carries the channel secret, and an argument is
    # readable by every account on the machine and kept by the shell afterwards.
    echo "$invite" | $bob join --name alice
    echo
    for line in "the first thing alice says" "the second" "the third"; do
        $alice send --to bob "$line" > /dev/null
    done
    $bob send --to alice "bob heard you" > /dev/null
    echo '--- bob reads alice, verified from genesis ---'
    $bob read --from alice
    echo
    echo '--- alice reads bob ---'
    $alice --json read --from bob
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
adversary:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v cabal > /dev/null 2>&1; then
        echo "skipped: cabal is not installed. See adversary/adversary-SPEC.md §2."
        exit 0
    fi
    built=$(cargo build --message-format json 2>/dev/null \
        | grep -o '"executable":"[^"]*kusanagi[^"]*"' | tail -1 | cut -d'"' -f4 | sed 's|\\\\|/|g')
    [ -n "$built" ] || { echo "cargo did not report an executable"; exit 1; }
    cd adversary
    KUSANAGI_BIN="$built" cabal test --test-show-details=direct

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
