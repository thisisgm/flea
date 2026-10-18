#!/bin/bash
# Preserve all three frozen-UI receipts, including failures, in one broker suite log.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
failed=0
for attempt in 1 2 3; do
    printf 'RENAMEMATRIX BEGIN run=%s\n' "$attempt"
    /bin/bash tests/rename-cost.sh
    rc=$?
    printf 'RENAMEMATRIX END run=%s rc=%s\n' "$attempt" "$rc"
    [ "$rc" -eq 0 ] || failed=1
done
exit "$failed"
