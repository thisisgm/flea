#!/usr/bin/env bash
# Exercise multi-item Properties through the real backend in an owned fixture.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
BIN=${BIN:-./target/debug/flea}
[ -x "$BIN" ] || { printf 'properties-multi: no executable at %s\n' "$BIN" >&2; exit 1; }
test_root=$(mktemp -d "$FIXTURE_ROOT/flea-properties-multi-XXXXXXXX")
case $test_root in /*/*) ;; *) exit 1 ;; esac
sandbox_require "$test_root"
: > "$test_root/$SANDBOX_MARKER"
cleanup() {
    [ -n "$test_root" ] && case $test_root in /*/*) sandbox_remove "$test_root" ;; *) return 1 ;; esac
}
trap cleanup EXIT
python3 tests/review-properties.py "$test_root" "$BIN"
async_root=$(mktemp -d "$test_root/flea-dirsize-async-XXXXXXXX")
case $async_root in /*/*) ;; *) exit 1 ;; esac
: > "$async_root/$SANDBOX_MARKER"
python3 tests/dirsize-async.py "$async_root" "$BIN"
