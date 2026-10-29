#!/usr/bin/env bash
# Native selection and Qt clipboard, without the desktop clipboard or user files.
set -euo pipefail
cd "$(dirname "$0")/.."
. tools/flea-sandbox-guard
. tests/headless-commons.sh
sandbox_root_ok
probe=$(mktemp -d "$FIXTURE_ROOT/flea-text-actions.XXXXXXXX")
: > "$probe/$SANDBOX_MARKER"
trap 'sandbox_remove "$probe"' EXIT
mkdir -p "$probe/config" "$probe/home" "$probe/state" "$probe/runtime"
chmod 700 "$probe/runtime"
headless_commons "$probe" || exit 1
ln -s "$PWD/ui" "$probe/config/flea"
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
cp tests/preview-text-actions.qml "$probe/config/shell.qml"
printf 'First & <plain>\nSecond café 😀\nThird line\n' > "$probe/sample.txt"
head -c 2097152 /dev/zero > "$probe/big.txt"
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_RUNTIME_DIR="$probe/runtime" \
    TEXT_ACTIONS_FILE="$probe/sample.txt" TEXT_ACTIONS_BIG="$probe/big.txt" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$probe/config" > "$probe/log" 2>&1 || { cat "$probe/log"; exit 1; }
cat "$probe/log"
[ "$(grep -c 'TEXTACTIONS PASS' "$probe/log")" -eq 28 ]
grep -q 'TEXTACTIONS DONE failures=0' "$probe/log"
! grep -qE 'TEXTACTIONS FAIL|TypeError|ReferenceError|WARN|ERROR' "$probe/log"
