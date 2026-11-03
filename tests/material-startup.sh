#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
# Count the frozen QML oracle's Glassy alpha search in an instrumented disposable UI copy.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
. "$PWD/tests/headless-commons.sh"
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL material-startup needs the built backend'; exit 1; }
sandbox_root_ok
work=$(mktemp -d "$SANDBOX_ROOT/flea-material-startup.XXXXXX") || exit 1
: > "$work/$SANDBOX_MARKER" || exit 1
ui_copy="$work/ui"
runner=""
poll_s=0.1
ready_polls=200
stop_polls=20
call_timeout_s=5
focused_alpha=255
inactive_alpha=255
stop_shell() {
    [[ -n "$runner" ]] || return 0
    local tick
    kill -TERM -- "-$runner" 2>/dev/null || true
    for ((tick = 0; tick < stop_polls; tick++)); do
        kill -0 -- "-$runner" 2>/dev/null || break
        sleep "$poll_s"
    done
    kill -0 -- "-$runner" 2>/dev/null && kill -KILL -- "-$runner" 2>/dev/null
    wait "$runner" 2>/dev/null || true
    if kill -0 -- "-$runner" 2>/dev/null; then
        printf 'FAIL material-startup owned group %s survived teardown\n' "$runner"
        return 1
    fi
    runner=""
}
cleanup() { stop_shell && sandbox_remove "$work"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
cp -a ui "$ui_copy" || exit 1
headless_commons "$work" "$work/commons" || exit 1
[[ -L "$ui_copy/boot/Commons" ]] || exit 1
ln -sfn "$work/commons" "$ui_copy/boot/Commons" || exit 1
# Sample source: function floor(c, need) { ... }; only its name changes, with a timed wrapper preserving its arguments and result.
python3 -I - "$ui_copy/js/Material.js" "$ui_copy/Glass.qml" <<'PY' || exit 1
from pathlib import Path
import sys
path = Path(sys.argv[1])
body = path.read_text()
needle = 'function floor(c, need) {'
assert body.count(needle) == 1
# Sample source: "function floor(c, need) {", the unchanged implementation's declaration.
body = body.replace(needle, 'function measuredFloor(c, need) {')
body += '''
function floor(c, need) {
    var started = Date.now()
    var result = measuredFloor(c, need)
    console.log("MATERIAL_FLOOR ms=" + (Date.now() - started))
    return result
}
'''
path.write_text(body)
glass = Path(sys.argv[2])
source = glass.read_text()
needle = 'function launch() {'
assert source.count(needle) == 1
# Sample source: "function launch() {", before the actual launch reads Omarchy's mode.
source = source.replace(needle, needle + '''
        console.log("MATERIAL_ENV hypr=" + JSON.stringify(Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE"))
            + " type=" + typeof Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE"))''')
glass.write_text(source)
PY
failed=0
# leg NAME MODE OVERRIDE EXPECT: OVERRIDE is absent, user or theme; EXPECT is the selected 8-bit alpha.
leg() {
    local name=$1 mode=$2 override=$3 expect=$4 probe="$work/$1" answer="" tick
    local theme_dir="$probe/home/.local/state/omarchy/current/theme"
    mkdir -p "$probe"/{home/.config/omarchy,config,state,data,cache,runtime,fixture} "$theme_dir" || return 1
    chmod 700 "$probe/runtime" || return 1
    printf 'a\n' > "$probe/fixture/a.txt" || return 1
    printf 'background = "#1e2030"\ndark_background = "#2c3045"\nforeground = "#d0d4e0"\naccent = "#7aa2f7"\n' > "$theme_dir/colors.toml" || return 1
    if [[ "$mode" != transparent ]]; then
        mkdir -p "$probe/state/omarchy/toggles/hypr" || return 1
        printf 'return\n' > "$probe/state/omarchy/toggles/hypr/window-$mode.lua" || return 1
    fi
    case "$override" in
        user) printf '[flea]\nbackground-alpha = 0.34\n' > "$probe/home/.config/omarchy/shell.toml" || return 1 ;;
        theme) printf '[flea]\nbackground-alpha = 0.34\n' > "$theme_dir/shell.toml" || return 1 ;;
    esac
    env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state '{"updates":{"autoCheck":false}}' >/dev/null || return 1
    setsid env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$probe/home" XDG_CONFIG_HOME="$probe/config" XDG_STATE_HOME="$probe/state" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_BIN="$bin" FLEA_UI="$ui_copy" FLEA_PATH="$probe/fixture" GVFS_DISABLE_FUSE=1 QSG_RHI_BACKEND=opengl \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
        dbus-run-session -- qs -p "$ui_copy/boot" > "$probe/qs.log" 2>&1 </dev/null &
    runner=$!
    for ((tick = 0; tick < ready_polls; tick++)); do
        answer=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
            XDG_RUNTIME_DIR="$probe/runtime" QT_QPA_PLATFORM=offscreen \
            timeout "$call_timeout_s" "$FLEA_IPC_HELPER" -p "$ui_copy/boot" call flea glass 2>/dev/null) && [[ "$answer" == '{'* ]] && break
        answer=""
        sleep "$poll_s"
    done
    stop_shell || return 1
    grep -aE 'MATERIAL_ENV|MATERIAL_FLOOR' "$probe/qs.log" || true
    if [[ -z "$answer" ]] || grep -aqE 'TypeError|ReferenceError|ERROR|Binding loop' "$probe/qs.log"; then
        printf 'FAIL material-startup %s did not reach a clean IPC seam\n' "$name"
        tail -15 "$probe/qs.log"
        return 1
    fi
    local calls elapsed actual sample
    # Sample log: "  INFO qml: MATERIAL_FLOOR ms=3", counted once per search.
    calls=$(grep -ac 'MATERIAL_FLOOR ms=' "$probe/qs.log" || true)
    # Sample log: "  INFO qml: MATERIAL_FLOOR ms=3", one elapsed whole-millisecond sample per call.
    elapsed=0
    while IFS= read -r sample; do
        elapsed=$((elapsed + sample))
    done < <(sed -n 's/.*MATERIAL_FLOOR ms=\([0-9]*\).*/\1/p' "$probe/qs.log")
    if [[ "$expect" == focus ]]; then
        # Sample IPC: {"focused":true,"alpha":0.984313725490196}, selected by the fixture window's actual focus state.
        expect=$(jq -er --argjson active "$focused_alpha" --argjson inactive "$inactive_alpha" \
            'if (.focused | type) != "boolean" then error("invalid focus") else if .focused then $active else $inactive end end' <<< "$answer") || return 1
    fi
    # Sample IPC: {"alpha":0.7529411764705882}, which rounds to the 8-bit alpha 192.
    actual=$(jq -er 'if (.alpha | type) == "number" and .alpha >= 0 and .alpha <= 1 then (.alpha * 255) | round else error("invalid alpha") end' <<< "$answer") || return 1
    printf 'MATERIAL_STARTUP %s calls=%s floorMs=%s alpha=%s expected=%s\n' "$name" "$calls" "$elapsed" "$actual" "$expect"
    [[ "$actual" == "$expect" ]] || return 1
    if [[ "$name" == glassy ]]; then [[ "$calls" -gt 0 ]]; else [[ "$calls" -eq 0 ]]; fi
}
printf 'material-startup: scope=frozen-qml-oracle entry=qs %s\n' "$(QT_QPA_PLATFORM=offscreen qml6 --version)"
leg opaque opaque absent 255 || failed=$((failed + 1))
leg transparent transparent absent focus || failed=$((failed + 1))
leg user-override glassy user 87 || failed=$((failed + 1))
leg theme-override glassy theme 87 || failed=$((failed + 1))
leg glassy glassy absent 192 || failed=$((failed + 1))
printf 'material-startup: failed=%s\n' "$failed"
[[ "$failed" -eq 0 ]]
