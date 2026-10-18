#!/usr/bin/env bash
# Run the reviewed Trash producer and cancellation regressions where the real sandbox must work.
set -uo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

sandbox_root_ok
cleanup() {
    [[ -n "${box:-}" && "$box" == /* ]] || return
    if [[ -f "$box/.flea-test-sandbox" ]]; then
        sandbox_remove "$box"
    else
        sandbox_require "$box"
        rmdir -- "$box" 2>/dev/null || true
    fi
}
box=$(mktemp -d "$FIXTURE_ROOT/trashthumb-backend.XXXXXXXX") || exit 1
trap cleanup EXIT
: > "$box/.flea-test-sandbox" || exit 1
sandbox_require "$box"
export CARGO_TARGET_DIR="$box/target"
export TMPDIR="$box/tmp"
mkdir -p "$TMPDIR" || exit 1

# Suite setup ends here.
python3 tests/trashthumb-backend-setup.py "$FIXTURE_ROOT" || exit 1

for module in trashthumb::tests trashbrowse::thumbnail_tests thumbreq::cancel_tests run::trashthumb_protocol_tests; do
    log="$box/${module//::/-}.log"
    cargo test --locked --bin flea -- "backend::$module::" --nocapture > "$log" 2>&1
    rc=$?
    cat "$log"
    [ "$rc" -eq 0 ] || exit "$rc"
    if grep -q 'SKIP ' "$log" || ! grep -Eq '^test result: ok\. [1-9][0-9]* passed' "$log"; then
        echo "FAIL: $module skipped its sandbox or selected no tests" >&2
        exit 1
    fi
done
for producer in trashthumb_producer_ready_publications_do_not_overwrite_the_current_picture trashthumb_producer_failed_and_ready_publications_keep_their_reviewed_owners; do
    grep -Fq "$producer ... ok" "$box/trashthumb-tests.log" || { echo "FAIL: $producer did not execute" >&2; exit 1; }
done
echo 'trashthumb-backend: producer, descriptor and cancellation checks passed without skips'
