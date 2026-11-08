#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# The real KeymapSheet over a stub pane draws CommandPalette's query states, then over the real pane runs its menu rows, offscreen with no display or lock.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "sheet-query.sh: qs is not installed, cannot build the sheet"
    exit 1
fi

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-sheetquery.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/sheet-query.qml "$test_root/config/shell.qml" || exit 1

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    timeout 30 qs -p "$test_root/config" 2>&1)
qs_status=$?

# Sample input: "  INFO qml: SHEETQUERY PASS checks=18" then "  INFO qml: SHEETQUERY DONE failures=0".
# The owned termination is the probe's own self-kill (SIGTERM, 143) after its one DONE receipt.
pass_count=$(printf '%s\n' "$output" | grep -c 'SHEETQUERY PASS')
fail_count=$(printf '%s\n' "$output" | grep -c 'SHEETQUERY FAIL')
done_count=$(printf '%s\n' "$output" | grep -c 'SHEETQUERY DONE')
verdict=0
if [ "$qs_status" -ne 143 ]; then
    printf 'FAIL qs exited %s, want the owned self-kill 143 after DONE\n' "$qs_status"
    verdict=1
fi
if [ "$done_count" -ne 1 ]; then
    printf 'FAIL completion receipts %s, want exactly 1 DONE beside the PASS\n' "$done_count"
    verdict=1
fi
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ]; then
    printf 'FAIL the sheet missed one of CommandPalette query states\n'
    printf '%s\n' "$output" | grep -aE 'SHEETQUERY|ERROR|error'
    verdict=1
fi
# The offscreen platform itself says it cannot mask a FloatingWindow; that one line is the platform's, never the sheet's.
platform_warning='This plugin does not support setting window masks'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN|ERROR' | grep -vF "$platform_warning")
if [ -n "$warnings" ]; then
    printf 'FAIL the sheet harness logged a warning\n'
    printf '%s\n' "$warnings"
    verdict=1
fi
if [ "$verdict" -ne 0 ]; then
    printf '%s\n' "$output"
    exit 1
fi
printf '%s\n' "$output" | grep -o 'SHEETQUERY PASS.*'
printf 'SHEETQUERY STATUS qs_exit=%s done=1\n' "$qs_status"

# The pane half: the real WindowBody and backend, so a menu row Enter runs meets the menu's own validation.
bin=${FLEA_BIN:-$PWD/target/debug/flea}
command -v dbus-run-session >/dev/null || { echo 'FAIL sheet-query pane half needs private D-Bus'; exit 1; }
[[ -x "$bin" ]] || { echo 'FAIL sheet-query pane half needs the candidate backend'; exit 1; }
pane_root="$test_root/pane"
mkdir -p "$pane_root"/{home,config,state,data,cache,runtime} "$pane_root/home/fixture" || exit 1
chmod 700 "$pane_root/runtime" || exit 1
for name in a-special.txt b.txt c.txt d.txt; do printf 'sheet query fixture\n' > "$pane_root/home/fixture/$name"; done
# The shipped list shows Permissions, so the profile hides it on top of that list for the hidden-row checks.
env HOME="$pane_root/home" XDG_STATE_HOME="$pane_root/state" "$bin" --ui-state \
    '{"view":"list","keys":"default","preview":{"column":false,"thumbnails":"off"},"updates":{"autoCheck":false},"display":{"textSize":{"mode":14}},"menu":{"hidden":["delete","openTerminal","placeMenu","runScript","moveto","copyto","permissions","copyAs","pasteAs","invertSelection","extThumbs","run","openAsText","openAgent","toggleHidden","toggleLangMarks"]}}' >/dev/null || exit 1
ln -s "$PWD/ui" "$pane_root/config/flea" || exit 1
headless_commons "$pane_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$pane_root/config/Ui" || exit 1
ln -s "$PWD/ui/boot/fleatab.qml" "$pane_root/config/fleatab.qml" || exit 1
cp tests/sheet-query-pane.qml "$pane_root/config/shell.qml" || exit 1
pane_log="$pane_root/qs.log"
readonly paneLimitSeconds=60 paneChecks=33 paneQsStatus=143
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
    HOME="$pane_root/home" XDG_STATE_HOME="$pane_root/state" XDG_CONFIG_HOME="$pane_root/config" \
    XDG_DATA_HOME="$pane_root/data" XDG_CACHE_HOME="$pane_root/cache" XDG_RUNTIME_DIR="$pane_root/runtime" \
    FLEA_PATH="$pane_root/home/fixture" FLEA_BIN="$bin" GIO_USE_VOLUME_MONITOR=unix QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout "$paneLimitSeconds" dbus-run-session -- qs -p "$pane_root/config" > "$pane_log" 2>&1
pane_status=$?
# Sample input: "  INFO qml: SHEETPANE ok  Permissions the query reads back whole: got perm, expected perm" and "SHEETPANE DONE checks=33 failed=0".
pane_verdict=0
[[ "$pane_status" == "$paneQsStatus" ]] || { printf 'FAIL pane half: qs exit %s, expected %s after backend drain\n' "$pane_status" "$paneQsStatus"; pane_verdict=1; }
rg -q "SHEETPANE DONE checks=$paneChecks failed=0" "$pane_log" || { echo 'FAIL pane half: tally is not every check passed'; pane_verdict=1; }
[[ "$(rg -c 'SHEETPANE ok ' "$pane_log")" == "$paneChecks" ]] || { echo 'FAIL pane half: passed count'; pane_verdict=1; }
if rg 'SHEETPANE FAIL|TypeError|ReferenceError|ERROR|Cannot assign|WARN' "$pane_log" | rg -vF "$platform_warning"; then
    echo 'FAIL pane half: a failed check or an engine warning'
    pane_verdict=1
fi
if [ "$pane_verdict" -ne 0 ]; then
    rg 'SHEETPANE' "$pane_log"
    exit 1
fi
printf 'SHEETPANE STATUS qs_exit=%s checks=%s\n' "$pane_status" "$paneChecks"
