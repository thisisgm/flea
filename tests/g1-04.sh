#!/usr/bin/env bash
# Compare generated native decisions with the unchanged oracle in the isolated CI lane.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch="$PWD/.superpowers/g1-04"
mkdir -p "$scratch"
roots() { find "$scratch" -mindepth 1 -maxdepth 1 -type d \( -name 'test.*' -o -name 'ci.*' -o -name 'clippy.*' \) | wc -l; }
before=$(roots)
unit_root=$(mktemp -d "$scratch/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root"/{home,config,state,cache,runtime}
setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
    XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
    XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" \
    CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
    CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" G1_04_VECTORS="$unit_root/vectors.jsonl" \
    CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
    cargo test --locked -p flea-gui --test g1_04 < /dev/null
oracle() {
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
        XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        QML_XHR_ALLOW_FILE_READ=1 timeout 180s qml6 tests/g1-04.qml -- "$1" < /dev/null
}
oracle "$unit_root/vectors.jsonl"
# A native sheet that drops or reorders one row must fail the oracle runner.
for mutation in drop swap; do
    python3 -B tests/g1-04-mutate.py "$unit_root/vectors.jsonl" "$unit_root/$mutation.jsonl" "$mutation"
    if oracle "$unit_root/$mutation.jsonl" > "$unit_root/$mutation.log" 2>&1; then
        printf 'FAIL oracle runner accepted a native sheet with one row %s\n' "$mutation"
        exit 1
    fi
    rejected=$(grep -m1 -o 'native/oracle sheet .*' "$unit_root/$mutation.log") || {
        printf 'FAIL oracle runner rejected the %s control for another reason\n' "$mutation"
        exit 1
    }
    printf 'G1_04 sheet control %s rejected: %s\n' "$mutation" "${rejected#native/oracle sheet }"
done
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
printf 'G1_04 scratch roots before=%s after=%s\n' "$before" "$after"
[ "$after" -le "$before" ] || { printf 'FAIL g1-04 left %s scratch roots behind\n' "$((after - before))"; exit 1; }
printf 'G1_04 PASS generated Rust, native events, oracle vectors, locale, allocation control and clippy\n'
