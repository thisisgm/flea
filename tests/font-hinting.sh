#!/usr/bin/env bash
# Production listing fonts and their measurements at every supported text-size stop.
set -uo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
command -v qs >/dev/null || { printf 'FAIL font-hinting: qs is required\n'; exit 1; }
[ -x target/debug/flea ] || cargo build -q || exit 1

test_root="$FIXTURE_ROOT/flea-font-hinting-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT
mkdir -p "$test_root"/{config,home,state/flea,cache,data,runtime} || exit 1
chmod 700 "$test_root/runtime" || exit 1
printf '{}\n' > "$test_root/state/flea/ui.json"
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
ln -s "$(readlink -f ui/boot/Commons)" "$test_root/config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/font-hinting.qml "$test_root/config/shell.qml" || exit 1

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_CONFIG_HOME="$test_root/config" XDG_STATE_HOME="$test_root/state" \
    XDG_CACHE_HOME="$test_root/cache" XDG_DATA_HOME="$test_root/data" XDG_RUNTIME_DIR="$test_root/runtime" \
    FLEA_BIN="$PWD/target/debug/flea" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 timeout 20 qs -p "$test_root/config" 2>&1)
status=$?
printf '%s\n' "$output" | grep -a 'FONT_HINTING' || true
# Commons imports Quickshell.Hyprland, which cannot connect in this offscreen fixture.
warnings=$(printf '%s\n' "$output" | grep -aE 'FONT_HINTING FAIL|TypeError|ReferenceError|ERROR|WARN' \
    | grep -vF '$HYPRLAND_INSTANCE_SIGNATURE is unset. Cannot connect to hyprland.' || true)
if [ "$status" -ne 0 ] || ! printf '%s\n' "$output" | grep -aqE 'FONT_HINTING DONE checks=[1-9][0-9]* failed=0' \
    || [ -n "$warnings" ]; then
    printf 'FAIL font-hinting: no warning-free completion (exit %s)\n' "$status"
    printf '%s\n' "$output"
    exit 1
fi
printf 'font-hinting: passed\n'
