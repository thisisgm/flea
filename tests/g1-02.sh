#!/usr/bin/env bash
# Exercise pane acceptance without a controlling terminal and judge the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g1-02"
unit_root=$(mktemp -d "$PWD/.superpowers/g1-02/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime" "$unit_root/data"
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
    XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    cargo test --locked -p flea-gui --test g1_02 < /dev/null
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
    XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" XDG_DATA_HOME="$unit_root/data" \
    FLEA_BIN="${FLEA_BIN:-${CARGO_TARGET_DIR:-$PWD/target}/debug/flea}" \
    python3 -B tests/g1-02-rowguard.py "$unit_root" < /dev/null
clippy_root=$(mktemp -d "$PWD/.superpowers/g1-02/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G1_02 PASS snapshots, cap, window, generation, state, IPC and clippy\n'
