#!/usr/bin/env bash
# Explicit optional-transport integration gate; no real cloud account is used.
set -eu
cd "$(dirname "$0")/.."
command -v rclone >/dev/null
cargo build --quiet
python3 tests/cloud-copy.py
