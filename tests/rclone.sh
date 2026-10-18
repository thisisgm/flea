#!/bin/bash
# Rclone integration checks run the real backend inside individually marked roots.
set -euo pipefail
cd "$(dirname "$0")/.."
exec python3 -B tests/rclone.py "${FLEA_BIN:-target/debug/flea}"
