#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Escape out of a live search begun past the folder's first window lands the cursor on its row in list, columns and grid.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "search-exit-deep.sh: qs is not installed, cannot build the views"
    exit 1
fi
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[ -x "$bin" ] || { echo "search-exit-deep.sh: build the backend first: $bin"; exit 1; }
reps=${EXITDEEP_REPS:-1}
case "$reps" in ''|*[!0-9]*|0*) echo "search-exit-deep.sh: EXITDEEP_REPS='$reps' is not a positive integer"; exit 2 ;; esac

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-search-exit-deep.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/runtime" "$test_root/fixture" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/search-exit-deep.qml "$test_root/config/shell.qml" || exit 1
# 668 rows, about twice the first window: box-01 to box-60, a-note-1 to 8, misc-001 to 300, north-001 to 300.
for i in $(seq -w 1 60); do mkdir -p "$test_root/fixture/box-$i" || exit 1; done
for i in $(seq 1 8); do printf 'x\n' > "$test_root/fixture/a-note-$i.md"; done
for i in $(seq -w 1 300); do printf 'x\n' > "$test_root/fixture/misc-$i.dat"; done
for i in $(seq -w 1 300); do printf 'x\n' > "$test_root/fixture/north-$i.txt"; done

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    FLEA_BIN="$bin" EXITDEEP_DIR="$test_root/fixture" EXITDEEP_REPS="$reps" \
    timeout "${EXITDEEP_TIMEOUT:-120}" qs -p "$test_root/config" 2>&1)

# Sample input, the receipt: "  INFO qml: EXITDEEP DONE checks=108 failed=0", 12 checks for each of 9 exits per repetition.
done_count=$(printf '%s\n' "$output" | grep -c "EXITDEEP DONE checks=$((108 * reps)) failed=0\$")
noisy=$(printf '%s\n' "$output" | grep -av 'does not support setting window masks' | grep -aiE 'WARN|ERROR|Binding loop|failed to load|Unable to assign|Cannot assign|TypeError|ReferenceError|is not a function|is not defined')
printf '%s\n' "$output" | grep -a 'EXITDEEP TIME'
if [ "$done_count" -ne 1 ] || printf '%s\n' "$output" | grep -aq 'EXITDEEP FAIL' || [ -n "$noisy" ]; then
    printf 'FAIL the exit did not land the cursor on its row in the first frame\n'
    printf '%s\n' "$output" | grep -a 'EXITDEEP' | grep -av 'EXITDEEP PASS\|EXITDEEP TIME'
    [ -n "$noisy" ] && printf '%s\n' "$noisy"
    exit 1
fi
printf '%s\n' "$output" | grep -a 'EXITDEEP DONE'
