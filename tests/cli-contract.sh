#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# Both frontends run before the verdict, so one run reports every broken row.
status=0
python3 tests/compat/cli_contract.py "${FLEA_BIN:-$PWD/target/debug/flea}" "$@" || status=1
python3 tests/compat/cli_contract.py "${FLEA_BIN:-$PWD/target/debug/flea}" --gpui "$@" || status=1
exit "$status"
