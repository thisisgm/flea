#!/usr/bin/env bash
# Every framed control in the real dialogs, cards, rows and Settings keeps Theme.chromeControlInset from the nearest rule and its host's edge, at stops 12, 14 and 16, at rest and focused; offscreen.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
. "$(dirname "$0")/headless-commons.sh"
cd "$(dirname "$0")/.." || exit 1
python3 -B tests/framegap-picker.py || exit 1

if ! command -v qs >/dev/null; then
    echo "framegap.sh: qs is not installed, cannot draw the hosts"
    exit 1
fi

# A marked sandbox of its own under the fixture root, so cleanup deletes only what this run owns.
test_root=$(mktemp -d "$FIXTURE_ROOT/flea-framegap-XXXXXX") || exit 1
# GNU mktemp -d honours a relative TMPDIR verbatim, so the path is checked absolute and two components deep.
case $test_root in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$test_root', which is not an absolute path two components deep"; exit 1 ;;
esac
sandbox_require "$test_root" || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home/.config" "$test_root/state" "$test_root/data" "$test_root/cache" "$test_root/runtime" || exit 1
chmod 700 "$test_root/runtime" || exit 1
headless_commons "$test_root" || exit 1
# The probe imports ui/ as Flea, and ui/'s qs.Commons resolves against this root, as it does from ui/boot.
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/framegap.qml "$test_root/config/shell.qml" || exit 1
cp tests/framegap.js "$test_root/config/framegap.js" || exit 1
cp tests/button-probe.js "$test_root/config/button-probe.js" || exit 1

run_timeout_s=90
# Every XDG root is pinned under the marked root and no backend answers; the subshell keeps bash's "Terminated" notice out of the report.
output=$( ( env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_CACHE_HOME="$test_root/cache" \
    XDG_DATA_HOME="$test_root/data" XDG_CONFIG_HOME="$test_root/home/.config" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_QPA_UPDATE_IDLE_TIME=1 QT_FORCE_STDERR_LOGGING=1 \
    timeout "$run_timeout_s" qs -p "$test_root/config" 2>&1 ) 2>/dev/null )
# The audit reads every row, passing or not, from this file when it is named.
[ -z "${FRAMEGAP_ROWS:-}" ] || printf '%s\n' "$output" > "$FRAMEGAP_ROWS"

verdict=0
fail() { printf 'FAIL %s\n' "$1"; verdict=1; }
# Sample input: "  INFO qml: FRAMEGAP DONE checks=4672 failed=0 controls=96 controlMiss=0 focusMiss=0" once, after "FRAMEGAP HOSTS 25 ArchiveCompressDialog,...". Every row is one frame side at one stop and state.
expected_preclip_sides=4708 # 1177 frames; a fold stands for its four sides, independent of the Settings scroll position.
expected_hosts=25
# Every GapDialog, GapCheck, GapChrome and GapCrossing side is a control row and must be flagged; GapChrome only draws its frame focused.
expected_controls=96 # 72 plus GapCrossing, 4 sides over 3 stops and 2 states
tally=$(printf '%s\n' "$output" | grep -ao 'FRAMEGAP DONE checks=[0-9]* failed=[0-9]* controls=[0-9]* controlMiss=[0-9]* focusMiss=[0-9]*' | tail -1)
hosts=$(printf '%s\n' "$output" | grep -ao 'FRAMEGAP HOSTS [0-9]*' | grep -o '[0-9]*$')
if printf '%s\n' "$output" | grep -q 'FRAMEGAP FAIL'; then
    fail 'a framed control comes within the inset of a rule or its host edge'
    printf '%s\n' "$output" | grep -ao 'FRAMEGAP FAIL.*' | head -40
fi
# A CLIPPED row is scrolled content cut by the fold, judged by what it shows; only the Settings card scrolls, and the count is positional so it is not pinned.
clipped_rows=$(printf '%s\n' "$output" | grep -ao 'FRAMEGAP CLIPPED.*')
clipped_count=$(printf '%s' "$clipped_rows" | grep -c .)
printf 'FRAMEGAP clipped rows: %s\n' "$clipped_count"
expected_checks=$((expected_preclip_sides - 4 * clipped_count))
printf '%s\n' "$output" > "$test_root/framegap-report.log"
python3 -B tests/framegap-report.py "$test_root/framegap-report.log" "$expected_preclip_sides" \
    || fail 'the raw frame-side inventory is incomplete or changed'
tab=$(printf '\t')
# Sample input: "FRAMEGAP CLIPPED SettingsPanel general<TAB>Rectangle<TAB>..." is a Settings section row and kept out; "FRAMEGAP CLIPPED GapDialog<TAB>..." is foreign.
foreign_clipped=$(printf '%s\n' "$clipped_rows" | grep -v -e '^$' -e "^FRAMEGAP CLIPPED SettingsPanel [^$tab]*$tab")
if [ -n "$foreign_clipped" ]; then
    fail 'a frame outside the Settings card was called clipped by the fold'
    printf '%s\n' "$foreign_clipped" | head -10
fi
[ "$tally" = "FRAMEGAP DONE checks=$expected_checks failed=0 controls=$expected_controls controlMiss=0 focusMiss=0" ] || fail "the run reported '${tally:-nothing}', not $expected_checks checks, 0 failed, $expected_controls controls and no misses"
# The chip rows must have run and none may have failed; the qml PASS line alone is not the verdict.
chip_tally=$(printf '%s\n' "$output" | grep -ao 'FRAMEGAP CHIPS checks=[0-9]* failed=[0-9]*' | tail -1)
case $chip_tally in
  'FRAMEGAP CHIPS checks=0 '*|'') fail 'no picker chip row was read' ;;
  *' failed=0') ;;
  *) fail "a picker chip frame comes within the inset: '$chip_tally'" ;;
esac
[ "${hosts:-0}" = "$expected_hosts" ] || fail "the run measured ${hosts:-0} hosts, not $expected_hosts"
if printf '%s\n' "$output" | grep -q 'FRAMEGAP CONTROLMISS'; then
    fail 'a negative control was not flagged, so the reader may be blind'
    printf '%s\n' "$output" | grep -ao 'FRAMEGAP CONTROLMISS.*' | head -20
fi
if printf '%s\n' "$output" | grep -q 'FRAMEGAP FOCUSMISS'; then
    fail 'the focused pass left a control type unfocused, so it may have re-measured rest'
    printf '%s\n' "$output" | grep -ao 'FRAMEGAP FOCUSMISS.*' | head -20
fi
for control_host in GapDialog GapCheck GapChrome GapCrossing; do
    printf '%s\n' "$output" | grep -q "FRAMEGAP CONTROLROW $control_host" || fail "no flagged control row names $control_host"
done
# The offscreen platform cannot mask a FloatingWindow, and Settings > About asks a flea binary the harness does not ship for its version; neither line is a host's.
platform_warning='This plugin does not support setting window masks'
binary_warning='Process failed to start, likely because the binary could not be found. Command: QList("flea"'
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN' | grep -vF -e "$platform_warning" -e "$binary_warning")
if [ -n "$warnings" ]; then
    fail 'the framegap harness logged a warning'
    printf '%s\n' "$warnings" | head -10
fi
[ "$verdict" -eq 0 ] || exit 1
printf 'FRAMEGAP PASS %s checks over %s hosts plus %s controls, every frame keeps the inset and every control is flagged\n' "$expected_checks" "$expected_hosts" "$expected_controls"
