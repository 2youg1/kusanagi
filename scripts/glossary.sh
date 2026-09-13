#!/usr/bin/env bash
# The words in GLOSSARY.md, held against the tree.
#
# This is the only authority for that: `just glossary` and the check lane both
# call it. What it holds, in the order GLOSSARY.md §4 states it:
#
#   §1  each word's declaration exists where the table says, and the word is a
#       type in at most one file of crates/*/src;
#   §2  a reserved word is declared nowhere;
#   §3  a rejected name is declared nowhere.
#
# "Declared" means a type: in Rust a struct, enum, trait or type alias, or an
# `identifier!` entry, which is how kernel spells a fixed-width type; in Lean a
# structure, inductive, abbrev, class or def; in Zig a const bound to a struct,
# enum or union. Modules, functions and locals are not looked at: a module is
# named for what it owns, and that name is not a concept.
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
all_files() { rust_files "$1"; lean_files "$1"; zig_files "$1"; }

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

# §1 — every word declared where it says, and as a Rust type in one place.
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
    if [ "$count" -gt 1 ]; then
        printf '§1: %s is declared as a type in %s files, and a word has one meaning:\n' "$word" "$count"
        printf '  %s\n' $files
        status=1
    fi
done < <(table Word 'In the code')

# §2 and §3 — reserved words and rejected names declared nowhere. Runs in this
# shell, not a subshell, so that `status` and `counted` reach the end.
counted=0
refuse() {
    local section=$1 header1=$2 header2=$3 cell say name hits
    counted=0
    while IFS=$'\t' read -r cell say; do
        for name in $(printf '%s' "$cell" | names); do
            counted=$((counted + 1))
            hits=$(all_files "$name")
            [ -z "$hits" ] || {
                printf '%s: `%s` is declared; the word is %s:\n' "$section" "$name" "$say"
                printf '  %s\n' $hits
                status=1
            }
        done
    done < <(table "$header1" "$header2")
}
refuse '§2' Reserved For
reserved=$counted
refuse '§3' 'Not this' This
rejected=$counted

if [ "$status" -eq 0 ]; then
    printf 'glossary: %s words declared where they say; %s names refused; %s reserved.\n' \
        "$words" "$rejected" "$reserved"
fi
exit $status
