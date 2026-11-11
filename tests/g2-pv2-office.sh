#!/usr/bin/env bash
# Exercise the office previews and the CSV table without a controlling terminal and judge the existing clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g2-pv2-office"
unit_root=$(mktemp -d "$PWD/.superpowers/g2-pv2-office/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
    XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    sh -c 'cargo test --locked -p flea-gui --test g2_pv2_office &&
        env -u CARGO_PROFILE_TEST_DEBUG -u CARGO_PROFILE_DEV_DEBUG cargo test --locked -p flea -- ui_logic_doc_preview::' < /dev/null
clippy_root=$(mktemp -d "$PWD/.superpowers/g2-pv2-office/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G2_PV2_OFFICE PASS office states, sheets, CSV table and clippy\n'
