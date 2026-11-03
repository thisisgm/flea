#!/usr/bin/env bash
# Exercise the Omi Rust player, snapped quads, shared frames and pane-render counters headless.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/gpui-spinner"
unit_root=$(mktemp -d "$PWD/.superpowers/gpui-spinner/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime" "$unit_root/specimens"
log="$unit_root/test.log"
set +e
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
    XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    FLEA_SPINNER_SPECIMENS="$unit_root/specimens" \
    cargo test --locked -p flea-gui --lib feedback:: -- --nocapture < /dev/null > "$log" 2>&1
rc=$?
set -e
grep -E '^(SPINNER_BOARD|SPINNER_ALLOCS|test |test result)' "$log" || true
[ "$rc" -eq 0 ] || { tail -40 "$log" >&2; printf 'gpui-spinner: FAIL cargo test exited %s\n' "$rc"; exit 1; }
# Every check is one test; a filter that matched nothing would pass with zero, so the count is required.
# Reads cargo's summary line, for example "test result: ok. 11 passed; 0 failed; 2 ignored; 0 measured; 33 filtered out; finished in 0.05s".
checks=$(sed -n 's/^test result: ok\. \([0-9]*\) passed; 0 failed.*/\1/p' "$log" | head -1)
[ "${checks:-0}" -ge 20 ] || { printf 'gpui-spinner: FAIL only %s checks ran\n' "${checks:-0}"; exit 1; }
shots=$(find "$unit_root/specimens" -name 'spinner-*.pgm' | wc -l)
[ "$shots" -eq 12 ] || { printf 'gpui-spinner: FAIL %s Omi specimens, want 12\n' "$shots"; exit 1; }
printf 'gpui-spinner: specimens in %s\n' "$unit_root/specimens"
clippy_root=$(mktemp -d "$PWD/.superpowers/gpui-spinner/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'GPUISPINNER PASS %s checks\n' "$checks"
