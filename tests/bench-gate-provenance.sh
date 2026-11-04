#!/usr/bin/env bash
# Checks that the B2 release gate ranks only closed, distinct, identity-bound workload evidence.
set -euo pipefail
cd "$(dirname "$0")/.."
test_parent="$PWD/.superpowers/bench-gate-provenance"
mkdir -p "$test_parent"
scratch=$(mktemp -d "$test_parent/test.XXXXXX")
: > "$scratch/.flea-test-sandbox"
trap 'case $scratch in "$test_parent"/test.?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT
python3 -B tests/bench-gate-provenance.py "$PWD" "$scratch" "${BENCH_GATE:-$PWD/tools/flea-bench-gate}"
printf 'BENCH_GATE_PROVENANCE PASS\n'
