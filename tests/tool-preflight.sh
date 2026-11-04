#!/bin/bash
# Early dependency refusals in tests/ui.sh, tests/drag.sh and tools/flea-field-bench, with every setup step stubbed.
set -euo pipefail
cd -- "$(dirname -- "$0")/.."
python3 -B tests/tool-preflight.py
printf '%s\n' 'tool-preflight: PASS'
