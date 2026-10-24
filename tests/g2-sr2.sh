#!/usr/bin/env bash
# Run the Filter040 controller, strip and core matcher tests without a controlling terminal and judge the clippy baseline.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g2-sr2"
unit_root=$(mktemp -d "$PWD/.superpowers/g2-sr2/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root/home" "$unit_root/config" "$unit_root/cache" "$unit_root/state" "$unit_root/runtime"
run() {
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_CACHE_HOME="$unit_root/cache" \
        XDG_STATE_HOME="$unit_root/state" XDG_RUNTIME_DIR="$unit_root/runtime" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only \
        cargo test --locked "$@" < /dev/null
}
# A filter matching nothing passes with zero, so the largest "test result: ok. 24 passed; 0 failed" count must reach this unit's cases.
count() { sed -n 's/^test result: ok\. \([0-9]*\) passed; 0 failed.*/\1/p' "$1" | sort -n | tail -1; }
run -p flea-gui --lib g2::sr2 > "$unit_root/gui.log" 2>&1 || { tail -40 "$unit_root/gui.log" >&2; exit 1; }
run -p flea --lib -- ui_logic_sr2 > "$unit_root/core.log" 2>&1 || { tail -40 "$unit_root/core.log" >&2; exit 1; }
run -p flea-gui --lib window::filter > "$unit_root/consumer.log" 2>&1 || { tail -40 "$unit_root/consumer.log" >&2; exit 1; }
run -p flea --bin flea -- backend::filter > "$unit_root/producer.log" 2>&1 || { tail -40 "$unit_root/producer.log" >&2; exit 1; }
gui=$(count "$unit_root/gui.log"); core=$(count "$unit_root/core.log")
consumer=$(count "$unit_root/consumer.log"); producer=$(count "$unit_root/producer.log")
[ "${gui:-0}" -ge 27 ] && [ "${core:-0}" -ge 36 ] && [ "${consumer:-0}" -ge 1 ] && [ "${producer:-0}" -ge 4 ] || {
    printf 'g2-sr2: FAIL gui=%s core=%s consumer=%s producer=%s checks ran\n' "${gui:-0}" "${core:-0}" "${consumer:-0}" "${producer:-0}"; exit 1; }
clippy_root=$(mktemp -d "$PWD/.superpowers/g2-sr2/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G2_SR2 PASS gui=%s core=%s consumer=%s producer=%s filter controller, strip, matcher, window consumer, backend producer and clippy\n' "$gui" "$core" "$consumer" "$producer"
