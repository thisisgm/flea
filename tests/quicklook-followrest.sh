#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# quicklook-followrest: a held j with Quick Look open loads one row, its own first repeat, and nothing more under a starved loop.
set -uo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
. "$(dirname "$0")/qslog-gate.sh"
cd "$(dirname "$0")/.." || exit 1
for tool in qs; do
    command -v "$tool" >/dev/null || { printf 'FAIL quicklook-followrest: %s is required\n' "$tool"; exit 1; }
done
test_root="$FIXTURE_ROOT/flea-quicklook-followrest-$$"
sandbox_make "$test_root"
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT
mkdir -p "$test_root"/{config,fixture,home,state/flea,cache,runtime} || exit 1
chmod 700 "$test_root/runtime" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -m ui/boot/Ui)" "$test_root/config/Ui" || exit 1
ln -s "$test_root/config/flea/boot/fleatab.qml" "$test_root/config/fleatab.qml" || exit 1
cp -a ui "$test_root/config/flea" || exit 1
cp tests/quicklook-followrest.qml "$test_root/config/shell.qml" || exit 1
# Small text files: a follow load is cheap, so the burst is about the settle and not a decode.
for i in $(seq -w 1 24); do
    printf 'line one of file %s\nline two of file %s\nline three of file %s\n' "$i" "$i" "$i" > "$test_root/fixture/file-$i.txt" || exit 1
done
# Thumbnails off and the list view, so nothing beside the follow loads.
printf '{"updates":{"autoCheck":false},"view":"list","preview":{"thumbnails":"off"}}\n' > "$test_root/state/flea/ui.json"
log="$test_root/run.log"
( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_RUNTIME_DIR="$test_root/runtime" FLEA_BIN="$PWD/target/debug/flea" FLEA_PATH="$test_root/fixture" \
    FLEA_REDUCED_MOTION="" QLFR_UI="$test_root/config/flea" QLFR_DIR="$test_root/fixture" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    QT_LOGGING_RULES="$(qslog_rules)" \
    dbus-run-session -- bash -c 'timeout 150 qs -p "$1" > "$2" 2>&1' _ "$test_root/config" "$log" 2> "$test_root/bus.log" ) 2>/dev/null
status=$?
[ -z "${FLEA_CI_SUITE_LOGS:-}" ] || cp "$log" "$FLEA_CI_SUITE_LOGS/quicklook-followrest.log" 2>/dev/null
grep -a 'QLFR \(SWEEP\|FAIL\|DONE\)' "$log"
failures=0
qslog_nullptr quicklook-followrest < "$log" || failures=$((failures + 1))
qslog_crash quicklook-followrest "$log" || failures=$((failures + 1))
if [ "$status" -ne 0 ] || [ "$(grep -ac 'QLFR DONE' "$log")" -ne 1 ] || grep -aqE 'QLFR FAIL|TypeError|ReferenceError|Binding loop detected' "$log"; then
    printf 'FAIL quicklook-followrest: the run did not hold (qs exit %s)\n' "$status"
    grep -aE 'TypeError|ReferenceError|Binding loop detected' "$log" | head -3
    failures=$((failures + 1))
fi
printf 'quicklook-followrest: %s failed\n' "$failures"
[ "$failures" -eq 0 ]
