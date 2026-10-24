#!/usr/bin/env bash
# Gate the shipped Omi rest, hidden and frame-counter contracts through the GPUI spinner suite.
set -euo pipefail
cd "$(dirname "$0")/.."
bash tests/gpui-spinner.sh
printf 'SPINNERSTILL PASS Omi rest and hidden frame contracts\n'
