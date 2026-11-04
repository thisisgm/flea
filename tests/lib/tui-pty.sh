#!/usr/bin/env bash
# Shared candidate and marked-sandbox launcher for the detail and owner PTY suites.
set -euo pipefail
. "$(dirname "$0")/../../tools/flea-sandbox-guard"
cd "$(dirname "$0")/../.."
suite=${1:?suite}
script=${2:?script}
shift 2
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { printf 'FAIL %s needs the candidate binary\n' "$suite"; exit 1; }
sandbox_root_ok
mkdir -p -- "$SANDBOX_ROOT"
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-$suite.XXXXXXXX")
: > "$scratch/.flea-test-sandbox"
cleanup() {
    [[ -n "$scratch" && "$scratch" = /* && -f "$scratch/.flea-test-sandbox" ]] || exit 1
    sandbox_remove "$scratch"
}
trap cleanup EXIT
if [ "$#" -eq 0 ]; then
    env -i PATH=/usr/bin:/bin HOME="$HOME" /usr/bin/setsid -w /usr/bin/python3 -B "tests/$script" "$bin" "$scratch" </dev/null
else
    for phase in "$@"; do
        mkdir "$scratch/$phase"
        : > "$scratch/$phase/.flea-test-sandbox"
        env -i PATH=/usr/bin:/bin HOME="$HOME" /usr/bin/setsid -w /usr/bin/python3 -B "tests/$script" "$bin" "$scratch/$phase" "$phase" </dev/null
    done
fi
