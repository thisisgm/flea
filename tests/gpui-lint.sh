#!/bin/bash
# Each mutant and control writes only inside this marked temporary root.
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-}" in ""|--tests-only) ;; *) echo "usage: tests/gpui-lint.sh [--tests-only]" >&2; exit 2 ;; esac
mkdir -p .superpowers/g10l
root=$(mktemp -d "$PWD/.superpowers/g10l/lint-tests.XXXXXX")
touch "$root/.flea-test-sandbox"
trap 'case "$root" in /*/.superpowers/g10l/lint-tests.?*) [ -n "$root" ] && [ -f "$root/.flea-test-sandbox" ] && rm -rf -- "$root" ;; esac' EXIT
FLEA_LINT_TEST_ROOT="$root" python3 -B tests/gpui_lint_tests.py
FLEA_LINT_TEST_ROOT="$root" python3 -B tests/gpui_lint_regressions.py
if [ "${1:-}" != --tests-only ]; then
  tools/flea-gpui-lint
fi
