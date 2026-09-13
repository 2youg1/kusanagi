#!/bin/bash
# Installs the kusanagi CLI from a GitHub release, verifying the checksum.
#
# The release notes say it plainly: the checksums prove the download matches
# what the release workflow built, and nothing more. If that is not enough
# trust for you, build from source instead (see README.md).
#
# Usage: ./install.sh [VERSION]   (default: v0.0.1-Pre-alpha-260913)
#
# The default is the newest release tag, and it moves with each release: GitHub's
# "latest release" endpoint skips prereleases, and every release so far is one,
# so asking it would answer nothing.
set -euo pipefail

VERSION="${1:-v0.0.1-Pre-alpha-260913}"
REPO="2youg1/kusanagi"
DEST="${KUSANAGI_DEST:-$HOME/.local/bin}"

OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS-$ARCH" in
  Linux-x86_64)  TARGET="x86_64-unknown-linux-gnu" ;;
  Darwin-arm64)  TARGET="aarch64-apple-darwin" ;;
  *) echo "unsupported platform: $OS-$ARCH (this script covers Linux x86_64 and macOS arm64)" >&2; exit 1 ;;
esac

NAME="kusanagi-$VERSION-$TARGET"
BASE="https://github.com/$REPO/releases/download/$VERSION/$NAME"

mkdir -p "$DEST"
cd "$(mktemp -d)"
curl -fsSL -o "$NAME" "$BASE"
curl -fsSL -o "$NAME.sha256" "$BASE.sha256"
sha256sum -c "$NAME.sha256"
install -m 755 "$NAME" "$DEST/kusanagi"
echo "installed to $DEST/kusanagi — make sure $DEST is on your PATH"
"$DEST/kusanagi" --help >/dev/null && echo "runs: yes"
