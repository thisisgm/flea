#!/usr/bin/env bash
# Judge the menu primitive's tree against the existing clippy baseline from a marked scratch root.
set -euo pipefail
cd "$(dirname "$0")/.."
mkdir -p "$PWD/.superpowers/g2-mn1"
clippy_root=$(mktemp -d "$PWD/.superpowers/g2-mn1/clippy.XXXXXXXX")
: > "$clippy_root/.flea-test-sandbox"
printf 'tools/flea-clippy tools/clippy-baseline.txt %s\n' "$clippy_root"
tools/flea-clippy tools/clippy-baseline.txt "$clippy_root"
printf 'G2_MN1 PASS clippy baseline\n'
