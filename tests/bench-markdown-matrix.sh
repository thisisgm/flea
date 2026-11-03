#!/bin/bash
set -euo pipefail
repo=$(cd -- "$(dirname -- "$0")/.." && pwd -P)
parent="$repo/.superpowers/m9-bench-14"
mkdir -p -- "$parent"
proof_root=$(mktemp -d "$parent/suite.XXXXXXXX")
case "$proof_root" in "$parent"/suite.*) ;; *) exit 2 ;; esac
[[ "$proof_root" = /* && -n "$proof_root" ]] || exit 2
: > "$proof_root/.flea-test-sandbox"
setsid -w env -i PATH="$PATH" LANG=C.UTF-8 PYTHONDONTWRITEBYTECODE=1 \
    FLEA_VISUAL_DESIGN="${FLEA_VISUAL_DESIGN-}" FLEA_RELEASE_ROOT="${FLEA_RELEASE_ROOT-}" \
    python3 -B "$repo/tests/bench-markdown-matrix.py" "$proof_root" </dev/null
printf 'PASS: bench-markdown-matrix\n'
