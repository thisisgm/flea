#!/usr/bin/env bash
# Prove native adapters can reach owned session sockets without importing the live Quickshell runtime.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -B tests/native/g2/runtime-controls.py
python3 -B tests/native/g2/op2-scenes.py
