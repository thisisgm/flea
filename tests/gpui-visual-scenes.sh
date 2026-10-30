#!/usr/bin/env bash
# Use the capture smoke's isolated native compositor and owned process cleanup.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -B "$PWD/ci/visual/test_scenes.py"
python3 -B "$PWD/ci/visual/test_codeicons.py"
python3 -B "$PWD/ci/visual/test_omi_fit.py"
export FLEA_GPUI_SCENES=1
exec /bin/bash "$PWD/tests/gpui-capture-smoke.sh"
