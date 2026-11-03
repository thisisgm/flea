#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
# The real entry offscreen: a glass launch clears the window before it exists, and any other launch keeps the opaque literal.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL glass-boot needs qs'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL glass-boot needs a private D-Bus session'; exit 1; }
command -v jq >/dev/null || { echo 'FAIL glass-boot needs jq'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL glass-boot needs the built backend'; exit 1; }
probe_root=""
work=""
runner=""
cleanup() {
    [[ -n "$runner" ]] && kill -TERM -- "-$runner" 2>/dev/null
    [[ -n "$runner" ]] && wait "$runner" 2>/dev/null
    [[ -n "$work" ]] && sandbox_remove "$work"
    # Hard rule 9: only the root this run made leaves /tmp, so a foreign path refuses instead of deleting.
    if [[ -n "$probe_root" ]]; then
        [[ "$probe_root" == /* ]] || { printf 'REFUSED: glass-boot cleanup: not absolute: %s\n' "$probe_root" >&2; exit 1; }
        case "$probe_root" in
            /tmp/flea-gb.*) ;;
            *) printf 'REFUSED: glass-boot cleanup: not the root this run made: %s\n' "$probe_root" >&2; exit 1 ;;
        esac
        if [[ -f "$probe_root/$SANDBOX_MARKER" ]]; then
            # The root holds only its marker now; rmdir refuses anything that is not empty.
            rm -f -- "$probe_root/$SANDBOX_MARKER" && rmdir -- "$probe_root"
        else
            # No marker yet (a refusal or failed write before it): this run's fresh dir, so rmdir takes it only if empty.
            rmdir -- "$probe_root" 2>/dev/null || true
        fi
    fi
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# A short root under /tmp, the way tests/shellload.sh makes one: the seam's socket path must fit the 108-byte sun_path.
sandbox_forbidden /tmp && sandbox_refuse "glass-boot: /tmp is inside a forbidden test target"
probe_root=$(mktemp -d /tmp/flea-gb.XXXXXX) || exit 1
FIXTURE_ROOT=$probe_root
sandbox_root_ok
probe_root=$SANDBOX_ROOT
: > "$probe_root/$SANDBOX_MARKER" || exit 1
work=$probe_root/w
# The seam is built after the first rows, so readiness is polled; each call is bounded the way tests/xwstate.sh bounds it.
ready_polls=200
poll_s=0.1
call_timeout_s=5

failed=0
# A leg that cannot be set up is a failure, so a suite that tested nothing never reads as green.
setup_failed() {
    printf 'FAIL glass-boot %s: %s\n' "$1" "$2"
    failed=$((failed + 1))
}
# leg NAME THEME_VALUE CLEAR WINDOW EARLY [MODE]: an absent theme value keeps the mode's own ground.
leg() {
    local name=$1 value=$2 want_clear=$3 want_window=$4 want_early=$5 mode=${6:-transparent}
    local probe="$work/$name" answer attempt
    local theme_dir="$probe/home/.local/state/omarchy/current/theme"
    mkdir -p "$probe"/{home/fixture,state,data,cache,runtime,config} "$theme_dir" && chmod 700 "$probe/runtime" \
        || { setup_failed "$name" "mkdir of the probe directories failed"; return 1; }
    printf 'a\n' > "$probe/home/fixture/a.txt" \
        && printf 'background = "#1e2030"\nforeground = "#d0d4e0"\naccent = "#7aa2f7"\n' > "$theme_dir/colors.toml" \
        || { setup_failed "$name" "writing the fixture and palette failed"; return 1; }
    if [[ "$value" != - ]]; then
        printf '[flea]\nbackground-alpha = %s\n' "$value" > "$theme_dir/shell.toml" \
            || { setup_failed "$name" "writing the theme's shell.toml failed"; return 1; }
    fi
    env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state '{"updates":{"autoCheck":false}}' >/dev/null \
        || { setup_failed "$name" "$bin --ui-state failed"; return 1; }
    if [[ "$mode" == opaque ]]; then
        mkdir -p "$probe/state/omarchy/toggles/hypr" && printf 'return\n' > "$probe/state/omarchy/toggles/hypr/window-opaque.lua" \
            || { setup_failed "$name" "writing its opaque mode failed"; return 1; }
    fi
    # No FUSE bridge: gvfs would mount one in the runtime dir that can outlive the shell and block the cleanup.
    setsid env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$probe/home" XDG_CONFIG_HOME="$probe/config" XDG_STATE_HOME="$probe/state" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_PATH="$probe/home/fixture" FLEA_BIN="$bin" GVFS_DISABLE_FUSE=1 \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        dbus-run-session -- qs -p "$PWD/ui/boot" > "$probe/qs.log" 2>&1 < /dev/null &
    runner=$!
    answer=""
    # qs ipc matches only instances on its own display, so the call goes without one, as the shell does.
    for ((attempt = 0; attempt < ready_polls; attempt++)); do
        answer=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE XDG_RUNTIME_DIR="$probe/runtime" QT_QPA_PLATFORM=offscreen timeout "$call_timeout_s" "$FLEA_IPC_HELPER" -p "$PWD/ui/boot" call flea glass 2>/dev/null) && [[ "$answer" == '{'* ]] && break
        answer=""
        sleep "$poll_s"
    done
    kill -TERM -- "-$runner" 2>/dev/null
    wait "$runner" 2>/dev/null
    runner=""
    if [[ -z "$answer" ]]; then
        printf 'FAIL glass-boot %s: the seam never answered; log tail:\n' "$name"
        tail -10 "$probe/qs.log"
        failed=$((failed + 1))
        return 0
    fi
    local clear window early
    clear=$(jq -r .clear <<< "$answer")
    window=$(jq -r .window <<< "$answer")
    early=$(jq -r .early <<< "$answer")
    if [[ "$clear" == "$want_clear" && "$window" == "$want_window" && "$early" == "$want_early" ]]; then
        printf 'ok   glass-boot %s: clear=%s window=%s early=%s\n' "$name" "$clear" "$window" "$early"
    else
        printf 'FAIL glass-boot %s: clear=%s window=%s early=%s, expected clear=%s window=%s early=%s\n' "$name" "$clear" "$window" "$early" "$want_clear" "$want_window" "$want_early"
        failed=$((failed + 1))
    fi
}

leg glass 0.34 true '#00000000' true
leg opaque - true '#00000000' true opaque
leg transparent - true '#00000000' true
leg rounds 0.999 false '#101315' false
printf 'glass-boot: failed=%s\n' "$failed"
[[ "$failed" -eq 0 ]]
