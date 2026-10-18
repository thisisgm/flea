#!/bin/bash
# Require navigation to answer before held rename work is released.
set -euo pipefail
cd "$(dirname "$0")/.."
. "$PWD/tools/flea-sandbox-guard"
bin=${BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL rename-lag needs the built backend'; exit 1; }
sandbox_root_ok
probe=$(mktemp -d "$SANDBOX_ROOT/flea-rename-lag.XXXXXX")
: > "$probe/$SANDBOX_MARKER"
trap 'sandbox_remove "$probe"' EXIT
cc -shared -fPIC -Wall -Werror tests/rename-hold.c -ldl -o "$probe/rename-hold.so"
python3 -B tests/rename-lag.py "$bin" "$probe" "$probe/rename-hold.so"
