#!/bin/bash
# Verify the locked binary license inventory, expression controls and all package declarations.
set -euo pipefail
cd "$(dirname "$0")/.."
python3 -B tests/third-party-licenses.py
tools/flea-third-party-licenses --check
echo 'PASS third-party licenses: inventory, package declarations and regression controls'
