#!/bin/bash
# Runs the unchanged clippy ratchet for every search test target, including review fixtures.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 2
mkdir -p .superpowers/fb-ci-fix || exit 2
root=$(mktemp -d "$PWD/.superpowers/fb-ci-fix/clippy.XXXXXX") || exit 2
: > "$root/.flea-test-sandbox"
tools/flea-clippy tools/clippy-baseline.txt "$root"
rc=$?
if [ "$rc" -ne 0 ]; then
    [ ! -f "$root/clippy-findings.tsv" ] || awk -F '\t' '$1 == "rustc" || $1 == "correctness" || $1 == "suspicious"' "$root/clippy-findings.tsv"
    [ ! -f "$root/clippy-cargo.log" ] || cat "$root/clippy-cargo.log"
    exit "$rc"
fi
printf 'PASS search-ci-lint: unchanged clippy baseline, no deny, rise or rustc warning\n'
