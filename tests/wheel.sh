#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# The real ui/ActionWheel.qml draws Tangerine's six petals, answers the annulus and the four keys, and gives focus back; offscreen.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "wheel.sh: qs is not installed, cannot draw the action wheel"
    exit 1
fi

test_root="$FIXTURE_ROOT/flea-wheel-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/wheel.qml "$test_root/config/shell.qml" || exit 1

output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" 2>&1 ) 2>/dev/null )

# Sample input, the verdict line: "  INFO qml: WHEEL PASS 38 checks, 0 failed"
expected_checks=47
if ! printf '%s\n' "$output" | grep -q "WHEEL PASS $expected_checks checks, 0 failed" || printf '%s\n' "$output" | grep -q 'WHEEL FAIL'; then
    printf 'FAIL the action wheel did not draw, hit, route or close as Tangerine does, or made other than %s checks\n' "$expected_checks"
    printf '%s\n' "$output" | grep -aE 'WHEEL (FAIL|PASS)|ERROR|error|TypeError|ReferenceError' | head -30
    exit 1
fi
passed=$(printf '%s\n' "$output" | grep -c 'WHEEL ok ')
if [ "$passed" -ne "$expected_checks" ]; then
    printf 'FAIL the run passed %s checks by name, not %s\n' "$passed" "$expected_checks"
    exit 1
fi
# The offscreen platform itself says it cannot mask a FloatingWindow; that one line is the platform's, never the wheel's.
platform_warning='This plugin does not support setting window masks'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN|Unable to assign' | grep -vF "$platform_warning")
if [ -n "$warnings" ]; then
    printf 'FAIL the wheel harness logged a warning\n'
    printf '%s\n' "$warnings" | head -10
    exit 1
fi
printf '%s\n' "$output" | grep -o 'WHEEL PASS.*'
