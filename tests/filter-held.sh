#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Real Pane/backend checks: matching first-key delegate stability and one-Escape refill after deep filtering, in list, columns and grid, offscreen.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "filter-held.sh: qs is not installed, cannot build the views"
    exit 1
fi
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[ -x "$bin" ] || { echo "filter-held.sh: build the backend first: $bin"; exit 1; }

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-filter-held.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/runtime" "$test_root/fixture" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/filter-held.qml "$test_root/config/shell.qml" || exit 1
# 150 names k matches sort first; 1200 names after them put the end beyond the held window.
for i in $(seq -w 1 150); do printf 'x\n' > "$test_root/fixture/keymap-$i.txt"; done
for i in $(seq -w 1 1200); do printf 'x\n' > "$test_root/fixture/notes-$i.txt"; done

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    FLEA_BIN="$bin" FILTERHELD_DIR="$test_root/fixture" timeout 60 qs -p "$test_root/config" 2>&1)
status=$?

# Sample input, the receipt: "  INFO qml: FILTERHELD PASS list drawn=34 made=0 gone=0 moved=0 count=150 shown=150; ...".
pass_count=$(printf '%s\n' "$output" | grep -c 'FILTERHELD PASS')
done_count=$(printf '%s\n' "$output" | grep -c 'FILTERHELD DONE')
clear_count=$(printf '%s\n' "$output" | grep -c 'FILTERHELD CLEAR')
noisy=$(printf '%s\n' "$output" | grep -av 'does not support setting window masks' | grep -aiE 'WARN|ERROR|Binding loop|failed to load|Unable to assign|Cannot assign|TypeError|ReferenceError|is not a function|is not defined')
if [ "$status" -ne 0 ] || [ "$pass_count" -ne 1 ] || [ "$done_count" -ne 1 ] || [ "$clear_count" -ne 3 ] || printf '%s\n' "$output" | grep -aq 'FILTERHELD FAIL' || [ -n "$noisy" ]; then
    printf 'FAIL filter delegate stability or deep-window clear refill (runner status %s)\n' "$status"
    printf '%s\n' "$output" | grep -a 'FILTERHELD'
    [ -n "$noisy" ] && printf '%s\n' "$noisy" | head -20
    exit 1
fi
printf '%s\n' "$output" | grep -a 'FILTERHELD PASS'
