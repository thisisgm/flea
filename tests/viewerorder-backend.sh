#!/usr/bin/env bash
# Exercise listing-order image paths and the external GIO handoff without a GUI.
set -euo pipefail
cd "$(dirname "$0")/.."
. tools/flea-sandbox-guard
sandbox_root_ok
test_root=$(mktemp -d "$FIXTURE_ROOT/flea-viewerorder-backend-XXXXXXXX")
sandbox_require "$test_root"
: > "$test_root/$SANDBOX_MARKER"
trap 'test -n "$test_root" && [[ "$test_root" = /* ]] && sandbox_remove "$test_root"' EXIT
timeout 90 python3 -I -B tests/viewerorder-backend.py "${BIN:-$PWD/target/debug/flea}" "$test_root"
