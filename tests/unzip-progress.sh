#!/bin/bash
# Exercise extraction jobs through the real backend inside marked fixture roots.
set -euo pipefail
cd "$(dirname "$0")/.."
exec setsid -w env -i PATH=/usr/local/bin:/usr/bin:/bin \
    python3 -B tests/unzip-progress.py "${FLEA_BIN:-target/debug/flea}" </dev/null
