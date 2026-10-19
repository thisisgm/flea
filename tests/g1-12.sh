#!/usr/bin/env bash
# Exercise the breadcrumb, path entry and suggestions without a controlling terminal and judge the clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
# Completion reads run on the pane's backend, so the path modules never touch the filesystem themselves.
if grep -rnE 'std::fs|read_dir|File::open|metadata\(' crates/flea-gui/src/path crates/flea-gui/src/ipc/path.rs; then
    printf 'G1_12 FAIL the path surface reads the filesystem on the UI thread\n'
    exit 1
fi
mkdir -p "$PWD/.superpowers/g1-12"
unit_root=$(mktemp -d "$PWD/.superpowers/g1-12/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
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
run cargo test --locked -p flea-gui --test g1_12
run cargo test --locked -p flea-gui --lib state::tests::g1_12
clippy_root=$(mktemp -d "$PWD/.superpowers/g1-12/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G1_12 PASS crumbs, path entry, completion, suggestions, IPC readers, window wiring and clippy\n'
