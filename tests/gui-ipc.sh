#!/bin/bash
# Exercise the shipped client and socket server against an isolated headless surface.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd)
parent="$repo/.superpowers/g04e"
mkdir -p "$parent"
root=$(mktemp -d "$parent/gui-ipc.XXXXXX")
: > "$root/.flea-test-sandbox"
bin=${FLEA_BIN:-${CARGO_TARGET_DIR:-$repo/target}/debug/flea}
fixture=${FLEA_GUI_IPC_FIXTURE:-${CARGO_TARGET_DIR:-$repo/target}/debug/examples/ipc-fixture}
[[ -x $bin && -x $fixture ]] || { echo 'gui-ipc: build flea and the flea-gui ipc-fixture example first' >&2; exit 1; }
python3 -B "$repo/tests/gui-ipc.py" "$root" "$bin" "$fixture"
printf 'GUI_IPC DONE\n'
