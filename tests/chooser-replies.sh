#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
python3 tests/compat/chooser_replies.py "${FLEA_BIN:-$PWD/target/debug/flea}"
