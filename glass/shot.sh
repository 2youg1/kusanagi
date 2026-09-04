#!/usr/bin/env bash
# Rebuild with the automation server, relaunch, and capture the canvas.
#
# Usage: bash shot.sh [name]         -> shots/<name>.png (default: screen)
# Screenshots are evidence for a pull request, not history: `shots/` is
# ignored by git (D-09). Needs `native` on PATH and the window to be able to
# open on this desktop. Every step is bounded; nothing here sleeps for long.
set -euo pipefail
cd "$(dirname "$0")"
name="${1:-screen}"
if [ "${SHOT_REBUILD:-1}" = 1 ]; then
    # The running window holds the file the build wants to replace.
    taskkill //F //IM glass.exe > /dev/null 2>&1 || true
    native build -Dautomation=true 2>&1 | grep -E "error|native build:" || true
    rm -rf .zig-cache/native-sdk-automation
    (./zig-out/bin/glass.exe > .zig-cache/glass-run.log 2>&1 &)
    native automate wait --timeout-ms 20000 > /dev/null
fi
native automate assert --absent --timeout-ms 2000 'error event=' > /dev/null
native automate screenshot glass-canvas > /dev/null
mkdir -p shots
cp .zig-cache/native-sdk-automation/screenshot-glass-canvas.png "shots/$name.png"
echo "shots/$name.png"
