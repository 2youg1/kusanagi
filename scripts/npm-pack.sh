#!/bin/bash
# Turns a release tag and the built binaries into an npm tree ready to publish.
#
# One input decides every version in the tree: the tag. Each package.json under
# npm/ carries the token 0.0.0-placeholder wherever a version belongs — its own,
# and in the shell the pin on each platform package — and this script replaces
# every occurrence with the version the tag names. No version is edited by hand
# anywhere, and nothing downstream invents one.
#
# Usage: npm-pack.sh <tag> <binary-dir> <out-dir>
#   tag         the release tag, e.g. v0.0.1-Pre-alpha-260913
#   binary-dir  holds the release assets named kusanagi-<tag>-<target>[.exe]
#   out-dir     written fresh; receives cli/ and three platform-*/ directories
#
# Progress goes to stderr. Stdout carries the published version and nothing
# else, so a caller can read it without parsing prose.
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: npm-pack.sh <tag> <binary-dir> <out-dir>" >&2
  exit 2
fi
tag="$1"
bin_dir="$2"
out_dir="$3"
repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
src="$repo/npm"

# The tag minus its leading `v` is the published version, with nothing added and
# nothing rewritten: `v0.0.1-Pre-alpha-260913` publishes as
# `0.0.1-Pre-alpha-260913`, so the release page, the git tag and the registry all
# say one string, and a reader comparing them never has to translate.
#
# Release tags here read v<major>.<minor>.<patch>-<Stage>-<YYMMDD>, which is a
# semver prerelease: the stage and the release date form one prerelease
# identifier. npm parses semver and nothing else, so a tag outside that grammar
# stops the run here rather than reaching the registry as a version nobody chose.
if [[ "${tag#v}" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z-]+(\.[0-9A-Za-z-]+)*)?$ ]]; then
  version="${tag#v}"
else
  echo "npm-pack: tag '$tag' is not v<major>.<minor>.<patch>[-prerelease]" >&2
  exit 1
fi
echo "npm-pack: tag $tag publishes as $version" >&2

# target triple | platform package directory | binary name inside the package
platforms="
x86_64-unknown-linux-gnu|platform-linux-x64|kusanagi
aarch64-apple-darwin|platform-darwin-arm64|kusanagi
x86_64-pc-windows-msvc|platform-win32-x64|kusanagi.exe
"

stamp() { # <source package.json> <destination package.json>
  sed "s/0\.0\.0-placeholder/$version/g" "$1" > "$2"
}

rm -rf "$out_dir"
mkdir -p "$out_dir"

while IFS='|' read -r target package binary; do
  [ -n "$target" ] || continue
  asset="kusanagi-$tag-$target"
  case "$binary" in *.exe) asset="$asset.exe" ;; esac
  if [ ! -f "$bin_dir/$asset" ]; then
    echo "npm-pack: missing release binary $bin_dir/$asset" >&2
    exit 1
  fi
  # The checksum beside the asset is the one the release page publishes.
  # Checking it here makes the bytes inside the npm package the same bytes a
  # direct download gets, and says so by failing rather than by promising.
  if [ -f "$bin_dir/$asset.sha256" ]; then
    (cd "$bin_dir" && sha256sum -c "$asset.sha256" >/dev/null)
  fi
  mkdir -p "$out_dir/$package/bin"
  cp "$bin_dir/$asset" "$out_dir/$package/bin/$binary"
  chmod 755 "$out_dir/$package/bin/$binary"
  stamp "$src/$package/package.json" "$out_dir/$package/package.json"
  cp "$repo/LICENSE" "$out_dir/$package/LICENSE"
  echo "npm-pack: $package carries $asset" >&2
done <<< "$platforms"

mkdir -p "$out_dir/cli"
cp "$src/cli/bin.js" "$out_dir/cli/bin.js"
stamp "$src/cli/package.json" "$out_dir/cli/package.json"
cp "$repo/LICENSE" "$out_dir/cli/LICENSE"
cp "$src/cli/README.md" "$out_dir/cli/README.md"
echo "npm-pack: cli carries bin.js and pins all three platform packages" >&2

echo "$version"
