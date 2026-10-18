#!/usr/bin/env bash
# Owner deadlines and release controls through the real TUI in a detached private PTY.
set -euo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL tui-owner-stall needs the candidate binary'; exit 1; }
sandbox_root_ok
mkdir -p -- "$SANDBOX_ROOT"
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-tui-owner-stall.XXXXXXXX")
: > "$scratch/.flea-test-sandbox"
cleanup() {
    [[ -n "$scratch" && "$scratch" = /* && -f "$scratch/.flea-test-sandbox" ]] || exit 1
    sandbox_remove "$scratch"
}
trap cleanup EXIT
for phase in control deadline; do
    mkdir "$scratch/$phase"
    : > "$scratch/$phase/.flea-test-sandbox"
    env -i PATH=/usr/bin:/bin HOME="$HOME" /usr/bin/setsid -w /usr/bin/python3 -B tests/tui_owner_stall.py "$bin" "$scratch/$phase" "$phase" </dev/null
done
printf 'PASS tui-owner-stall: held-stat deadline, healthy navigation, release control, clean TTY quit\n'
