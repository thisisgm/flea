#!/usr/bin/env bash
# Compare native scroll physics and slow-click decisions with the unchanged oracle in the isolated CI lane.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch="$PWD/.superpowers/g1-05"
mkdir -p "$scratch"
roots() { find "$scratch" -mindepth 1 -maxdepth 1 -type d \( -name 'test.*' -o -name 'ci.*' -o -name 'clippy.*' \) | wc -l; }
before=$(roots)
unit_root=$(mktemp -d "$scratch/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root"/{home,config,state,cache,runtime}
lane() {
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
        XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" G1_05_VECTORS="$unit_root/vectors.jsonl" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only "$@" < /dev/null
}
lane cargo test --locked -p flea-gui --test g1_05
# A filter matching nothing passes with zero, so the largest "test result: ok. N passed; 0 failed" count must reach this unit's cases.
count() { sed -n 's/^test result: ok\. \([0-9]*\) passed; 0 failed.*/\1/p' "$1" | sort -n | tail -1; }
wiring_floor=10
# The shell's wheel, stroke and slow-click wiring tests live in the library's test tree.
lane cargo test --locked -p flea-gui --lib g1_05 2>&1 | tee "$unit_root/lib.log"
wiring=$(count "$unit_root/lib.log")
[ "${wiring:-0}" -ge "$wiring_floor" ] || { printf 'FAIL g1_05 library wiring ran %s tests, want at least %s\n' "${wiring:-0}" "$wiring_floor"; exit 1; }
oracle() {
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
        XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        QML_XHR_ALLOW_FILE_READ=1 timeout 300s qml6 tests/g1-05.qml -- "$1" < /dev/null
}
oracle "$unit_root/vectors.jsonl"
# A native decision changed by one pixel or one slow-click answer must fail the oracle runner.
for mutation in nudge slow; do
    python3 -B tests/g1-05-mutate.py "$unit_root/vectors.jsonl" "$unit_root/$mutation.jsonl" "$mutation"
    if oracle "$unit_root/$mutation.jsonl" > "$unit_root/$mutation.log" 2>&1; then
        printf 'FAIL oracle runner accepted a native %s mutation\n' "$mutation"
        exit 1
    fi
    # The log names the first mismatch, for example: FAIL native/oracle parity Scroll.distance
    rejected=$(grep -m1 -o 'FAIL native/oracle parity [A-Za-z.]*' "$unit_root/$mutation.log") || {
        printf 'FAIL oracle runner rejected the %s control for another reason\n' "$mutation"
        exit 1
    }
    printf 'G1_05 control %s rejected: %s\n' "$mutation" "${rejected#FAIL native/oracle parity }"
done
clippy_root=$(mktemp -d "$scratch/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
# Remove a passing run's root only when it is absolute, non-empty, marked and directly inside this unit's scratch.
drop_root() {
    [[ -n "$1" && "$1" == "$scratch"/?* && "$(dirname "$1")" == "$scratch" && -f "$1/.flea-test-sandbox" ]] || {
        printf 'FAIL g1-05 refused to remove scratch root %s\n' "$1"
        exit 1
    }
    rm -rf -- "$1"
}
drop_root "$unit_root"
drop_root "$clippy_root"
after=$(roots)
printf 'G1_05 scratch roots before=%s after=%s\n' "$before" "$after"
[ "$after" -le "$before" ] || { printf 'FAIL g1-05 left %s scratch roots behind\n' "$((after - before))"; exit 1; }
printf 'G1_05 PASS scroll and slow-click policy, shell wiring tests, oracle vectors, mutation controls and clippy\n'
