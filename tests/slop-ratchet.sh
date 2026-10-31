#!/bin/bash
# Runs the tools/flea-slop-ratchet unit tests, then the ratchet on this tree against its checked-in baseline.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
python3 -B tests/slop_ratchet_tests.py
tools/flea-slop-ratchet .
printf '%s\n' 'slop-ratchet: PASS'
