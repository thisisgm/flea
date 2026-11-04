#!/usr/bin/env bash
# Marked UISH fixture and receipt controls, with private PID namespace, env and no controlling tty.
set -euo pipefail
repo="$(cd -- "$(dirname -- "$0")/.." && pwd -P)"
exec python3 -B "$repo/tests/uish-fixture-controls.py" --ci-launch
