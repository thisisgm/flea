#!/usr/bin/env bash
# Strict Unicode table census for run-all and flea-ci; package tests skip this ignored check.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
out=$(CARGO_TERM_COLOR=never cargo test --locked -p flea --bin flea backend::fatnames::casefold::casefold_unicode_tests::unicode_version_is_known -- --exact --ignored 2>&1)
rc=$?
printf '%s\n' "$out"
[ "$rc" -eq 0 ] || exit "$rc"
grep -Fq 'test result: ok. 1 passed; 0 failed; 0 ignored;' <<< "$out" || {
    printf 'FAIL unicode-tables: the strict version check did not run exactly once\n' >&2
    exit 1
}
