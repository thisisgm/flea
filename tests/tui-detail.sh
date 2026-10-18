#!/usr/bin/env bash
# Real TUI detail, dates and sort replies in a private PTY, also runnable under a detached CI lane.
set -euo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL tui-detail needs the candidate binary'; exit 1; }
sandbox_root_ok
mkdir -p -- "$SANDBOX_ROOT"
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-tui-detail.XXXXXXXX")
: > "$scratch/.flea-test-sandbox"
cleanup() {
    [[ -n "$scratch" && "$scratch" = /* && -f "$scratch/.flea-test-sandbox" ]] || exit 1
    sandbox_remove "$scratch"
}
trap cleanup EXIT
env -i PATH=/usr/bin:/bin HOME="$HOME" /usr/bin/setsid -w /usr/bin/python3 -B tests/tui_detail.py "$bin" "$scratch" </dev/null
