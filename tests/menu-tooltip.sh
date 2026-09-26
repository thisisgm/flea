#!/usr/bin/env bash
# Real menu hover events, offscreen and with isolated settings.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."

test_root="$FIXTURE_ROOT/flea-menu-tooltip-$$"
sandbox_make "$test_root"
trap 'sandbox_remove "$test_root"' EXIT
mkdir -p "$test_root"/{config,home,state,cache,runtime,data}
chmod 700 "$test_root/runtime"
ln -s "$PWD/ui" "$test_root/config/flea"
ln -s "$(readlink -f ui/boot/Commons)" "$test_root/config/Commons"
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui"
cp tests/menu-tooltip.qml "$test_root/config/shell.qml"

output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_CONFIG_HOME="$test_root/config" XDG_STATE_HOME="$test_root/state" \
    XDG_CACHE_HOME="$test_root/cache" XDG_DATA_HOME="$test_root/data" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" 2>&1 ) 2>/dev/null ) || true

printf '%s\n' "$output" | grep -E 'MENU_TOOLTIP|ERROR|TypeError|ReferenceError' || true
if ! printf '%s\n' "$output" | grep -Eq 'MENU_TOOLTIP DONE [1-9][0-9]* checks, 0 failed'; then
    printf '%s\n' "$output"
    exit 1
fi
warnings=$(printf '%s\n' "$output" | grep -E 'TypeError|ReferenceError|WARN' \
    | grep -vF 'This plugin does not support setting window masks' || true)
if [ -n "$warnings" ]; then
    printf '%s\n' "$warnings"
    exit 1
fi
