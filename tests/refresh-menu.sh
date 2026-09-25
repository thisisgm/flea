#!/usr/bin/env bash
# Exercise the actual menu/rename focus lifecycle without touching a desktop window or user state.
set -Eeuo pipefail
shopt -s inherit_errexit
trap 'printf "%s: line %d: %s exited %d\n" "${0##*/}" "$LINENO" "$BASH_COMMAND" "$?" >&2' ERR
cd "$(dirname "$0")/.."
. "$PWD/tools/flea-sandbox-guard"
test_root=$(mktemp -d /tmp/flea-refresh-menu.XXXXXXXX)
FIXTURE_ROOT=$test_root
sandbox_root_ok
printf 'Flea refresh menu sandbox\n' > "$test_root/$SANDBOX_MARKER"
work="$test_root/work"
cleanup() { sandbox_remove "$work"; }
trap cleanup EXIT
sandbox_scratch "$work"
mkdir -p "$work"/{home,config,state,data,cache,runtime}
chmod 700 "$work/runtime"
ln -s "$PWD/ui" "$work/ui"
ln -s "$PWD/ui/Commons" "$work/Commons"
ln -s "$PWD/ui/Ui" "$work/Ui"
cp tests/refresh-menu.qml "$work/shell.qml"
# Quickshell exits on its own SIGTERM, so inspect the executed tally as well as the timeout.
result=0
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$work/home" XDG_CONFIG_HOME="$work/config" XDG_STATE_HOME="$work/state" \
    XDG_DATA_HOME="$work/data" XDG_CACHE_HOME="$work/cache" XDG_RUNTIME_DIR="$work/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=basic QSG_RHI_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout 15 qs -p "$work/shell.qml" > "$test_root/output" 2>&1 || result=$?
cat "$test_root/output"
[[ "$result" != 124 ]]
grep -q 'refresh-menu: 14 checks, 0 failed' "$test_root/output"
! grep -qE 'ERROR|ReferenceError|TypeError|FAIL ' "$test_root/output"
