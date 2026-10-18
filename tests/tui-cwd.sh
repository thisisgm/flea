#!/usr/bin/env bash
# Exercise the real CLI, backend, terminal handoff and editor in private controlling PTYs.
set -euo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL tui-cwd needs the candidate binary'; exit 1; }
sandbox_root_ok
mkdir -p -- "$SANDBOX_ROOT"
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-tui-cwd.XXXXXXXX")
: > "$scratch/.flea-test-sandbox"
cleanup() {
  [[ -n "$scratch" && "$scratch" = /* && -f "$scratch/.flea-test-sandbox" ]] || exit 1
  sandbox_remove "$scratch"
}
trap cleanup EXIT
setsid -w env -i PATH="$PATH" HOME="$HOME" python3 -B tests/tui_cwd.py "$bin" "$scratch" < /dev/null
