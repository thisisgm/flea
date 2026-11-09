#!/usr/bin/env bash
# Runs the device, phone, eject and unlock tests without a controlling terminal and judges the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g2-pl1"
scratch=()
# Removes each scratch root on any exit, only when it is absolute, under this unit's dir and marked.
drop_scratch() {
    local root
    for root in "${scratch[@]}"; do
        case $root in "$PWD"/.superpowers/g2-pl1/ci.?*|"$PWD"/.superpowers/g2-pl1/clippy.?*) [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac
    done
}
trap drop_scratch EXIT
unit_root=$(mktemp -d "$PWD/.superpowers/g2-pl1/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
scratch+=("$unit_root")
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
run() {
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
        XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
        "$@" < /dev/null
}
run cargo test --locked -p flea --lib -- ui_logic_device_rail
run cargo test --locked -p flea-gui --lib -- g2::device_rail
clippy_root=$(mktemp -d "$PWD/.superpowers/g2-pl1/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
scratch+=("$clippy_root")
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G2_PL1 PASS devices, phones, eject, unlock and clippy\n'
