#!/usr/bin/env bash
# Exercise the Plugins section through the backend protocol and the GPUI pane, then judge the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g2-st2"
unit_root=$(mktemp -d "$PWD/.superpowers/g2-st2/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime" "$unit_root/data"
for package in flea flea-gui; do
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
        XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" XDG_DATA_HOME="$unit_root/data" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
        cargo test --locked -p "$package" -- st2 < /dev/null
done
clippy_root=$(mktemp -d "$PWD/.superpowers/g2-st2/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G2_ST2 PASS backend protocol, pages, pane and clippy\n'
