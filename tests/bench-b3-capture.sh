#!/bin/bash
# Offline controls for the B3 installed capture driver: a no-tty, minimal-environment run that fails unless the suite prints PASS.
set -uo pipefail
here=$(cd -- "$(dirname -- "$0")" && pwd -P)
root=$(mktemp -d "${TMPDIR:-/tmp}/bench-b3-capture.XXXXXXXX") || exit 2
: > "$root/.flea-test-sandbox"
out=$(env -i PATH=/usr/bin:/bin HOME="$root" TMPDIR="$root" LANG=C.UTF-8 /usr/bin/python3 -B "$here/bench-b3-capture.py" "$root" < /dev/null 2>&1); rc=$?
printf '%s\n' "$out"
[[ $root == /*/bench-b3-capture.* && -f $root/.flea-test-sandbox ]] && rm -rf -- "$root"
(( rc == 0 )) && grep -qx 'BENCH_B3_CAPTURE PASS controls=23' <<< "$out" || { echo 'BENCH_B3_CAPTURE FAIL'; exit 1; }
