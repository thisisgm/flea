#!/usr/bin/env bash
# Exercise navigation acceptance without a controlling terminal and judge the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g1-03"
unit_root=$(mktemp -d "$PWD/.superpowers/g1-03/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
run_navigation_tests() {
  setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
      XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
      XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
      CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
      CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
      CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
      cargo test --locked -p flea-gui "$@" < /dev/null
}
run_navigation_tests --test g1_03
run_navigation_tests --lib sort0400_
clippy_root=$(mktemp -d "$PWD/.superpowers/g1-03/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G1_03 PASS history, watch, anchor, folder state, sort, transport and clippy\n'
