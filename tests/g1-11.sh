#!/usr/bin/env bash
# Run the rail host's model and GPUI host tests in the isolated CI lane, then the clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch="$PWD/.superpowers/g1-11"
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
    cargo test --locked -p flea-gui --test g1_11 < /dev/null | tee "$unit_root/test.log"
# Zero discovered tests is a failure, never a pass.
count=$(grep -oE '^test result: ok\. [0-9]+ passed' "$unit_root/test.log" | grep -oE '[0-9]+' | head -1)
[ "${count:-0}" -gt 0 ] || { printf 'FAIL g1-11 discovered no tests\n'; exit 1; }
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
printf 'G1_11 scratch roots before=%s after=%s\n' "$before" "$after"
[ "$after" -le "$before" ] || { printf 'FAIL g1-11 left %s scratch roots behind\n' "$((after - before))"; exit 1; }
printf 'G1_11 PASS rail model, GPUI host (%s tests) and clippy\n' "$count"
