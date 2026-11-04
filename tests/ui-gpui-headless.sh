#!/usr/bin/env bash
# Exercise the mounted GPUI host in the broker's private PID namespace without a controlling terminal.
set -euo pipefail
cd "$(dirname "$0")/.."
parent="$PWD/.superpowers/m9-uish-02"
mkdir -p "$parent"
root=$(mktemp -d "$parent/suite.XXXXXXXX")
: > "$root/.flea-test-sandbox"
trap 'case $root in "$parent"/suite.?*) [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac' EXIT
mkdir -p "$root/home" "$root/config" "$root/cache" "$root/state" "$root/data" "$root/runtime"
chmod 700 "$root/runtime"
setsid -w env -i PATH="$PATH" HOME="$root/home" \
    XDG_CONFIG_HOME="$root/config" XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" \
    XDG_DATA_HOME="$root/data" XDG_RUNTIME_DIR="$root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    /bin/bash -euo pipefail -c '
        test ! -t 0 && test ! -t 1
        cargo build --locked --target-dir "$1" -p flea
        FLEA_BIN="$1/debug/flea" cargo test --locked -p flea-gui --test uish_driver -- --nocapture
    ' headless-backend "$root/backend" < /dev/null
printf 'UI_GPUI_HEADLESS PASS mounted Window input, production IPC, identity, refusal, deadlines and teardown\n'
