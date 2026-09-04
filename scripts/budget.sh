#!/usr/bin/env bash
# The line budget in ARCHITECTURE.md §5, as one executable rule.
#
# This is the only authority for the budget: `just budget` and CI both call it, so
# the two cannot drift. CI used to carry a second copy of this arithmetic that
# checked only the crate and workspace totals — which meant the strictest of the
# three gates, the per-file one, was never enforced by the machine that merges.
#
# What each number counts:
#   per file   every file git tracks, text only — a binary carries no lines, so
#              the question "can one reader hold this in one sitting" has no
#              answer for it. `grep -I` decides from the bytes, not from a list
#              of extensions somebody has to remember to update.
#   per crate  `src/*.rs` only — the implementation one has to hold in mind.
#   workspace  `src/*.rs` across every crate. Tests are outside the total since
#              D-15: a black-box claim belongs to `adversary/`, which is not
#              Rust and never was counted, and leaving test lines inside the
#              total made "write another test" compete with "write the code".
#              Every test file is still under the per-file limit, and `just
#              boxes` still decides where a test may stand.
set -euo pipefail

FILE_LIMIT=400
CRATE_LIMIT=4000
WORKSPACE_LIMIT=25000

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

status=0

over=$(git ls-files | grep -Ev '^(Cargo\.lock|LICENSE)$' | while IFS= read -r file; do
    grep -qI '' "$file" || continue
    lines=$(awk 'END { print NR }' "$file")
    if [ "$lines" -gt "$FILE_LIMIT" ]; then printf '  %5s / %-4s %s\n' "$lines" "$FILE_LIMIT" "$file"; fi
done)
if [ -n "$over" ]; then
    printf 'over the per-file limit. Split it or delete it; the limit does not move.\n%s\n' "$over"
    status=1
fi

total=0
printf '%-12s %8s %8s %8s\n' crate src limit all
for dir in crates/*/; do
    crate=$(basename "$dir")
    src=$(find "$dir/src" -name '*.rs' -exec cat {} + | awk 'END { print NR + 0 }')
    all=$(find "$dir" -name '*.rs' -exec cat {} + | awk 'END { print NR + 0 }')
    total=$((total + src))
    printf '%-12s %8s %8s %8s' "$crate" "$src" "$CRATE_LIMIT" "$all"
    if [ "$src" -gt "$CRATE_LIMIT" ]; then printf '  OVER\n'; status=1; else printf '  ok\n'; fi
done
printf '%-12s %8s %8s %8s' TOTAL "$total" "$WORKSPACE_LIMIT" ""
if [ "$total" -gt "$WORKSPACE_LIMIT" ]; then printf '  OVER\n'; status=1; else printf '  ok\n'; fi

# The total is src-only; say what the tests add so a reader can see both.
tests=$(find crates -name '*.rs' -path '*/tests/*' -exec cat {} + | awk 'END { print NR + 0 }')
echo "  (test lines, outside the total: $tests)"

exit $status
