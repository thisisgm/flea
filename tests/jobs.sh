#!/bin/bash
# Real backend integration checks keep all writes inside marked roots owned by the suite.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 tests/jobs.py "${FLEA_BIN:-target/debug/flea}"
