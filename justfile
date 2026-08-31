# The closing condition for every change.
default: check

check: fmt lint test budget

fmt:
    cargo fmt --all

fmt-check:
    cargo fmt --all -- --check

lint:
    cargo clippy --all-targets --all-features -- -D warnings

test:
    cargo test --all-features

# Line counts against the budget in ARCHITECTURE.md §4.2. The budget is the
# mechanical form of "one million tokens is enough to read all of it".
budget:
    #!/usr/bin/env bash
    set -euo pipefail
    total=0
    printf '%-12s %8s %8s\n' crate lines limit
    for dir in crates/*/; do
        crate=$(basename "$dir")
        lines=$(find "$dir" -name '*.rs' -print0 | xargs -0 wc -l | tail -1 | awk '{print $1}')
        total=$((total + lines))
        printf '%-12s %8s %8s' "$crate" "$lines" 2500
        if [ "$lines" -gt 2500 ]; then printf '  OVER\n'; exit 1; else printf '  ok\n'; fi
    done
    printf '%-12s %8s %8s' TOTAL "$total" 25000
    if [ "$total" -gt 25000 ]; then printf '  OVER\n'; exit 1; else printf '  ok\n'; fi

# Stage 0 end to end, in a directory that is deleted afterwards.
demo:
    #!/usr/bin/env bash
    set -euo pipefail
    cargo build --quiet
    root=$(mktemp -d)
    trap 'rm -rf "$root"' EXIT
    bin=target/debug/kusanagi
    for line in "the first thing alice says" "the second" "the third"; do
        "$bin" --root "$root/city" send --as alice "$line"
    done
    "$bin" --root "$root/city" send --as bob "bob was here"
    echo
    "$bin" --root "$root/city" read --from alice
    echo
    "$bin" --root "$root/city" --json read --from bob
