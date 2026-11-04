#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Omarchy's Window Transparency precedence through Glass, one headless launch per case, and a Hyprland config reload with no theme switch.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL glass-mode needs qs'; exit 1; }
command -v python3 >/dev/null || { echo 'FAIL glass-mode needs python3'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL glass-mode needs a private D-Bus session'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL glass-mode needs the built backend'; exit 1; }
sandbox_root_ok
probe_root=$(mktemp -d "$SANDBOX_ROOT/flea-glass-mode.XXXXXX") || exit 1
: > "$probe_root/$SANDBOX_MARKER" || exit 1
qs_pid=""
server_pid=""
# Polled ten times a second up to these bounds: one case gets 60 s to print its receipt, a shell gets 2 s to stop, the socket stand-in 5 s to bind.
poll_s=0.1
polls_per_s=10
case_budget_s=60
socket_wait_s=5
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
        printf 'FAIL glass-mode owned process group %s survived teardown\n' "$qs_pid"
        return 1
    fi
    qs_pid=""
}
cleanup() { [[ -n "$server_pid" ]] && kill "$server_pid" 2>/dev/null; stop_qs || return 1; sandbox_remove "$probe_root"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM

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
setup_failed() {
    printf 'FAIL glass-mode %s: %s\n' "$1" "$2"
    failed=$((failed + 1))
}
# run_case NAME MODE TOGGLE EXPECT THEME USER [GLASS]: TOGGLE is opaque, glassy or "-"; THEME and USER are the [flea] value, "-" for no file and "" for a file without one; GLASS sets Glass.glassAlpha in the harness, "-" keeps the shipped default.
run_case() {
    local name=$1 mode=$2 toggle=$3 expect=$4 theme=$5 user=$6 glass=${7:--}
    local probe="$probe_root/$name" budget=$case_budget_s
    local theme_dir="$probe/home/.local/state/omarchy/current/theme" toggle_dir="$probe/state/omarchy/toggles/hypr"
    mkdir -p "$probe"/{home,config,state,data,cache,runtime,bin} "$theme_dir" "$probe/home/.config/omarchy" "$toggle_dir" \
        && chmod 700 "$probe/runtime" || { setup_failed "$name" "mkdir of the probe directories failed"; return 1; }
    printf 'glass\n' > "$probe/home/.local/state/omarchy/current/theme.name" \
        && printf '%s\n' "$palette" > "$theme_dir/colors.toml" || { setup_failed "$name" "writing the palette failed"; return 1; }
    # A stand-in for hyprctl that answers decoration:rounding from a file, so a reload can be seen to move it, and lists this
    # shell's one window, answers its opacity from a file (Omarchy's 0.985 while it carries the tag), logs each read, and logs a
    # tag change only when the tag's state actually changes, as setting a tag Hyprland already holds changes nothing.
    printf '10\n' > "$probe/rounding" && cat > "$probe/bin/hyprctl" <<'STUB' && chmod +x "$probe/bin/hyprctl" \
        || { setup_failed "$name" "writing the hyprctl stand-in failed"; return 1; }
#!/bin/sh
case "$*" in
    *decoration:rounding*) printf '{"option":"decoration:rounding","int":%s,"set":true}\n' "$(cat "$ROUNDING_FILE")" ;;
    *gaps_out*) echo '{"int":0,"css":"0 0 0 0"}' ;;
    *monitors*) echo '[]' ;;
    *"clients -j"*) printf '[{"address":"0xabc123","pid":%s,"class":"com.thisisgm.flea","tags":[%s]}]\n' "$PPID" "$(cat "$TAGS_FILE")" ;;
    *getprop*) printf '%s\n' "$*" >> "$PROBE_LOG"; if [ -s "$TAGS_FILE" ]; then echo 0.985; else cat "$OPACITY_FILE"; fi ;;
    *+transparent*) [ -s "$TAGS_FILE" ] || { printf '%s\n' "$*" >> "$DISPATCH_LOG"; printf '"transparent"' > "$TAGS_FILE"; }; echo ok ;;
    *-transparent*) [ -s "$TAGS_FILE" ] && { printf '%s\n' "$*" >> "$DISPATCH_LOG"; : > "$TAGS_FILE"; }; echo ok ;;
    *) echo '{"bool":false}' ;;
esac
STUB
    [[ "$toggle" == - ]] || printf 'return\n' > "$toggle_dir/window-$toggle.lua" || { setup_failed "$name" "writing the toggle failed"; return 1; }
    # Hyprland's own opacity for the window: 1 where Omarchy leaves it alone, 0.985 where stable Omarchy's catch-all rule draws it.
    local opacity=1
    [[ "$mode" == tag-stable ]] && opacity=0.985
    printf '%s\n' "$opacity" > "$probe/opacity" && : > "$probe/tag.log" && : > "$probe/probe.log" && : > "$probe/tags" \
        || { setup_failed "$name" "writing the opacity stand-in failed"; return 1; }
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
    ln -s "$PWD/ui" "$probe/config/flea" || { setup_failed "$name" "linking the probe's UI failed"; return 1; }
    headless_commons "$probe" || exit 1
    ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui" && cp tests/glass-mode.qml "$probe/config/shell.qml" \
        || { setup_failed "$name" "linking the probe's shell directory failed"; return 1; }
    local log="$probe/qs.log" tick sig=()
    if [[ "$mode" == socket || "$mode" == tag* ]]; then
        # A stand-in for Hyprland's event socket, so the raw event takes the path it takes in the product.
        mkdir -p "$probe/runtime/hypr/glassmode" || { setup_failed "$name" "mkdir of the socket directory failed"; return 1; }
        python3 -I tests/glass-mode-socket.py "$probe/runtime/hypr/glassmode" "$probe/go" >"$probe/socket.log" 2>&1 &
        server_pid=$!
        for ((tick = 0; tick < socket_wait_s * polls_per_s; tick++)); do
            [[ -S "$probe/runtime/hypr/glassmode/.socket2.sock" ]] && break
            sleep "$poll_s"
        done
        [[ -S "$probe/runtime/hypr/glassmode/.socket2.sock" ]] || { setup_failed "$name" "the event socket never appeared inside ${socket_wait_s}s"; kill "$server_pid" 2>/dev/null; server_pid=""; return 1; }
        sig=(HYPRLAND_INSTANCE_SIGNATURE=glassmode)
    fi
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT "${sig[@]}" \
        HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" PATH="$probe/bin:$PATH" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_BIN="$bin" ROUNDING_FILE="$probe/rounding" OPACITY_FILE="$probe/opacity" DISPATCH_LOG="$probe/tag.log" PROBE_LOG="$probe/probe.log" TAGS_FILE="$probe/tags" GLASSM_DISPATCH="$probe/tag.log" GLASSM_PROBE="$probe/probe.log" GLASSM_ROUNDING="$probe/rounding" GLASSM_MODE="$mode" GLASSM_EXPECT="$expect" GLASSM_GLASS="$glass" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
        setsid dbus-run-session -- qs -p "$probe/config" >"$log" 2>&1 </dev/null &
    qs_pid=$!
    for ((tick = 0; tick < budget * polls_per_s; tick++)); do
        grep -saq 'GLASSM DONE checks=[0-9]* failed=[0-9]*' "$log" && break
        sleep "$poll_s"
    done
    stop_qs || { setup_failed "$name" "its shell did not stop"; return 1; }
    if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null; wait "$server_pid" 2>/dev/null; server_pid=""; fi
    grep -a 'GLASSM FAIL' "$log" | sed "s/^/  $name: /"
    grep -a 'GLASSM EDGE' "$log" || true
    local tally
    tally=$(grep -ao 'GLASSM DONE checks=[0-9]* failed=[0-9]*' "$log" | tail -1)
    if [[ -z "$tally" ]]; then
        printf 'FAIL glass-mode %s produced no receipt inside %ss; log tail:\n' "$name" "$budget"
        tail -20 "$log"
        failed=$((failed + 1))
        return 0
    fi
    if grep -aqE 'TypeError|ReferenceError|ERROR|Cannot assign|Binding loop' "$log"; then
        printf 'FAIL glass-mode %s logged an engine error beside its receipt\n' "$name"
        grep -aE 'TypeError|ReferenceError|ERROR|Cannot assign|Binding loop' "$log" | head -5
        failed=$((failed + 1))
    fi
    # Sample input, one log line: "GLASSM DONE checks=2 failed=0"
    local checks=${tally#*checks=}
    checks=${checks%% *}
    total=$((total + checks))
    if [[ "$tally" == *' failed=0' ]]; then
        printf 'ok   glass-mode %s (%s)\n' "$name" "${tally#GLASSM DONE }"
    else
        printf 'FAIL glass-mode %s (%s)\n' "$name" "${tally#GLASSM DONE }"
        failed=$((failed + 1))
    fi
}

# Row (a): the user's own value wins over every mode and over the theme's.
run_case user-over-opaque precedence opaque 128 0.34 0.5
run_case user-over-glassy precedence glassy 153 0.34 0.6
# Row (b): opaque is 1, even over a theme value.
run_case opaque precedence opaque 255 0.34 -
# Row (c): glassy takes the theme's value, else its pick; the synthetic override still proves the 0.75 path.
run_case glassy-theme precedence glassy 87 0.34 -
run_case glassy-bare precedence glassy 191 - - 0.75
run_case glassy-empty-theme precedence glassy 191 "" - 0.75
run_case glassy-default precedence glassy 192 - -
# Row (d): transparent takes the theme's value, else opaque at 255/255: no shipped theme sets an opacity and no toggle file means transparency was never chosen.
run_case transparent-theme precedence - 87 0.34 -
run_case transparent-bare precedence - 255 - -
# A value that is not a number is the default 1 where it is set, as before the mode existed.
run_case user-word precedence glassy 255 0.34 abc
# The same through a socket speaking Hyprland's event protocol: a workspace event sent after the rounding changed moves nothing, configreloaded moves the radius.
run_case socket socket - 255 - -
# A reload moves the mode and the corner radius under a running window.
run_case reload reload glassy 191 - - 0.75
# Flea's own window takes Omarchy's transparency tag where Hyprland leaves it at 1, gives it up in Glassy, and leaves a window
# Hyprland already draws below 1 to that rule.
run_case tag tag - 255 - -
run_case tag-stable tag-stable - 255 - -

printf 'glass-mode: checks=%s failed=%s\n' "$total" "$failed"
[[ "$failed" -eq 0 ]] || exit 1
