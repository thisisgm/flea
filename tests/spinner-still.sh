#!/usr/bin/env bash
# Gate the shipped Omi rest, hidden and frame-counter contracts through the GPUI spinner suite.
set -euo pipefail
cd "$(dirname "$0")/.."
rc=0
out=$(bash tests/gpui-spinner.sh 2>&1) || rc=$?
printf '%s\n' "$out"
[ "$rc" -eq 0 ] || { printf 'FAIL spinner-still: gpui-spinner exited %s\n' "$rc"; exit 1; }
# Sample input: GPUISPINNER PASS 14 checks yields 14, the count the receipt carries so an empty run cannot pass.
checks=$(printf '%s\n' "$out" | sed -n 's/^GPUISPINNER PASS \([1-9][0-9]*\) checks$/\1/p')
[ -n "$checks" ] || { printf 'FAIL spinner-still: gpui-spinner printed no check count\n'; exit 1; }
printf 'SPINNERSTILL PASS %s checks\n' "$checks"
