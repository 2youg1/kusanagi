#!/usr/bin/env bash
# Prints the RUSTFLAGS that make a release build independent of where it
# happened. Used by `just dist`, `just repro`, and the CI jobs that build
# what a release ships.
#
# **Why a script and not `.cargo/config.toml`.** Cargo does not expand
# environment variables inside `rustflags`, so a config line reading
# `--remap-path-prefix=$CARGO_HOME/registry=…` remaps the literal string
# `$CARGO_HOME` and nothing else. The binary this produced carried forty-three
# copies of the building account's home directory — a name that every panic
# location in every dependency spelled out. The shell expands paths; cargo
# does not; so the shell does it.
#
# Three prefixes are remapped: the registry cache, the toolchain (the standard
# library's own paths), and the checkout. On Windows the paths rustc sees are
# the Windows spelling, so each one goes through `cygpath` when it is present.
#
# `RUSTFLAGS` replaces every `rustflags` in `.cargo/config.toml`, so the one
# target-specific flag that lives there, `/Brepro`, is repeated here for the
# MSVC target rather than lost.
set -euo pipefail

native() {
    if command -v cygpath > /dev/null 2>&1; then cygpath -w "$1"; else printf '%s' "$1"; fi
}

cargo_home=$(native "${CARGO_HOME:-$HOME/.cargo}")
rustup_home=$(native "${RUSTUP_HOME:-$HOME/.rustup}")
checkout=$(native "$(cd "$(dirname "$0")/.." && pwd)")

flags="--remap-path-prefix=${cargo_home}=/cargo"
flags="$flags --remap-path-prefix=${rustup_home}=/rustup"
flags="$flags --remap-path-prefix=${checkout}=/kusanagi"
case "$(rustc -vV | sed -n 's/^host: //p')" in
    *-msvc) flags="$flags -Clink-arg=/Brepro" ;;
esac
printf '%s\n' "$flags"
