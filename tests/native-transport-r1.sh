#!/bin/bash
# Exercise startup, acknowledged exit, QML IDs and auxiliary-process ownership.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
. "$repo/tests/lib/flea-transport-ci.sh"
flea_transport_ci "$0" "$@"
python3 -B "$repo/tests/lib/flea_transport_r1.py" "$@"
