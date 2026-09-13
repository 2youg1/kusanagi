#!/bin/bash
# Composes the body of a release page: what a reader needs in order to install
# this build and to check it, followed by what GitHub generated about the changes.
#
# Every fact about an artefact comes from the job that built it, in a `.facts`
# file beside the artefact — the BLAKE3 the binary reports about itself, the
# compiler that produced it, and the runner image that hosted the link step. No
# version, hash or toolchain is written by hand anywhere, and this script invents
# none: an artefact whose `.facts` file is missing is reported as missing rather
# than described from a guess.
#
# Usage: release-notes.sh <tag> <asset-dir> <generated-notes-file>
#   tag                     the release tag, e.g. v0.0.1-Pre-alpha-260913
#   asset-dir               holds the release assets and their .facts files
#   generated-notes-file    GitHub's generated notes: What's Changed, New
#                           Contributors. Pass /dev/null to leave them out.
#
# The body goes to stdout and nothing else does, so a caller redirects it into a
# file and hands that to `gh release`.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: release-notes.sh <tag> <asset-dir> <generated-notes-file>" >&2
  exit 2
fi
tag="$1"
assets="$2"
generated="$3"
version="${tag#v}"

if [ ! -d "$assets" ]; then
  echo "release-notes: no such asset directory: $assets" >&2
  exit 1
fi

# One `.facts` file per artefact, written by the job that built it:
#   asset=<file name>
#   blake3=<hex, or absent when the artefact is not a binary that can hash itself>
#   rustc=<rustc --version output>
#   image=<runner image and its version>
fact() { # <facts file> <key>
  sed -ne "s/^$2=//p" "$1" | head -1
}

facts=("$assets"/*.facts)
if [ ! -e "${facts[0]}" ]; then
  echo "release-notes: no .facts file in $assets; the build jobs record them" >&2
  exit 1
fi

cat <<HEAD
Pre-alpha, tagged \`$tag\`. The tag, this page and the npm packages all carry one
string: \`v<major>.<minor>.<patch>-<Stage>-<YYMMDD>\` minus its leading \`v\` is the
published version, so nothing here has to be translated into anything else.

## Install

\`\`\`bash
npx --yes @kasanagi/cli@$version id     # or bunx
curl -fsSL https://raw.githubusercontent.com/2youg1/kusanagi/main/scripts/install.sh | bash -s -- $tag
\`\`\`

\`\`\`powershell
irm https://raw.githubusercontent.com/2youg1/kusanagi/main/scripts/install.ps1 | iex
\`\`\`

The Windows zip carries the window and the CLI beside it; the window shells out
to the CLI, so one build produces the pair.

## Nothing here is signed

The \`.sha256\` beside each asset proves that a download matches what the release
workflow built, and nothing more — it carries no provenance. The npm packages do
carry a provenance attestation, because they are published over OIDC from that
same workflow. [\`docs/VERIFY.md\`](https://github.com/2youg1/kusanagi/blob/$tag/docs/VERIFY.md)
says what each check establishes and what it leaves open.

## BLAKE3 of each asset

\`kusanagi doctor --here\` prints this same hash for the binary it runs from, so a
download can be checked against this table without a second tool. Each value was
computed on the runner that built that binary, by that binary.

| asset | BLAKE3 |
|---|---|
HEAD

for file in "${facts[@]}"; do
  asset="$(fact "$file" asset)"
  hash="$(fact "$file" blake3)"
  if [ -n "$hash" ]; then
    printf '| `%s` | `%s` |\n' "$asset" "$hash"
  fi
done

printf '\n'
for file in "${facts[@]}"; do
  asset="$(fact "$file" asset)"
  if [ -z "$(fact "$file" blake3)" ]; then
    printf 'The archive `%s` is an archive rather than a binary and reports no hash of its own; its `.sha256` is published beside it.\n\n' "$asset"
  fi
done

cat <<'TOOLCHAIN'
## The toolchain that built these

A reproducible build is only reproducible against a named toolchain.
`rust-toolchain.toml` pins the compiler, and rustup installs that version inside
the build step even when the lane also has `stable` available — so the version
below is the one that compiled these files, not the newest one on the runner. It
cannot pin the Microsoft linker: reproducing a Windows artefact byte for byte
needs the image named beside it.

TOOLCHAIN

for file in "${facts[@]}"; do
  asset="$(fact "$file" asset)"
  printf -- '- `%s`\n  - %s\n  - runner image %s\n' \
    "$asset" "$(fact "$file" rustc)" "$(fact "$file" image)"
done

if [ -s "$generated" ]; then
  printf '\n'
  cat "$generated"
fi
