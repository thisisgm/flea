#!/bin/bash
# Headless search instrument contracts, without a display or cache operations.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
python3 -B tests/fs-instrument.py
python3 -B tests/fs-instrument-regressions.py
python3 -B tests/search-admission.py
python3 -B tests/fs-instrument-control.py
printf '%s\n' 'fs-instrument: PASS'
