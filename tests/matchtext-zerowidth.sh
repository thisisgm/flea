#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# matchtext-zerowidth: a late match draws no zero-width run over the cells after the name in Columns, the list, a search row and a row with no match; the controls hold, and a ui tree without the fix fails the late cases.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

if ! command -v qs >/dev/null; then
    echo "matchtext-zerowidth.sh: qs is not installed, cannot draw the views"
    exit 1
fi

sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-matchtext-zerowidth.XXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

# One config root per run: the driver as shell.qml, its stub pane and probes, the Commons and Ui singletons, and a ui tree as flea.
setup_config() {
    local name=$1 uitree=$2 config="$test_root/$1/config"
    mkdir -p "$config" "$test_root/$1/home" "$test_root/$1/state" "$test_root/$1/runtime" || return 1
    chmod 700 "$test_root/$1/runtime" || return 1
    ln -s "$uitree" "$config/flea" || return 1
    headless_commons "$test_root" "$config/Commons" || exit 1
    ln -s "$(readlink -f ui/boot/Ui)" "$config/Ui" || return 1
    # One ShellId per config, so the green and revert processes never share an instance id.
    sed "s/ShellId flea-matchtext-zerowidth-test/ShellId flea-matchtext-zerowidth-$1-test/" tests/matchtext-zerowidth.qml > "$config/shell.qml" || return 1
    cp tests/matchtext-pane.qml "$config/MatchtextPane.qml" || return 1
    cp tests/matchtext-zerowidth.js "$config/matchtext-zerowidth.js" || return 1
}

run_config() {
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
        HOME="$test_root/$1/home" XDG_STATE_HOME="$test_root/$1/state" XDG_RUNTIME_DIR="$test_root/$1/runtime" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        timeout 60 qs -p "$test_root/$1/config" 2>&1
}

# The verdict line of one case in an output: "PASS" or "FAIL" when it printed one.
has() { printf '%s\n' "$1" | grep -aq "MATCHTEXT $2 $3 "; }

setup_config green "$PWD/ui" || { echo "matchtext-zerowidth.sh: could not build the green config"; exit 1; }
green=$(run_config green)
green_rc=$?

# The revert is a copy of ui/ in the fixture with the three zero-width fix lines removed, built only while ui/ carries them, so the tree is never touched.
fix_lines=$(grep -c '^[[:space:]]*visible: width > 0$' ui/MatchText.qml)
revert=""
if [ "$fix_lines" -eq 3 ]; then
    mkdir -p "$test_root/revert" || exit 1
    cp -a ui "$test_root/revert/flea" || exit 1
    sed -i '/^[[:space:]]*visible: width > 0$/d' "$test_root/revert/flea/MatchText.qml" || exit 1
    setup_config revert "$test_root/revert/flea" || exit 1
    revert=$(run_config revert)
fi

late="columns-late list-late search-late"
controls="columns-early list-early nomatch search-early pixels-detect"
bad=0

# A partial fix is a broken tree, not a revert to compare against.
if [ "$fix_lines" -ne 3 ] && [ "$fix_lines" -ne 0 ]; then
    printf 'FAIL matchtext: ui/MatchText.qml holds %s of 3 zero-width fix lines, a partial fix\n' "$fix_lines"
    bad=1
fi
# The driver ends its own qs with SIGTERM after DONE, so the owned self-kill 143 is the clean exit.
if [ "$green_rc" -ne 143 ] || ! printf '%s\n' "$green" | grep -aq 'MATCHTEXT DONE .* failed=0$'; then
    bad=1
fi
# On an unfixed ui/ the green run's own late cases fail, which is the signal; no revert is needed to see it.
for c in $late $controls; do
    has "$green" PASS "$c" || bad=1
done
if [ "$fix_lines" -eq 3 ]; then
    for c in $late; do
        has "$revert" FAIL "$c" || { printf 'FAIL matchtext: the revert did not fail %s\n' "$c"; bad=1; }
    done
    for c in $controls; do
        has "$revert" PASS "$c" || { printf 'FAIL matchtext: the revert broke control %s\n' "$c"; bad=1; }
    done
fi
noisy=$(printf '%s\n' "$green" "$revert" | grep -av 'does not support setting window masks' | grep -aiE 'WARN|ERROR|Binding loop|failed to load|Unable to assign|Cannot assign|TypeError|ReferenceError|is not a function|is not defined')
[ -n "$noisy" ] && bad=1

if [ "$bad" -ne 0 ]; then
    printf '%s\n' "$green" | grep -a 'MATCHTEXT' | head -40
    printf '%s\n' "$green" | grep -aiE 'ERROR|error' | head -20
    [ -n "$noisy" ] && printf '%s\n' "$noisy" | head -20
    printf '%s\n' "$green" | tail -20
    if [ -n "$revert" ]; then
        printf 'matchtext: revert output tail follows\n'
        printf '%s\n' "$revert" | tail -20
    fi
    exit 1
fi
printf '%s\n' "$green" | grep -ao 'MATCHTEXT PASS.*'
if [ "$fix_lines" -eq 3 ]; then
    # The revert's own verdicts, reworded so the suite's FAIL contract does not read them as failures.
    printf '%s\n' "$revert" | grep -a 'MATCHTEXT FAIL ' | sed 's/MATCHTEXT FAIL /MATCHTEXT expected: revert failed /'
    printf 'MATCHTEXT ok: green passes all %s cases; the revert fails the %s late cases and holds the %s controls\n' "$(printf '%s\n' $late $controls | wc -l)" "$(printf '%s\n' $late | wc -l)" "$(printf '%s\n' $controls | wc -l)"
else
    printf 'MATCHTEXT ok: green passes all %s cases on ui/\n' "$(printf '%s\n' $late $controls | wc -l)"
fi
