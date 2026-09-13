#!/usr/bin/env bash
# The words in GLOSSARY.md, held against the tree.
#
# This is the only authority for that: `just glossary` and the check lane both
# call it. What it holds, in the order GLOSSARY.md §5 states it:
#
#   §1  each word's declaration exists where the table says, and the word is a
#       type in at most one file of crates/*/src — §4 names the two exemptions,
#       and an exemption whose second declaration is gone is refused too, so the
#       row cannot outlive the rename it waits for;
#   §2  a reserved word is declared nowhere: Rust, Lean or Zig;
#   §3  a rejected name is declared nowhere under crates/*/src.
#
# A declaration is a struct, enum, trait or type alias, or an `identifier!`
# entry, which is how kernel spells a fixed-width type. Modules, functions and
# locals are not looked at: a module is named for what it owns, and that name
# is not a concept.
set -euo pipefail

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"

glossary=GLOSSARY.md
status=0

# Word boundaries, spelt so that BSD grep reads them the same as GNU grep.
end='([^A-Za-z0-9_]|$)'

rust_decl() {
    printf '%s' '^[[:space:]]*(pub(\([a-z]+\))?[[:space:]]+)?(struct|enum|trait|type)[[:space:]]+'"$1$end"'|^[[:space:]]+'"$1"',[[:space:]]*[0-9]+[[:space:]]*$'
}
lean_decl() { printf '%s' '^(structure|inductive|abbrev|class|def)[[:space:]]+'"$1"'([^A-Za-z0-9_.]|$)'; }
zig_decl() { printf '%s' '^[[:space:]]*(pub[[:space:]]+)?const[[:space:]]+'"$1"'[[:space:]]*=[[:space:]]*(packed[[:space:]]+|extern[[:space:]]+)?(struct|enum|union)'; }

rust_files() { grep -rlE --include='*.rs' "$(rust_decl "$1")" crates/*/src 2>/dev/null || true; }
lean_files() { grep -rlE --include='*.lean' --exclude-dir=.lake "$(lean_decl "$1")" adversary 2>/dev/null || true; }
zig_files() { grep -rlE --include='*.zig' "$(zig_decl "$1")" glass/src 2>/dev/null || true; }

# Every data row of one table, as "first cell<TAB>second cell". A table is known
# by the first two cells of its header row, and ends at the first non-table line.
table() {
    awk -F'|' -v c1="$1" -v c2="$2" '
        function trim(s) { gsub(/^[ \t]+|[ \t\r]+$/, "", s); return s }
        /^\|/ {
            a = trim($2); b = trim($3)
            if (a == c1 && b == c2) { inside = 1; next }
            if (!inside) next
            if (a ~ /^-+$/) next
            print a "\t" b
            next
        }
        { inside = 0 }
    ' "$glossary"
}

# The backticked names in one cell, one per line.
names() { grep -oE '`[^`]+`' | tr -d '`' || true; }

# Whether `crate::Name` or `crate::Enum::Variant` is declared where it says.
# Prints the reason when it is not.
declared() {
    local path=$1 crate name variant dir found
    read -r crate name variant <<< "$(printf '%s' "$path" | sed 's/::/ /g')"
    dir="crates/$crate/src"
    [ -d "$dir" ] || { printf 'no crate `%s`' "$crate"; return 1; }
    found=$(grep -rlE --include='*.rs' "$(rust_decl "$name")" "$dir" || true)
    [ -n "$found" ] || { printf 'not declared under %s' "$dir"; return 1; }
    if [ -n "$variant" ]; then
        printf '%s\n' "$found" | xargs grep -lE "^[[:space:]]+$variant$end" > /dev/null 2>&1 \
            || { printf 'declared, but `%s` has no variant `%s`' "$name" "$variant"; return 1; }
    fi
}

# §4 — the two words allowed a second declaration, while it still exists.
twice=""
while IFS=$'\t' read -r cell second; do
    word=$(printf '%s' "$cell" | names | head -1)
    path=$(printf '%s' "$second" | names | head -1)
    if reason=$(declared "$path"); then
        twice="$twice$word"$'\n'
    else
        printf '§4: `%s` no longer has its second declaration (%s); delete the row.\n' "$word" "$reason"
        status=1
    fi
done < <(table Word 'Second declaration')

# §1 — every word declared where it says, and as a type in one place.
words=0
while IFS=$'\t' read -r bold cell; do
    word=$(printf '%s' "$bold" | sed -E 's/^\*\*(.*)\*\*$/\1/')
    path=$(printf '%s' "$cell" | names | head -1)
    words=$((words + 1))
    if ! reason=$(declared "$path"); then
        printf '§1: %s names `%s`, which is %s.\n' "$word" "$path" "$reason"
        status=1
        continue
    fi
    files=$(rust_files "$word")
    count=$(printf '%s' "$files" | grep -c . || true)
    allowed=1
    if printf '%s' "$twice" | grep -qx "$word"; then allowed=2; fi
    if [ "$count" -gt "$allowed" ]; then
        printf '§1: %s is declared as a type in %s files; the glossary allows %s:\n' "$word" "$count" "$allowed"
        printf '  %s\n' $files
        status=1
    fi
done < <(table Word 'In the code')

# §2 — reserved words declared nowhere.
reserved=0
while IFS=$'\t' read -r cell _; do
    for name in $(printf '%s' "$cell" | names); do
        reserved=$((reserved + 1))
        hits=$(rust_files "$name"; lean_files "$name"; zig_files "$name")
        [ -z "$hits" ] || {
            printf '§2: `%s` is reserved for work not yet done, and is declared:\n' "$name"
            printf '  %s\n' $hits
            status=1
        }
    done
done < <(table Reserved For)

# §3 — rejected names declared nowhere in Rust.
rejected=0
while IFS=$'\t' read -r cell say; do
    for name in $(printf '%s' "$cell" | names); do
        rejected=$((rejected + 1))
        hits=$(rust_files "$name")
        [ -z "$hits" ] || {
            printf '§3: `%s` is declared; the word is %s:\n' "$name" "$say"
            printf '  %s\n' $hits
            status=1
        }
    done
done < <(table 'Not this' This)

if [ "$status" -eq 0 ]; then
    printf 'glossary: %s words declared where they say; %s names refused; %s reserved.\n' \
        "$words" "$rejected" "$reserved"
fi
exit $status
