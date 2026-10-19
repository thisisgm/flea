#!/usr/bin/env bash
# Exercise the List Element, header and row facts without a controlling terminal and judge the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch="$PWD/.superpowers/g1-06"
mkdir -p "$scratch"
roots() { find "$scratch" -mindepth 1 -maxdepth 1 -type d \( -name 'ci.*' -o -name 'clippy.*' \) | wc -l; }
before=$(roots)
unit_root=$(mktemp -d "$scratch/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root"/{home,config,state,cache,runtime}
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
    XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    cargo test --locked -p flea-gui --test g1_06 < /dev/null
clippy_root=$(mktemp -d "$scratch/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
# Remove a passing run's root only when it is absolute, non-empty, marked and directly inside this unit's scratch.
drop_root() {
    [[ -n "$1" && "$1" == "$scratch"/?* && "$(dirname "$1")" == "$scratch" && -f "$1/.flea-test-sandbox" ]] && rm -rf -- "$1"
}
drop_root "$unit_root"
drop_root "$clippy_root"
after=$(roots)
printf 'G1_06 scratch roots before=%s after=%s\n' "$before" "$after"
[ "$after" -le "$before" ] || { printf 'FAIL g1-06 left %s scratch roots behind\n' "$((after - before))"; exit 1; }
printf 'G1_06 PASS columns, #333 widths, header, row cache, formats, marks, row IPC and clippy\n'
