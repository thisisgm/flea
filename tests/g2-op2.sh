#!/usr/bin/env bash
# The op2 archive and flash unit in the isolated CI lane: its core and GPUI tests, then the clippy baseline gate.
set -euo pipefail
cd "$(dirname "$0")/.."
scratch="$PWD/.superpowers/g2-op2"
mkdir -p "$scratch"
unit_root=$(mktemp -d "$scratch/ci.XXXXXXXX")
: > "$unit_root/.flea-test-sandbox"
mkdir -p "$unit_root"/{home,config,state,cache,runtime}
lane() {
    setsid -w env -i PATH="$PATH" HOME="$unit_root/home" \
        XDG_CONFIG_HOME="$unit_root/config" XDG_STATE_HOME="$unit_root/state" \
        XDG_CACHE_HOME="$unit_root/cache" XDG_RUNTIME_DIR="$unit_root/runtime" \
        CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}" \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$PWD/target}" \
        CARGO_PROFILE_TEST_DEBUG=line-tables-only CARGO_PROFILE_DEV_DEBUG=line-tables-only "$@" < /dev/null
}
# A failing run still prints its tail and a FAIL line, so the rc is captured with errexit off.
count() { sed -n 's/^test result: ok\. \([0-9]*\) passed.*/\1/p' "$1" | head -1; }
set +e
lane cargo test --locked -p flea --lib -- ui_logic_archive_flash > "$unit_root/core.log" 2>&1
core_rc=$?
lane cargo test --locked -p flea-gui --lib -- g2::archive_flash > "$unit_root/gui.log" 2>&1
gui_rc=$?
set -e
tail -3 "$unit_root/core.log"
tail -3 "$unit_root/gui.log"
[ "$core_rc" -eq 0 ] || { tail -40 "$unit_root/core.log" >&2; printf 'g2-op2: FAIL core tests exited %s\n' "$core_rc"; exit 1; }
[ "$gui_rc" -eq 0 ] || { tail -40 "$unit_root/gui.log" >&2; printf 'g2-op2: FAIL GPUI tests exited %s\n' "$gui_rc"; exit 1; }
core_n=$(count "$unit_root/core.log")
gui_n=$(count "$unit_root/gui.log")
[ "${core_n:-0}" -ge 57 ] || { printf 'g2-op2: FAIL core cases %s, want at least 57\n' "${core_n:-0}"; exit 1; }
[ "${gui_n:-0}" -ge 22 ] || { printf 'g2-op2: FAIL GPUI cases %s, want at least 22\n' "${gui_n:-0}"; exit 1; }
clippy_root=$(mktemp -d "$scratch/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
# Remove a passing run's root only when it is absolute, non-empty, marked and directly inside this unit's scratch.
drop_root() {
    if [[ -n "$1" && "$1" == "$scratch"/?* && "$(dirname "$1")" == "$scratch" && -f "$1/.flea-test-sandbox" ]]; then
        rm -rf -- "$1"
    else
        printf 'g2-op2: FAIL kept %s, not a marked root of %s\n' "$1" "$scratch" >&2
        return 1
    fi
}
drop_root "$unit_root"
printf 'g2-op2: ok G2OP2 PASS %s core and %s GPUI checks\n' "$core_n" "$gui_n"
