#!/bin/bash
# Proves an empty-desktop drop reaches a Bottom-layer receiver and prints exactly one LAYERDROP line on stdout, with diagnostics on stderr.
. "$(dirname "${BASH_SOURCE[0]}")/../../tools/flea-ipc"
set -u
out() { printf 'LAYERDROP %s\n' "$*"; }
refuse() {
    declare -F layerdrop_diagnostics >/dev/null && layerdrop_diagnostics
    # LAYERDROP FAIL <why> reports a setup, input or unobserved-drop failure and exits unsuccessfully.
    out "FAIL $*"
    exit 1
}
need() { command -v "$1" >/dev/null 2>&1 || refuse "missing $1"; }
need qs; need hyprctl; need ydotool; need omarchy-drive
[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ] || refuse "no Hyprland session"
flea_bin="${FLEA_BIN:-$(command -v flea || true)}"
[ -n "$flea_bin" ] || refuse "no flea binary (set FLEA_BIN)"
flea_ui="${FLEA_UI:-$(cd "$(dirname "$0")/../../ui" && pwd)}"
[ -f "$flea_ui/boot/shell.qml" ] || refuse "no Flea ui at $flea_ui (set FLEA_UI)"
repo="$(cd "$(dirname "$0")/../.." && pwd)"
[ -f "$repo/tests/lib/hypr-dispatch.sh" ] || refuse "no typed Hyprland helper at $repo/tests/lib/hypr-dispatch.sh"
. "$repo/tests/lib/hypr-dispatch.sh"
probe_py="$repo/tests/xwtab_free_point.py"
[ -f "$probe_py" ] || refuse "no free-point helper at $probe_py"
verdict_sh="$(dirname "$0")/layer-drop-verdict.sh"
[ -f "$verdict_sh" ] || refuse "no verdict helper at $verdict_sh"
# shellcheck disable=SC1090
. "$verdict_sh"

work=$(mktemp -d "${TMPDIR:-/tmp}/layer-drop.XXXXXXXX") || refuse "mktemp failed"
layerdrop_button_down=false
layerdrop_cleanup() {
    if [[ "$layerdrop_button_down" == true ]]; then
        ydotool click 0x80 >/dev/null 2>&1 || true
    fi
    kill "$qs_pid" "$flea_pid" $torn 2>/dev/null || true
    rm -rf "$work"
}
trap 'layerdrop_cleanup' EXIT
qs_pid=""; flea_pid=""; torn=""; addr=""; qid=""; drag_mark=0
centre=""; lifted_path=""; source_rect=""; sx=""; sy=""; dx=""; dy=""
layerdrop_panel_attempts=40 # Bound the wait for the fixture panel's layer surface to map.
layerdrop_panel_poll=0.25 # Poll the layer list between checks.
layerdrop_lookup_attempts=60 # Bound the wait for the Flea client and its qs instance to appear.
layerdrop_lookup_poll=0.5 # Poll the clients and qs instances between checks.
layerdrop_torn_attempts=20 # Bound the lookup of a torn-off window's qs instance.
layerdrop_settle_attempts=40 # Bound each wait for window state: focus, float, fixture listing, second tab and drop receipt.
layerdrop_settle_poll=0.1 # Poll the compositor and qs between state checks.
layerdrop_park_attempts=60 # Bound the wait for the probe window to reach its park rectangle.
layerdrop_park_x=40
layerdrop_park_y=40
layerdrop_park_w=900
layerdrop_park_h=500

# Print setup and this gesture's trace only when the probe fails.
layerdrop_diagnostics() {
    local lines
    printf 'LAYERDROP source pid=%s address=%s instance=%s expected=%s lifted=%s\n' "$flea_pid" "$addr" "$qid" "${srcdir:-}" "$lifted_path" >&2
    printf 'LAYERDROP rect=%s tabCentre=%s press=%s,%s release=%s,%s; cursorpos: ' "$source_rect" "$centre" "$sx" "$sy" "$dx" "$dy" >&2
    hyprctl cursorpos >&2 || true
    printf 'LAYERDROP active: ' >&2
    hyprctl activewindow -j >&2 || true
    if [ -n "$addr" ]; then printf 'LAYERDROP current source rect=%s\n' "$(layerdrop_rect || true)" >&2; fi
    if [ -n "$flea_pid" ]; then
        printf 'LAYERDROP stdout=%s stderr=%s\n' "$(readlink "/proc/$flea_pid/fd/1" || true)" "$(readlink "/proc/$flea_pid/fd/2" || true)" >&2
    fi
    printf 'LAYERDROP TABDRAG file=%s/flea.log after-line=%s\n' "$work" "$drag_mark" >&2
    lines=$(tail -n +"$((drag_mark + 1))" "$work/flea.log" 2>/dev/null | grep -a TABDRAG || true)
    printf '%s\n' "${lines:-(no TABDRAG lines since press mark)}" >&2
    tail -5 "$work/qs.log" >&2 2>/dev/null || true
}

log="$work/panel.log"
: > "$log"

# A minimal Bottom-layer panel offers the tab DropArea and appends PANEL-DROP to the file the check reads.
cat > "$work/panel.qml" <<EOF
import QtQuick
import Quickshell
import Quickshell.Wayland
ShellRoot {
    Variants {
        model: Quickshell.screens
        PanelWindow {
            required property var modelData
            screen: modelData
            color: "transparent"
            anchors { top: true; bottom: true; left: true; right: true }
            WlrLayershell.layer: WlrLayer.Bottom
            WlrLayershell.exclusiveZone: 0
            // The startup wait below matches this namespace exactly, never a qs prefix.
            WlrLayershell.namespace: "flea-layer-drop"
            DropArea {
                anchors.fill: parent
                keys: ["application/x-flea-tab"]
                onEntered: function (drag) {
                    Quickshell.execDetached(["sh", "-c", "printf 'PANEL-ENTER\\\\n' >> '$log'"])
                }
                onDropped: function (drop) {
                    Quickshell.execDetached(["sh", "-c", "printf 'PANEL-DROP\\\\n' >> '$log'"])
                }
            }
        }
    }
}
EOF

setsid qs -p "$work/panel.qml" >"$work/qs.log" 2>&1 &
qs_pid=$!
qs_up=""
for _ in $(seq 1 "$layerdrop_panel_attempts"); do
    if hyprctl layers -j 2>/dev/null | grep -Fq '"namespace": "flea-layer-drop"'; then qs_up=1; break; fi
    sleep "$layerdrop_panel_poll"
done
[ -n "$qs_up" ] || refuse "no Quickshell layer surface appeared"

# A Flea window with two tabs is the drag source: the strip is hidden for one.
srcdir="$work/src"
mkdir -p "$srcdir/sub" "$work/state"
export XDG_STATE_HOME="$work/state"
# The session runtime dir stays real, so the undo journal needs its own knob.
export FLEA_UNDO_DIR="$work/flea-undo"
"$flea_bin" --ui-state '{"newTab":"current","view":"list"}' >/dev/null || refuse "could not seed probe settings"
FLEA_TRACE_TABDRAG=1 FLEA_UI="$flea_ui" FLEA_BIN="$flea_bin" setsid nohup "$flea_bin" --gui "$srcdir" >"$work/flea.log" 2>&1 </dev/null &
flea_pid=$!
addr=""
for _ in $(seq 1 "$layerdrop_lookup_attempts"); do
    addr=$(hyprctl clients -j | python3 -c '
import json, sys
# Sample input: [{"address":"0xa","pid":101,"at":[40,40],"size":[900,500],"floating":true}].
hits = [c for c in json.load(sys.stdin) if str(c.get("pid")) == sys.argv[1]]
print(hits[0]["address"] if len(hits) == 1 else "")
' "$flea_pid") || true
    [ -n "$addr" ] && break
    sleep "$layerdrop_lookup_poll"
done
[ -n "$addr" ] || refuse "no Flea window came up"
# Read only the probe's address, including after every compositor operation.
layerdrop_rect() {
    hyprctl clients -j 2>/dev/null | python3 -c '
import json, sys
# Sample input: [{"address":"0xa","pid":101,"at":[40,40],"size":[900,500],"floating":true}].
hits = [c for c in json.load(sys.stdin) if c.get("address") == sys.argv[1] and str(c.get("pid")) == sys.argv[2]]
if len(hits) != 1:
    raise SystemExit(1)
c = hits[0]
print(c["at"][0], c["at"][1], c["size"][0], c["size"][1], str(bool(c.get("floating"))))
' "$addr" "$flea_pid"
}

layerdrop_focus() {
    local i active
    hypr_window_focus "$addr" || refuse "could not focus probe window"
    for i in $(seq 1 "$layerdrop_settle_attempts"); do
        # Sample input: {"address":"0xa","pid":101,"class":"com.thisisgm.flea"} from hyprctl activewindow -j.
        active=$(hyprctl activewindow -j 2>/dev/null | python3 -c 'import json,sys; print(json.load(sys.stdin).get("address", ""))' || true)
        [ "$active" = "$addr" ] && return 0
        sleep "$layerdrop_settle_poll"
    done
    refuse "probe window never took focus"
}

layerdrop_focus
source_rect=$(layerdrop_rect) || refuse "no initial probe geometry"
read -r wx wy ww wh floating <<< "$source_rect"
if [ "$floating" != True ]; then
    hypr_window_float "$addr" on || refuse "could not float probe window"
fi
floated=""
for _ in $(seq 1 "$layerdrop_settle_attempts"); do
    source_rect=$(layerdrop_rect || true)
    read -r wx wy ww wh floating <<< "$source_rect"
    if [ "${floating:-}" = True ]; then floated=1; break; fi
    sleep "$layerdrop_settle_poll"
done
[ -n "$floated" ] || refuse "probe window never floated"
hypr_window_resize_absolute "$addr" "$layerdrop_park_w" "$layerdrop_park_h" || refuse "could not resize probe window"
hypr_window_move_absolute "$addr" "$layerdrop_park_x" "$layerdrop_park_y" || refuse "could not park probe window"
parked=""
for _ in $(seq 1 "$layerdrop_park_attempts"); do
    source_rect=$(layerdrop_rect || true)
    if [ "$source_rect" = "$layerdrop_park_x $layerdrop_park_y $layerdrop_park_w $layerdrop_park_h True" ]; then parked=1; break; fi
    sleep "$layerdrop_settle_poll"
done
[ -n "$parked" ] || refuse "probe window never reached $layerdrop_park_x,$layerdrop_park_y at ${layerdrop_park_w}x${layerdrop_park_h}"
# The qs instance id for this pid, same lookup tests/ui.sh uses for its tabs.
qid=""
for _ in $(seq 1 "$layerdrop_lookup_attempts"); do
    qid=$(flea_ipc_list --all --json 2>/dev/null | python3 -c '
import json, sys
# Sample input: [{"id":"abc123","pid":101,"config_path":"/ui/boot/shell.qml"}].
hits = [x for x in json.load(sys.stdin) if x.get("config_path") == sys.argv[1] and x.get("pid") == int(sys.argv[2])]
print(hits[0]["id"] if len(hits) == 1 else "")
' "$flea_ui/boot/shell.qml" "$flea_pid") || true
    [ -n "$qid" ] && break
    sleep "$layerdrop_lookup_poll"
done
[ -n "$qid" ] || refuse "no qs instance for $flea_pid"
# The command-line fixture must have landed before t snapshots it into another tab.
ready=""
for _ in $(seq 1 "$layerdrop_settle_attempts"); do
    lifted_path=$(flea_ipc_call -i "$qid" call flea path 2>/dev/null || true)
    if [ "$lifted_path" = "$srcdir" ] && [ "$(flea_ipc_call -i "$qid" call flea listInFlight 2>/dev/null || true)" = false ]; then ready=1; break; fi
    sleep "$layerdrop_settle_poll"
done
[ -n "$ready" ] || refuse "source window did not settle on fixture $srcdir"
layerdrop_focus
omarchy-drive key --window "$addr" t >/dev/null 2>&1 || refuse "t did not reach probe window"
# A second tab must be current and settled on the fixture, never the operator's home.
two=""
for _ in $(seq 1 "$layerdrop_settle_attempts"); do
    lifted_path=$(flea_ipc_call -i "$qid" call flea path 2>/dev/null || true)
    if [ "$(flea_ipc_call -i "$qid" call flea tabCount 2>/dev/null || true)" = "2" ] \
        && [ "$(flea_ipc_call -i "$qid" call flea tabIndex 2>/dev/null || true)" = "1" ] \
        && [ "$lifted_path" = "$srcdir" ] \
        && [ "$(flea_ipc_call -i "$qid" call flea listInFlight 2>/dev/null || true)" = false ]; then two=1; break; fi
    sleep "$layerdrop_settle_poll"
done
[ -n "$two" ] || refuse "second tab did not settle on fixture $srcdir"
# Read both the painted tab centre and the real client geometry after the park.
centre=$(flea_ipc_call -i "$qid" call flea tabCentre 1 2>/dev/null || true)
[ -n "$centre" ] || refuse "second tab has no centre"
read -r cx cy <<< "$centre"
source_rect=$(layerdrop_rect) || refuse "no post-move geometry for $addr"
read -r wx wy ww wh floating <<< "$source_rect"
sx=$((wx + cx)); sy=$((wy + cy))
(( cx > 0 && cy > 0 && cx < ww && cy < wh )) || refuse "tab centre is outside the parked probe window"
# The release uses the same free-point scan the case uses, after the park above.
mon_json=$(hyprctl monitors -j 2>/dev/null || true)
[ -n "$mon_json" ] || refuse "no monitors to scan"
read -r mx my mw mh mon_name < <(printf '%s' "$mon_json" | python3 -c '
import json, sys
# Sample input: [{"name":"DP-2","x":0,"y":0,"width":2560,"height":1440,"focused":true,"activeWorkspace":{"id":1}}].
ms = json.load(sys.stdin)
m = [x for x in ms if x.get("focused")] or ms
print(m[0]["x"], m[0]["y"], m[0]["width"], m[0]["height"], m[0].get("name", ""))
' || true)
[ -n "${mon_name:-}" ] || refuse "no focused monitor to scan"
hyprctl clients -j 2>/dev/null > "$work/clients.json" || refuse "no clients to scan"
# The own panel receives the drop, so it is filtered out of the cover like the case filters nothing yet mapped.
hyprctl layers -j 2>/dev/null | python3 -c '
import json, sys
# Sample input: {"DP-2":{"levels":{"1":[{"namespace":"flea-layer-drop","x":0,"y":0,"w":2560,"h":1440,"pid":200}]}}}.
d = json.load(sys.stdin)
for entry in (d.values() if isinstance(d, dict) else []):
    lv = entry.get("levels") if isinstance(entry, dict) else None
    if not isinstance(lv, dict):
        continue
    for k in list(lv.keys()):
        v = lv[k]
        if isinstance(v, list):
            lv[k] = [n for n in v if not (isinstance(n, dict) and n.get("namespace") == "flea-layer-drop")]
print(json.dumps(d))
' > "$work/layers.json" || refuse "no layers to scan"
printf '%s' "$mon_json" > "$work/monitors.json"
point=$(python3 -B "$probe_py" "$mx" "$my" "$mw" "$mh" "$mon_name" "$work/clients.json" "$work/layers.json" "$work/monitors.json" || true)
[ -n "$point" ] || refuse "no empty desktop point on the focused monitor"
read -r dx dy <<< "$point"
# The Bottom panel must still be mapped at release, or the drop has no receiver.
hyprctl layers -j 2>/dev/null | grep -Fq '"namespace": "flea-layer-drop"' || refuse "Bottom panel went away before the drop"
move_to() {
    local tx="$1" ty="$2" i cx cy cursor
    local attempts=16 tolerance=4 divisor=2 poll=0.05
    for i in $(seq 1 "$attempts"); do
        cursor=$(hyprctl cursorpos) || refuse "cursorpos read failed"
        # Sample input: hyprctl cursorpos returns "40, 80"; negative coordinates are valid.
        [[ "$cursor" =~ ^[[:space:]]*(-?[0-9]+),[[:space:]]+(-?[0-9]+)[[:space:]]*$ ]] || refuse "invalid cursor position: $cursor"
        cx=${BASH_REMATCH[1]}
        cy=${BASH_REMATCH[2]}
        if (( tx - cx <= tolerance && tx - cx >= -tolerance && ty - cy <= tolerance && ty - cy >= -tolerance )); then
            return 0
        fi
        ydotool mousemove -x "$(((tx - cx) / divisor))" -y "$(((ty - cy) / divisor))" >/dev/null 2>&1 || refuse "pointer motion failed"
        sleep "$poll"
    done
    refuse "pointer did not reach $tx,$ty"
}
layerdrop_outside_x=200
layerdrop_outside_y=60
layerdrop_target_nudge=6
layerdrop_drag_attempts=40
layerdrop_drag_poll=0.1
# Require platform start, mapped catcher and either receiver's hover while the pointer remains held.
layerdrop_wait_drag() {
    local stage="$1" attempt lines
    for attempt in $(seq 1 "$layerdrop_drag_attempts"); do
        # Sample input: TABDRAG catcher-enter pid=1154634 global=1280,720
        lines=$(tail -n +"$((drag_mark + 1))" "$work/flea.log" | grep -a "TABDRAG .* pid=$flea_pid " || true)
        if grep -aq 'TABDRAG drag-finished' <<< "$lines"; then
            refuse "drag ended before $stage while pointer held"
        fi
        case "$stage" in
            start)
                if grep -aq "TABDRAG drag-start .* path=$srcdir mime=" <<< "$lines"; then
                    return 0
                fi
                ;;
            mapped)
                if hyprctl layers -j 2>/dev/null | grep -Fq '"namespace": "flea-tab-tearoff"'; then
                    return 0
                fi
                ;;
            receiver)
                if grep -aq 'TABDRAG catcher-enter' <<< "$lines" || grep -aq '^PANEL-ENTER$' "$log"; then
                    return 0
                fi
                ;;
        esac
        sleep "$layerdrop_drag_poll"
    done
    refuse "no $stage receipt from platform tab drag on fixture $srcdir"
}

# Flea windows before the drop: its own Bottom catcher may take the drop and tear off instead.
before_flea=$(pgrep -x qs | while read -r pid; do tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq "$flea_ui" && printf '%s ' "$pid"; done)
[ "$(layerdrop_rect || true)" = "$source_rect" ] || refuse "source geometry changed before the press"
move_to "$sx" "$sy"
drag_mark=$(wc -l < "$work/flea.log")
layerdrop_button_down=true
ydotool click 0x40 >/dev/null 2>&1 || refuse "pointer press failed"
move_to "$((wx + layerdrop_outside_x))" "$((wy + wh + layerdrop_outside_y))"
layerdrop_wait_drag start
layerdrop_wait_drag mapped
move_to "$dx" "$dy"
move_to "$((dx + layerdrop_target_nudge))" "$dy"
move_to "$dx" "$dy"
layerdrop_wait_drag receiver
ydotool click 0x80 >/dev/null 2>&1 || refuse "pointer release failed"
layerdrop_button_down=false
# Wait for an observed panel receipt or a new Flea process before evaluating either route.
for _ in $(seq 1 "$layerdrop_settle_attempts"); do
    layerdrop_panel_hit "$log" && break
    after_flea=$(pgrep -x qs | while read -r pid; do tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq "$flea_ui" && printf '%s ' "$pid"; done)
    torn=$(layerdrop_torn_pids "$before_flea" "$after_flea")
    [ -n "$torn" ] && break
    sleep "$layerdrop_settle_poll"
done
# The qs instance id for a torn-off pid, same lookup the case uses for its tabs.
layerdrop_qsid() {
    local pid="$1" i tid
    for i in $(seq 1 "$layerdrop_torn_attempts"); do
        tid=$(flea_ipc_list --all --json 2>/dev/null | python3 -c '
import json, sys
# Sample input: [{"id":"def456","pid":202,"config_path":"/ui/boot/shell.qml"}], read for the torn-off window.
hits = [x for x in json.load(sys.stdin) if x.get("config_path") == sys.argv[1] and x.get("pid") == int(sys.argv[2])]
print(hits[0]["id"] if len(hits) == 1 else "")
' "$flea_ui/boot/shell.qml" "$pid") || true
        if [ -n "$tid" ]; then printf '%s' "$tid"; return 0; fi
        sleep "$layerdrop_lookup_poll"
    done
    return 1
}
if layerdrop_panel_hit "$log"; then
    # LAYERDROP PASS proves the fixture Bottom panel recorded PANEL-DROP.
    out "PASS"
    exit 0
fi
# The drop reached Flea's own Bottom catcher when a new owned window reads as the lifted folder through qs ipc.
after_flea=$(pgrep -x qs | while read -r pid; do tr '\0' ' ' 2>/dev/null < "/proc/$pid/cmdline" | grep -Fq "$flea_ui" && printf '%s ' "$pid"; done)
torn=$(layerdrop_torn_pids "$before_flea" "$after_flea")
paths_tsv=""
if layerdrop_catcher_hit "$torn" "$lifted_path"; then
    # LAYERDROP CATCHER-TEAROFF proves a new Flea window opened on the lifted folder through the catcher.
    out "CATCHER-TEAROFF"
    exit 0
fi
printf 'flea windows before: %s after: %s\n' "$before_flea" "$after_flea" >&2
printf 'lifted folder: %s torn: %s paths: %s\n' "$lifted_path" "$torn" "$(printf '%s' "$paths_tsv" | tr '\n' ';')" >&2
refuse "no PANEL-DROP in $log and no torn-off window"
