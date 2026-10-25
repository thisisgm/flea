#!/usr/bin/env bash
# Run the agent/actions/plugin checks in both crates headless, require their counts, then judge clippy against the baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g2-mn2"
unit_root=$(mktemp -d "$PWD/.superpowers/g2-mn2/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
python3 -B tests/native/g2/mn2-boundary-controls.py "$unit_root"
python3 -B tests/native/g2/mn2-boundary.py "$unit_root"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
run() {
    local log=$1
    shift
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
        XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
        cargo test --locked "$@" < /dev/null > "$log" 2>&1
}
# A filter that matched nothing would pass with zero, so each count is required from cargo's summary line.
count() { sed -n 's/^test result: ok\. \([0-9]*\) passed; 0 failed.*/\1/p' "$1" | head -1; }
set +e
run "$unit_root/core.log" -p flea --lib ui_logic_mn2
core_rc=$?
run "$unit_root/gui.log" -p flea-gui --lib g2::mn2
gui_rc=$?
set -e
[ "$core_rc" -eq 0 ] || { tail -40 "$unit_root/core.log" >&2; printf 'g2-mn2: FAIL core tests exited %s\n' "$core_rc"; exit 1; }
[ "$gui_rc" -eq 0 ] || { tail -40 "$unit_root/gui.log" >&2; printf 'g2-mn2: FAIL GPUI tests exited %s\n' "$gui_rc"; exit 1; }
core=$(count "$unit_root/core.log")
gui=$(count "$unit_root/gui.log")
[ "${core:-0}" -ge 39 ] || { printf 'g2-mn2: FAIL only %s core checks ran\n' "${core:-0}"; exit 1; }
[ "${gui:-0}" -ge 25 ] || { printf 'g2-mn2: FAIL only %s GPUI checks ran\n' "${gui:-0}"; exit 1; }
clippy_root=$(mktemp -d "$PWD/.superpowers/g2-mn2/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G2MN2 PASS %s core and %s GPUI checks\n' "$core" "$gui"
