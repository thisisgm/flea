#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# [flea] background-alpha through the real WindowBody offscreen, one launch per case with its own theme and user shell.toml.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL glass needs qs'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL glass needs a private D-Bus session'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL glass needs the built backend'; exit 1; }
sandbox_root_ok
probe_root=$(mktemp -d "$SANDBOX_ROOT/flea-glass.XXXXXX") || exit 1
: > "$probe_root/$SANDBOX_MARKER" || exit 1
qs_pid=""
# Polled ten times a second up to these bounds; the sweep walks every view and overlay, so it gets the longer one.
poll_s=0.1
polls_per_s=10
parse_budget_s=60
sweep_budget_s=150
stop_grace_s=2
stop_polls=$(( stop_grace_s * polls_per_s ))
stop_qs() {
    [[ -n "$qs_pid" ]] || return 0
    local tick
    kill -TERM -- "-$qs_pid" 2>/dev/null || true
    for ((tick = 0; tick < stop_polls; tick++)); do
        kill -0 -- "-$qs_pid" 2>/dev/null || break
        sleep "$poll_s"
    done
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        kill -KILL -- "-$qs_pid" 2>/dev/null || true
        for ((tick = 0; tick < stop_polls; tick++)); do
            kill -0 -- "-$qs_pid" 2>/dev/null || break
            sleep "$poll_s"
        done
    fi
    wait "$qs_pid" 2>/dev/null || true
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        printf 'FAIL glass owned process group %s survived teardown\n' "$qs_pid"
        return 1
    fi
    qs_pid=""
}
cleanup() { stop_qs || return 1; sandbox_remove "$probe_root"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

# A palette whose surface and background differ, so a plane read in the wrong role fails by colour.
palette='background = "#1e2030"
dark_background = "#2c3045"
foreground = "#d0d4e0"
accent = "#7aa2f7"
color1 = "#e06c75"
color2 = "#98c379"
color6 = "#56b6c2"
color8 = "#6b7089"'

failed=0
total=0
# A case that cannot be set up is a failure, so a suite that tested nothing never reads as green.
setup_failed() {
    printf 'FAIL glass %s: %s\n' "$1" "$2"
    failed=$((failed + 1))
}
# run_case NAME MODE EXPECT THEME USER [LATE [LATE_EXPECT]]: THEME and USER are the [flea] value, "-" for no file and "" for a file without one.
run_case() {
    local name=$1 mode=$2 expect=$3 theme=$4 user=$5 late=${6:-} late_expect=${7:-$3} stock=0
    local probe="$probe_root/$name" budget=$parse_budget_s
    local theme_dir="$probe/home/.local/state/omarchy/current/theme"
    [[ "$mode" == sweep ]] && budget=$sweep_budget_s
    mkdir -p "$probe"/{home,config,state,data,cache,runtime} "$probe/home/fixture/sub" "$theme_dir" "$probe/home/.config/omarchy" \
        && chmod 700 "$probe/runtime" || { setup_failed "$name" "mkdir of the probe directories failed"; return 1; }
    if [[ -n "${GLASS_SHOTS:-}" ]]; then
        mkdir -p "$GLASS_SHOTS/$name" || { setup_failed "$name" "mkdir of $GLASS_SHOTS/$name failed"; return 1; }
    fi
    printf 'a\n' > "$probe/home/fixture/a.txt" && printf 'b\n' > "$probe/home/fixture/b.txt" \
        && printf 'glass\n' > "$probe/home/.local/state/omarchy/current/theme.name" \
        && printf '%s\n' "$palette" > "$theme_dir/colors.toml" || { setup_failed "$name" "writing the fixture and palette failed"; return 1; }
    if [[ "$theme" != - ]]; then
        printf '[font]\nbase-size = 14\n\n[flea]\n%s\n' "${theme:+background-alpha = $theme}" > "$theme_dir/shell.toml" \
            || { setup_failed "$name" "writing the theme's shell.toml failed"; return 1; }
    fi
    if [[ "$user" != - ]]; then
        printf '[flea]\n%s\n' "${user:+background-alpha = $user}" > "$probe/home/.config/omarchy/shell.toml" \
            || { setup_failed "$name" "writing the user's shell.toml failed"; return 1; }
    fi
    env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state \
        '{"preview":{"thumbnails":"off"},"updates":{"autoCheck":false}}' >/dev/null \
        || { setup_failed "$name" "$bin --ui-state failed"; return 1; }
    if [[ "$name" == none ]]; then
        mkdir -p "$probe/state/omarchy/toggles/hypr" && printf 'return\n' > "$probe/state/omarchy/toggles/hypr/window-opaque.lua" \
            || { setup_failed "$name" "writing its explicit opaque mode failed"; return 1; }
    fi
    if [[ "$name" == stock-glassy ]]; then
        stock=1
        mkdir -p "$probe/state/omarchy/toggles/hypr" && printf 'return\n' > "$probe/state/omarchy/toggles/hypr/window-glassy.lua" \
            || { setup_failed "$name" "writing its glassy mode failed"; return 1; }
    fi
    # The window loads its tab catcher from the shell directory, so the probe ships it as the product does.
    ln -s "$PWD/ui" "$probe/config/flea" || { setup_failed "$name" "linking the probe's UI failed"; return 1; }
    headless_commons "$probe" || exit 1
    ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui" && ln -s "$PWD/ui/boot/fleatab.qml" "$probe/config/fleatab.qml" \
        && cp tests/glass.qml "$probe/config/shell.qml" && cp tests/glass.js "$probe/config/glass.js" \
        || { setup_failed "$name" "linking the probe's shell directory failed"; return 1; }
    local log="$probe/qs.log" tick
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_PATH="$probe/home/fixture" FLEA_BIN="$bin" GLASS_MODE="$mode" GLASS_EXPECT="$expect" GLASS_STOCK="$stock" \
        GLASS_LATE_USER="$late" GLASS_LATE_EXPECT="$late_expect" GLASS_SHOTS="${GLASS_SHOTS:+$GLASS_SHOTS/$name}" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
        setsid dbus-run-session -- qs -p "$probe/config" >"$log" 2>&1 </dev/null &
    qs_pid=$!
    for ((tick = 0; tick < budget * polls_per_s; tick++)); do
        grep -saq 'GLASS DONE checks=[0-9]* failed=[0-9]*' "$log" && break
        sleep "$poll_s"
    done
    stop_qs || { setup_failed "$name" "its shell did not stop"; return 1; }
    grep -a 'GLASS FAIL' "$log" | sed "s/^/  $name: /"
    local tally
    tally=$(grep -ao 'GLASS DONE checks=[0-9]* failed=[0-9]*' "$log" | tail -1)
    if [[ -z "$tally" ]]; then
        printf 'FAIL glass %s produced no receipt inside %ss; log tail:\n' "$name" "$budget"
        tail -20 "$log"
        failed=$((failed + 1))
        return 0
    fi
    if grep -aqE 'TypeError|ReferenceError|ERROR|Cannot assign|Binding loop' "$log"; then
        printf 'FAIL glass %s logged an engine error beside its receipt\n' "$name"
        grep -aE 'TypeError|ReferenceError|ERROR|Cannot assign|Binding loop' "$log" | head -5
        failed=$((failed + 1))
    fi
    # Sample input, one log line: "GLASS DONE checks=92 failed=0"
    local checks=${tally#*checks=}
    checks=${checks%% *}
    total=$((total + checks))
    if [[ "$tally" == *' failed=0' ]]; then
        printf 'ok   glass %s (%s)\n' "$name" "${tally#GLASS DONE }"
    else
        printf 'FAIL glass %s (%s)\n' "$name" "${tally#GLASS DONE }"
        failed=$((failed + 1))
    fi
}

# Every plane and overlay at 0.34 from the theme, then live changes from the user's file.
run_case sweep sweep 87 0.34 ""
run_case stock-glassy sweep 192 - -
# The user's file alone, and the user's value over the theme's.
run_case user parse 128 - 0.5
run_case override parse 153 0.34 0.6
# What is not a number is the default, and a number outside 0 to 1 is clamped, as Omarchy's own alpha keys are.
run_case word parse 255 abc -
run_case quoted parse 255 '"x"' -
run_case above parse 255 1.5 -
run_case below parse 0 -0.2 -
# A value that rounds to 1 in 8 bits is 1, and so is no signal at all: bare Transparent draws the theme opaque.
run_case rounds parse 255 0.999 -
run_case bare parse 255 - -
# A launch no value pins is clear, so a value written after it shows at once; one a value pins opaque holds 1 until the next.
run_case none parse 255 - - 0.34 87
run_case pinned parse 255 - 0.999 0.34 255

printf 'glass: checks=%s failed=%s\n' "$total" "$failed"
[[ "$failed" -eq 0 ]]
