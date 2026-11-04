#!/bin/bash
# Replays tools/flea-field-bench's run_one on stubbed launches: which media rows may rank, and whose CPU each row counts.
cd "$(dirname "$0")/.." || exit 1
exec python3 -B tests/bench-field-work.py "$@"
