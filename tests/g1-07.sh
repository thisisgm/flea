#!/usr/bin/env bash
# Exercise the Columns model, peeks and track Elements without a controlling terminal and judge the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch="$PWD/.superpowers/g1-07"
mkdir -p "$scratch"
unit_root=$(mktemp -d "$scratch/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
    XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    cargo test --locked -p flea-gui --test g1_07 < /dev/null
clippy_root=$(mktemp -d "$scratch/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
# Remove a passing run's root only when it is absolute, non-empty, marked and directly inside this unit's scratch.
drop_root() {
    [[ -n "$1" && "$1" == "$scratch"/?* && "$(dirname "$1")" == "$scratch" && -f "$1/.flea-test-sandbox" ]] && rm -rf -- "$1"
}
drop_root "$unit_root"
drop_root "$clippy_root"
printf 'G1_07 PASS columns geometry, peeks, landing, carry, third-column host, track Elements and clippy\n'
