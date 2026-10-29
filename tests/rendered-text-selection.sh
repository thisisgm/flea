#!/usr/bin/env bash
# Native selection and Qt clipboard, without the desktop clipboard or user files.
set -euo pipefail
cd "$(dirname "$0")/.."
. tools/flea-sandbox-guard
. tests/headless-commons.sh
sandbox_root_ok
probe=$(mktemp -d "$FIXTURE_ROOT/flea-rendered-text-selection.XXXXXXXX")
: > "$probe/$SANDBOX_MARKER"
trap 'sandbox_remove "$probe"' EXIT
mkdir -p "$probe/config" "$probe/home" "$probe/state" "$probe/runtime"
chmod 700 "$probe/runtime"
headless_commons "$probe" || exit 1
ln -s "$PWD/ui" "$probe/config/flea"
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
cp tests/rendered-text-selection.qml "$probe/config/shell.qml"
python3 -c 'from pathlib import Path; import sys; Path(sys.argv[1]).write_text(("A **bold** paragraph with enough words to wrap across several visual lines. color: red; margin-top:8px; " * 4 + "\n\nSecond paragraph with more words to select.\n\n") * 15)' "$probe/sample.txt"
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_RUNTIME_DIR="$probe/runtime" \
    RENDER_SELECTION_FILE="$probe/sample.txt" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$probe/config" > "$probe/log" 2>&1 || { cat "$probe/log"; exit 1; }
cat "$probe/log"
[ "$(grep -c 'RENDERSELECT PASS' "$probe/log")" -eq 15 ]
grep -q 'RENDERSELECT DONE failures=0' "$probe/log"
! grep -qE 'RENDERSELECT FAIL|TypeError|ReferenceError|WARN|ERROR' "$probe/log"
