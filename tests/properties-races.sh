#!/usr/bin/env bash
# Exercise Properties metadata stalls, directory swaps and async size walks through the real backend.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
BIN=${BIN:-./target/debug/flea}
[ -x "$BIN" ] || { printf 'properties-races: no executable at %s\n' "$BIN" >&2; exit 1; }
test_root=$(mktemp -d "$FIXTURE_ROOT/flea-properties-races-XXXXXXXX")
case $test_root in /*/*) ;; *) exit 1 ;; esac
sandbox_require "$test_root"
: > "$test_root/$SANDBOX_MARKER"
cleanup() {
    [ -n "$test_root" ] && case $test_root in /*/*) sandbox_remove "$test_root" ;; *) return 1 ;; esac
}
trap cleanup EXIT
python3 tests/properties-races.py "$test_root" "$BIN"
async_root=$(mktemp -d "$test_root/flea-dirsize-async-XXXXXXXX")
case $async_root in /*/*) ;; *) exit 1 ;; esac
: > "$async_root/$SANDBOX_MARKER"
python3 tests/dirsize-async.py "$async_root" "$BIN"
printf '%s\n' 'properties-races: PASS metadata stalls, link swaps, single-item replies and 4 async size-walk scenarios'
