#!/usr/bin/env bash
# headerautofit (#333): the header menu's Autofit columns row brings back Modified and Kind under the
# report's stored widths, through the same Focus.act route as F4. Stages headercost's in-memory
# ViewState so no ui.json is written.
. "$(dirname "$0")/headless-commons.sh"
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "headerautofit.sh: qs is not installed, cannot build a header"
    exit 1
fi

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-headerautofit.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
# The write spy is supplied before construction: a shadow module where every entry links
# the tree file except ViewState.qml, which is the test-only double. Header code under test stays
# the identical inode. ln never follows sources, so the OEM
# boot links stay dangling outside the root without copying them.
stage="$test_root/stage"
mkdir -p "$stage/ui" || exit 1
for src in "$PWD"/ui/*; do
    base=${src##*/}
    [ "$base" = "ViewState.qml" ] && continue
    ln -s "$src" "$stage/ui/$base" || exit 1
done
cp tests/headercost-viewstate.qml "$stage/ui/ViewState.qml" || exit 1
ln -s "$stage/ui" "$test_root/config/flea" || exit 1
# readlink -m, not -f: identical where the OEM shell exists, and a dangling link where it does not, so a box without it still reaches qs and fails with qs's own error in the log.
headless_commons "$test_root" || exit 1
ln -s "$(readlink -m ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/headerautofit.qml "$test_root/config/shell.qml" || exit 1

# Sample input, one probe line: "  INFO qml: HEADERAUTOFIT PASS checks=9".
log="$test_root/headerautofit.log"
( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    timeout 20 qs -p "$test_root/config" > "$log" 2>&1; exit $? ) 2>/dev/null
status=$?

pass_count=$(grep -ac 'HEADERAUTOFIT PASS' "$log")
fail_count=$(grep -ac 'HEADERAUTOFIT FAIL' "$log")
done_count=$(grep -ac 'HEADERAUTOFIT DONE' "$log")
platform_warning='This plugin does not support setting window masks'
warnings=$(grep -aE 'TypeError|ReferenceError|ERROR|WARN|Cannot|Unable to assign|is not a type|failed to load' "$log" | grep -vF "$platform_warning" || true)

# 143 is the fixture's owned self-kill, 124 the timeout; only a finished fixture proves its counts.
if [ "$done_count" -ne 1 ] || [ "$status" -eq 124 ] || { [ "$status" -ne 0 ] && [ "$status" -ne 143 ]; }; then
    printf 'FAIL headerautofit: no clean completion (qs exit %s, done=%s)\n' "$status" "$done_count"
    cat "$log"
    exit 1
fi
if [ "$fail_count" -ne 0 ] || [ "$pass_count" -ne 1 ] || [ -n "$warnings" ]; then
    printf 'FAIL headerautofit: want one PASS, no FAIL and no engine warning (pass=%s fail=%s)\n' "$pass_count" "$fail_count"
    grep -a 'HEADERAUTOFIT FAIL' "$log"
    printf '%s\n' "$warnings"
    exit 1
fi
grep -a 'HEADERAUTOFIT PASS' "$log" | tail -1
