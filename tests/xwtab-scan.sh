#!/bin/bash
# Proves the shipped xwtab free-desktop scan skips level 0 and other workspaces while levels 1 to 3 on the focused monitor block.
set -u
cd "$(dirname "$0")/.." || exit 1
repo=$PWD
pass=0
fail=0
ok() { printf 'ok %s\n' "$*"; pass=$((pass+1)); }
bad() { printf 'FAIL %s\n' "$*" >&2; fail=$((fail+1)); }
scratch=$(mktemp -d) || exit 1
case $scratch in /*/*) ;; *) echo "FAIL mktemp gave $scratch" >&2; exit 1 ;; esac
trap 'rm -rf "$scratch"' EXIT
# One focused monitor, DP-2 at 0 0 2560 1440, active workspace 1, no special.
cat > "$scratch/monitors.json" <<'EOF'
[{"name":"DP-2","x":0,"y":0,"width":2560,"height":1440,"focused":true,"activeWorkspace":{"id":1,"name":"1"},"specialWorkspace":{"id":0,"name":""}}]
EOF
# The minipc fixture has a fullscreen level 0 background, fullscreen level 1 catcher and level 2 bar strip.
cat > "$scratch/layers.json" <<'EOF'
{"DP-2":{"levels":{"0":[{"address":"0x1","x":0,"y":0,"w":2560,"h":1440,"namespace":"omarchy-background","pid":100}],"1":[{"address":"0x2","x":0,"y":0,"w":2560,"h":1440,"namespace":"flea-tab-tearoff","pid":200}],"2":[{"address":"0x3","x":0,"y":0,"w":2560,"h":30,"namespace":"omarchy-bar","pid":300}],"3":[]}}}
EOF
# Active bottom cover and a fullscreen workspace 2 client, plus hidden and unmapped rows that never cover.
cat > "$scratch/clients.json" <<'EOF'
[{"address":"0xa","mapped":true,"hidden":false,"at":[0,1000],"size":[2560,440],"workspace":{"id":1,"name":"1"},"floating":true,"monitor":1,"class":"x","title":"t","pid":1000},{"address":"0xb","mapped":true,"hidden":false,"at":[0,0],"size":[2560,1440],"workspace":{"id":2,"name":"2"},"floating":false,"monitor":1,"class":"x","title":"t","pid":1001},{"address":"0xc","mapped":true,"hidden":true,"at":[0,0],"size":[2560,1440],"workspace":{"id":1,"name":"1"},"floating":false,"monitor":1,"class":"x","title":"t","pid":1002},{"address":"0xd","mapped":false,"hidden":false,"at":[0,0],"size":[2560,1440],"workspace":{"id":1,"name":"1"},"floating":false,"monitor":1,"class":"x","title":"t","pid":1003}]
EOF
got=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients.json" "$scratch/layers.json" "$scratch/monitors.json" || true)
# Bottom cover ends at y 1000, so the bottom-up 24 px grid first frees at 976.
if [ "$got" = "8 976" ]; then
ok "level 0 plus other workspace skipped, active bottom kept: $got"
else
bad "level 0 plus other workspace skipped, want '8 976', got '$got'"
fi
# Old scan at fb5d999e counted every layer and every client, so these three files leave it no point and the caller fails.
old=$(python3 -B -c '
import json,sys
mx,my,mw,mh=[int(v) for v in sys.argv[1:5]]
clients=json.load(open(sys.argv[5]))
layers=json.load(open(sys.argv[6]))
rects=[[c["at"][0],c["at"][1],c["size"][0],c["size"][1]] for c in clients]
def harvest(node):
    global rects
    if isinstance(node,dict):
        if all(k in node for k in ("x","y","w","h")) and "namespace" in node:
            ns=str(node["namespace"])
            full=node["x"]==mx and node["y"]==my and node["w"]==mw and node["h"]==mh
            if not (ns.startswith("qs") and full):
                rects.append([node["x"],node["y"],node["w"],node["h"]])
        for v in node.values():
            harvest(v)
    elif isinstance(node,list):
        for v in node:
            harvest(v)
harvest(layers)
def covered(px,py):
    return any(rx<=px<rx+rw and ry<=py<ry+rh for rx,ry,rw,rh in rects)
for py in range(my+mh-8,my-1,-24):
    for px in range(mx+8,mx+mw-8,24):
        if not covered(px,py):
            print(px,py)
            raise SystemExit(0)
print("")
' 0 0 2560 1440 "$scratch/clients.json" "$scratch/layers.json" || true)
if [ -z "$old" ]; then
ok "old scan at fb5d999e finds no point on the same files"
else
bad "old scan at fb5d999e should find no point, got '$old'"
fi
# Without the level 0 background the other-workspace fullscreen alone still blocks the old scan, while the new one keeps 8 976.
cat > "$scratch/layers-nobg.json" <<'EOF'
{"DP-2":{"levels":{"0":[],"1":[],"2":[{"address":"0x3","x":0,"y":0,"w":2560,"h":30,"namespace":"omarchy-bar","pid":300}],"3":[]}}}
EOF
got2=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients.json" "$scratch/layers-nobg.json" "$scratch/monitors.json" || true)
if [ "$got2" = "8 976" ]; then
ok "other workspace alone is skipped: $got2"
else
bad "other workspace alone is skipped, want '8 976', got '$got2'"
fi
old2=$(python3 -B -c '
import json,sys
mx,my,mw,mh=[int(v) for v in sys.argv[1:5]]
clients=json.load(open(sys.argv[5]))
layers=json.load(open(sys.argv[6]))
rects=[[c["at"][0],c["at"][1],c["size"][0],c["size"][1]] for c in clients]
def harvest(node):
    global rects
    if isinstance(node,dict):
        if all(k in node for k in ("x","y","w","h")) and "namespace" in node:
            ns=str(node["namespace"])
            full=node["x"]==mx and node["y"]==my and node["w"]==mw and node["h"]==mh
            if not (ns.startswith("qs") and full):
                rects.append([node["x"],node["y"],node["w"],node["h"]])
        for v in node.values():
            harvest(v)
    elif isinstance(node,list):
        for v in node:
            harvest(v)
harvest(layers)
def covered(px,py):
    return any(rx<=px<rx+rw and ry<=py<ry+rh for rx,ry,rw,rh in rects)
for py in range(my+mh-8,my-1,-24):
    for px in range(mx+8,mx+mw-8,24):
        if not covered(px,py):
            print(px,py)
            raise SystemExit(0)
print("")
' 0 0 2560 1440 "$scratch/clients.json" "$scratch/layers-nobg.json" || true)
if [ -z "$old2" ]; then
ok "old scan blocked by the other workspace alone"
else
bad "old scan should be blocked by the other workspace alone, got '$old2'"
fi
# A layer on another monitor never covers this one, even fullscreen on level 2.
cat > "$scratch/layers-foreign.json" <<'EOF'
{"DP-2":{"levels":{"0":[],"1":[],"2":[{"address":"0x3","x":0,"y":0,"w":2560,"h":30,"namespace":"omarchy-bar","pid":300}],"3":[]}},"DP-1":{"levels":{"0":[],"1":[],"2":[{"address":"0x9","x":0,"y":0,"w":2560,"h":1440,"namespace":"other-bar","pid":900}],"3":[]}}}
EOF
cat > "$scratch/clients-empty.json" <<'EOF'
[]
EOF
got3=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients-empty.json" "$scratch/layers-foreign.json" "$scratch/monitors.json" || true)
if [ "$got3" = "8 1432" ]; then
ok "foreign monitor layer is skipped: $got3"
else
bad "foreign monitor layer is skipped, want '8 1432', got '$got3'"
fi
# Interactive levels block; a bottom cover leaving only the bar must report empty.
cat > "$scratch/clients-fullbelow.json" <<'EOF'
[{"address":"0xe","mapped":true,"hidden":false,"at":[0,30],"size":[2560,1410],"workspace":{"id":1,"name":"1"},"floating":false,"monitor":1,"class":"x","title":"t","pid":1004}]
EOF
got4=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients-empty.json" "$scratch/layers.json" "$scratch/monitors.json" || true)
# No client, only bar plus ignored background and qs, so bottom stays free.
if [ "$got4" = "8 1432" ]; then
ok "bar alone leaves the bottom free: $got4"
else
bad "bar alone leaves the bottom free, want '8 1432', got '$got4'"
fi
got5=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients-fullbelow.json" "$scratch/layers-nobg.json" "$scratch/monitors.json" || true)
if [ -z "$got5" ]; then
ok "bar plus a full-below cover reports empty"
else
bad "bar plus a full-below cover should report empty, got '$got5'"
fi
# An open special workspace covers like the active one, a closed one is ignored.
cat > "$scratch/monitors-special.json" <<'EOF'
[{"name":"DP-2","x":0,"y":0,"width":2560,"height":1440,"focused":true,"activeWorkspace":{"id":1,"name":"1"},"specialWorkspace":{"id":99,"name":"special"}}]
EOF
cat > "$scratch/clients-special.json" <<'EOF'
[{"address":"0xf","mapped":true,"hidden":false,"at":[0,1000],"size":[2560,440],"workspace":{"id":99,"name":"special"},"floating":false,"monitor":1,"class":"x","title":"t","pid":1005}]
EOF
got6=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients-special.json" "$scratch/layers-nobg.json" "$scratch/monitors-special.json" || true)
if [ "$got6" = "8 976" ]; then
ok "open special workspace covers: $got6"
else
bad "open special workspace covers, want '8 976', got '$got6'"
fi
got7=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients-special.json" "$scratch/layers-nobg.json" "$scratch/monitors.json" || true)
if [ "$got7" = "8 1432" ]; then
ok "closed special workspace is skipped: $got7"
else
bad "closed special workspace is skipped, want '8 1432', got '$got7'"
fi
# The tear-off count compares normalised sets, so a leading space, a doubled space and a duplicate collapse to one sorted unique set.
. "$repo/tests/xwtab-norm.sh" || { bad "cannot source xwtab-norm.sh"; }
norm=$(xwtab_norm_set " 160643 159605 160229 159605 " || true)
if [ "$norm" = "159605 160229 160643" ]; then
ok "leading space plus duplicate normalises: $norm"
else
bad "leading space plus duplicate normalises, want '159605 160229 160643', got '$norm'"
fi
norm_empty=$(xwtab_norm_set "" || true)
if [ -z "$norm_empty" ]; then
ok "empty set stays empty"
else
bad "empty set stays empty, got '$norm_empty'"
fi
# The probe verdict against the native lines that failed it: one torn pid (before "169083 ", after "169083 169306 ") reading as the lifted folder is the catcher drop.
. "$repo/tests/probes/layer-drop-verdict.sh" || { bad "cannot source layer-drop-verdict.sh"; }
flea_ipc_init qs
torn_case=$(layerdrop_torn_pids "169083 " "169083 169306 " || true)
if [ "$torn_case" = "169306" ]; then
ok "native before/after leaves one torn pid: $torn_case"
else
bad "native before/after leaves one torn pid, want '169306', got '$torn_case'"
fi
layerdrop_path_attempts=2
layerdrop_path_poll=0
flea_pid=169083
layerdrop_qsid() {
    case "$1" in
        169083) printf 'source-id\n' ;;
        169306) printf 'torn-id\n' ;;
        *) return 1 ;;
    esac
}
qs() {
    [[ "$1" == ipc && "$2" == -i && "$4 $5 $6" == 'call flea path' ]] || return 1
    case "$3" in
        source-id) printf '/fake/layer-drop/src\n' ;;
        torn-id) printf '%s\n' "$torn_path" ;;
        *) return 1 ;;
    esac
}
torn_path=/fake/layer-drop/src
if layerdrop_catcher_hit "$torn_case" "/fake/layer-drop/src"; then
ok "a torn window on the lifted folder is the drop reaching the catcher"
else
bad "a torn window on the lifted folder should count as the drop reaching the catcher"
fi
torn_path=/elsewhere
if layerdrop_catcher_hit "$torn_case" "/fake/layer-drop/src"; then
bad "a torn window on another folder must not count as the drop reaching the catcher"
else
ok "a torn window on another folder does not count"
fi
if layerdrop_catcher_hit "" "/fake/layer-drop/src"; then
bad "no torn window must not count as the drop reaching the catcher"
else
ok "no torn window does not count"
fi
if layerdrop_catcher_hit 169307 "/fake/layer-drop/src"; then
bad "a torn pid without a qs instance must not count as the catcher"
else
ok "a torn pid without a qs instance does not count"
fi
if layerdrop_catcher_hit "$torn_case" ""; then
bad "an empty lifted folder must not count as the catcher"
else
ok "an empty lifted folder does not count"
fi
unset -f qs layerdrop_qsid
printf 'PANEL-DROP\n' > "$scratch/panel-hit.log"
if layerdrop_panel_hit "$scratch/panel-hit.log"; then
ok "the probe panel route still passes on its own log line"
else
bad "the probe panel route should still pass on its own log line"
fi
: > "$scratch/panel-miss.log"
if layerdrop_panel_hit "$scratch/panel-miss.log"; then
bad "an empty panel log must not count as the panel taking the drop"
else
ok "an empty panel log does not count"
fi
# Hyprland selectors built from an address need the address: prefix, because a bare address resolves nothing while the dispatcher answers ok; the typed helper's fixture file holds bare selectors on purpose.
bare=$(grep -rnE 'window[[:space:]]*=[[:space:]]*\\?"(\$|0x|\{)' "$repo/tests" --exclude=xwtab-scan.sh --exclude=hypr-dispatch.json || true)
if [ -n "$bare" ]; then
bad "bare window selector without address: prefix: $bare"
else
ok "every window selector carries the address: prefix"
fi
# Exercise the live trace helpers with launch logs, including stale events before both marks.
flea_log="$scratch/flea.log"
run_root="$scratch"
printf 'qml: TABDRAG drag-finished pid=101 old=true\n' > "$flea_log"
printf 'qml: TABDRAG enter-window pid=202 old=true\n' > "$run_root/flea-second.log"
eval "$(sed -n '/^xwtab_logs=/,/^# The addr and rect/p' "$repo/tests/ui.sh")"
xwtab_source=101; xwtab_target=202; xwtab_gesture="test press"
xwtab_mark_logs
printf 'qml: TABDRAG drag-start pid=101 index=1\n' >> "$flea_log"
printf 'qml: TABDRAG enter-window pid=202 ok=true\n' >> "$run_root/flea-second.log"
expected=$(printf 'qml: TABDRAG drag-start pid=101 index=1\nqml: TABDRAG enter-window pid=202 ok=true')
if [ "$(xwtab_trace_lines)" = "$expected" ]; then
ok "both live launch logs are read after their own pre-press marks"
else
bad "trace reader mixed in stale events or missed a launch log"
fi
if (fail() { exit 1; }; xwtab_wait_start; xwtab_wait_enter 202 require); then
ok "source start and target enter accept the marked launch traces"
else
bad "marked source start and target enter should satisfy the waits"
fi
if (hyprctl() { printf '281, 106\n'; }; xwtab_rect_of() { printf '0xa 165 65 1000 720 True\n'; }; readlink() { printf 'test launch log\n'; }; xwtab_dump_trace) > "$scratch/dump.out" 2> "$scratch/dump.err" \
    && [ ! -s "$scratch/dump.out" ] && grep -Fq 'TABDRAG drag-start pid=101' "$scratch/dump.err" \
    && grep -Fq 'TABDRAG enter-window pid=202' "$scratch/dump.err" && ! grep -Fq old=true "$scratch/dump.err"; then
ok "failure dump prints both marked traces to stderr without hiding them"
else
bad "failure dump hid output, printed stale trace or wrote to stdout"
fi
printf 'qml: TABDRAG drag-finished pid=101 action=0\n' >> "$flea_log"
if (fail() { exit 1; }; xwtab_wait_enter 202 require); then
bad "a source finishing before release must fail even with a target enter"
else
ok "a source finishing before release is refused"
fi
# Run the real gesture helper in catcher mode; the own and refused modes stay pinned by tests/xwtab-safety.py.
: > "$scratch/catcher-drag.out"
(
    fail() { exit 1; }
    xwdrag_glide() { printf 'glide %s %s\n' "$1" "$2"; }
    xwdrag_geometry() { printf '165 65 1000 720\n'; }
    xwtab_mark_logs() { printf 'mark\n'; }
    xwtab_wait_start() { printf 'start\n'; }
    xwtab_wait_catcher() { printf 'catcher\n'; }
    xwtab_wait_enter() { printf 'enter %s %s\n' "$1" "$2"; }
    ydotool() { printf 'pointer %s %s\n' "$1" "$2" >> "$scratch/catcher-drag.out"; }
    xwtab_drag_to_window 501 106 281 106 101 101 catcher
) >> "$scratch/catcher-drag.out"
expected=$(printf 'glide 501 106\nmark\npointer click 0x40\nglide 365 845\nstart\nglide 281 106\nglide 287 106\nglide 281 106\ncatcher\npointer click 0x80')
if [ "$(cat "$scratch/catcher-drag.out")" = "$expected" ]; then
ok "catcher drag starts outside before post-start motion and catcher landing"
else
bad "catcher drag did not wait for start and catcher before landing and releasing"
fi
# A failure after pressing releases through the same case cleanup, at every waiting stage.
for stage in start target enter; do
    log="$scratch/release-$stage.out"
    (
        fail() { exit 1; }
        xwtab_restore_place() { :; }
        trap 'xwtab_cleanup' EXIT
        xwdrag_geometry() { printf '165 65 1000 720\n'; }
        xwtab_mark_logs() { :; }
        xwdrag_glide() { [[ "$stage" != target || "$1" != 1200 ]] || fail "target motion failed"; }
        xwtab_wait_start() { [[ "$stage" != start ]] || fail "start failed"; }
        xwtab_wait_enter() { [[ "$stage" != enter ]] || fail "enter failed"; }
        ydotool() { printf '%s %s\n' "$1" "$2" >> "$log"; }
        xwtab_drag_to_window 501 106 1200 400 101 202 require
    )
    result=$?
    if [ "$result" != 0 ] && [ "$(cat "$log")" = "$(printf 'click 0x40\nclick 0x80')" ]; then
        ok "$stage failure releases the held button exactly once"
    else
        bad "$stage failure leaked the held button or did not fail"
    fi
done
# The move leg also starts outside the source before the target receives motion.
: > "$scratch/move-order.out"
(
    fail() { exit 1; }
    xwdrag_geometry() { printf '165 65 1000 720\n'; }
    xwtab_mark_logs() { printf 'mark\n'; }
    xwdrag_glide() { printf 'glide %s %s\n' "$1" "$2"; }
    xwtab_wait_start() { printf 'start\n'; }
    xwtab_wait_enter() { printf 'enter %s %s\n' "$1" "$2"; }
    ydotool() { printf 'pointer %s %s\n' "$1" "$2" >> "$scratch/move-order.out"; }
    xwtab_drag_to_window 501 106 1200 400 101 202 require
) >> "$scratch/move-order.out"
expected=$(printf 'glide 501 106\nmark\npointer click 0x40\nglide 365 845\nstart\nglide 1200 400\nglide 1206 400\nglide 1200 400\nenter 202 require\npointer click 0x80')
if [ "$(cat "$scratch/move-order.out")" = "$expected" ]; then
    ok "cross-window target receives three motions after the platform start"
else
    bad "cross-window target motion raced platform start"
fi
# A reused pid is not enough: the layer probe must read back its exact client address.
(
    addr=0xa; flea_pid=101
    hyprctl() { printf '%s\n' '[{"address":"0xb","pid":101,"at":[900,600],"size":[500,300],"floating":false},{"address":"0xa","pid":101,"at":[40,40],"size":[900,500],"floating":true}]'; }
    eval "$(sed -n '/^layerdrop_rect()/,/^layerdrop_focus()/p' "$repo/tests/probes/layer-drop-bottom.sh" | sed '$d')"
    layerdrop_rect
) > "$scratch/probe-rect.out"
if [ "$(cat "$scratch/probe-rect.out")" = "40 40 900 500 True" ]; then
ok "layer probe geometry belongs to its address and pid after the move"
else
bad "layer probe geometry selected another client"
fi
# Malformed snapshots must never produce automation coordinates.
for kind in clients-object layers-list levels-object level-object monitors-empty client-missing client-text client-mapped-missing client-mapped-text client-mapped-number client-mapped-null layer-missing layer-text monitor-missing monitor-text; do
    python3 -B - "$scratch" "$kind" <<'PYFIX'
import json, pathlib, sys
root = pathlib.Path(sys.argv[1])
kind = sys.argv[2]
clients = json.loads((root / "clients.json").read_text())
layers = json.loads((root / "layers.json").read_text())
monitors = json.loads((root / "monitors.json").read_text())
if kind == "clients-object": clients = {}
if kind == "layers-list": layers = []
if kind == "levels-object": layers = {"DP-2": {"levels": []}}
if kind == "level-object": layers["DP-2"]["levels"]["2"] = {}
if kind == "monitors-empty": monitors = []
if kind == "client-missing": del clients[0]["size"]
if kind == "client-text": clients[0]["at"][0] = "zero"
if kind == "client-mapped-missing": del clients[0]["mapped"]
if kind == "client-mapped-text": clients[0]["mapped"] = "true"
if kind == "client-mapped-number": clients[0]["mapped"] = 1
if kind == "client-mapped-null": clients[0]["mapped"] = None
if kind == "layer-missing": del layers["DP-2"]["levels"]["2"][0]["w"]
if kind == "layer-text": layers["DP-2"]["levels"]["2"][0]["w"] = "wide"
if kind == "monitor-missing": del monitors[0]["width"]
if kind == "monitor-text": monitors[0]["width"] = "wide"
for name, value in (("bad-clients", clients), ("bad-layers", layers), ("bad-monitors", monitors)):
    (root / (name + ".json")).write_text(json.dumps(value))
PYFIX
    python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/bad-clients.json" "$scratch/bad-layers.json" "$scratch/bad-monitors.json" > "$scratch/bad.out" 2> "$scratch/bad.err"
    status=$?
    if [[ "$status" == 2 && ! -s "$scratch/bad.out" && -s "$scratch/bad.err" ]]; then
        ok "$kind snapshot refused without a coordinate"
    else
        bad "$kind snapshot must exit 2 without coordinate (status=$status output=$(cat "$scratch/bad.out"))"
    fi
done
# A monitor the snapshots do not carry answers no point, so another monitor's panels never stand in for it.
cat > "$scratch/layers-other.json" <<'EOF'
{"DP-1":{"levels":{"0":[],"1":[],"2":[],"3":[]}}}
EOF
printf '{}\n' > "$scratch/layers-empty.json"
for kind in monitor-unknown layers-without-monitor layers-empty; do
    mon=DP-2
    layers_file="$scratch/layers.json"
    [ "$kind" = monitor-unknown ] && mon=DP-9
    [ "$kind" = layers-without-monitor ] && layers_file="$scratch/layers-other.json"
    [ "$kind" = layers-empty ] && layers_file="$scratch/layers-empty.json"
    python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 "$mon" "$scratch/clients-empty.json" "$layers_file" "$scratch/monitors.json" > "$scratch/unknown.out" 2> "$scratch/unknown.err"
    status=$?
    if [[ "$status" == 2 && ! -s "$scratch/unknown.out" ]] && grep -Fq "$mon" "$scratch/unknown.err"; then
        ok "$kind refused by monitor name without a coordinate"
    else
        bad "$kind must exit 2 naming $mon without a coordinate (status=$status output=$(cat "$scratch/unknown.out") error=$(cat "$scratch/unknown.err"))"
    fi
done
# Every interactive layer level blocks except the exact Bottom catcher.
for layer in '1 desktop-widget' '3 overlay-widget' '3 qs-launcher' '1 qs-launcher'; do
    read -r level namespace <<< "$layer"
    printf '{"DP-2":{"levels":{"%s":[{"x":0,"y":0,"w":2560,"h":1440,"namespace":"%s"}]}}}\n' "$level" "$namespace" > "$scratch/blocker.json"
    blocked=$(python3 -B "$repo/tests/xwtab_free_point.py" 0 0 2560 1440 DP-2 "$scratch/clients-empty.json" "$scratch/blocker.json" "$scratch/monitors.json")
    status=$?
    if [[ "$status" != 0 ]]; then
        bad "level $level $namespace scan failed (status=$status)"
    elif [[ -z "$blocked" ]]; then
        ok "level $level $namespace blocks desktop"
    else
        bad "level $level $namespace must block desktop, got $blocked"
    fi
done
# Keep the live shell helpers under the same deterministic regression gate.
if python3 -B - "$repo" <<'PYSHAPES'
import importlib.util
import pathlib
import sys

sys.dont_write_bytecode = True
path = pathlib.Path(sys.argv[1]) / 'tests/xwtab_free_point.py'
spec = importlib.util.spec_from_file_location('free_point', path)
scan = importlib.util.module_from_spec(spec)
spec.loader.exec_module(scan)
checks = 0


def check(name, condition):
    global checks
    if not condition:
        raise AssertionError(name)
    checks += 1
    print('ok ' + name)


monitor = {'name': 'DP-2', 'x': 0, 'y': 0, 'width': 100, 'height': 100}
scan.validate_snapshots([], {}, [monitor])
try:
    scan.layer_rects({}, 'DP-2', 0, 0, 100, 100)
except ValueError as error:
    check('layers omitting the selected monitor are refused by its name', 'DP-2' in str(error))
else:
    check('layers omitting the selected monitor are refused by its name', False)
try:
    scan.monitor_entry([monitor], 'DP-9')
except ValueError as error:
    check('a monitor missing from the snapshot is refused by its name', 'DP-9' in str(error))
else:
    check('a monitor missing from the snapshot is refused by its name', False)
for shape in ('object', 'integer'):
    entry = dict(monitor, activeWorkspace={'id': 1} if shape == 'object' else 1,
                 specialWorkspace={'id': 99} if shape == 'object' else 99)
    client = {'mapped': True, 'at': [0, 0], 'size': [100, 100],
              'workspace': {'id': 99} if shape == 'object' else 99}
    scan.validate_snapshots([client], {}, [entry])
    valid = scan.workspace_ids(entry)
    check(shape + ' active and special workspaces stay supported', valid == {1, 99})
    check(shape + ' client workspace stays supported', scan.client_rects([client], valid) == [[0, 0, 100, 100]])
# Omarchy 4.5's bar surface spans the screen and takes input only on its reserved strip.
bar = {'1': [], '2': [{'namespace': 'omarchy-bar', 'x': 0, 'y': 0, 'w': 100, 'h': 100}], '3': []}
strip = scan.layer_rects({'DP-2': {'levels': bar}}, 'DP-2', 0, 0, 100, 100, dict(monitor, reserved=[0, 26, 0, 0]))
check('a screen-wide bar surface covers only its reserved strip', strip == [[0, 0, 100, 26]])
check('below the bar strip a free point is found', scan.find_free_point(0, 0, 100, 100, strip) == (8, 92))
print(str(checks) + ' retained snapshot shape checks, 0 failed')
PYSHAPES
then
    ok "retained snapshot shapes"
else
    bad "retained snapshot shapes"
fi
if python3 -B "$repo/tests/xwtab-safety.py"; then ok "live shell safety regressions"; else bad "live shell safety regressions"; fi
if python3 -B "$repo/tests/xwtab-gpui-return.py"; then ok "GPUI own-strip return guards"; else bad "GPUI own-strip return guards"; fi
if python3 -B "$repo/tests/layerdrop-receipts.py"; then
    ok "layer receiver receipts"
else
    bad "layer receiver receipts"
fi
if python3 -B "$repo/tests/xwinput-safety.py"; then
    ok "cross-window input routing"
else
    bad "cross-window input routing"
fi
printf '%s checks, %s failed\n' "$((pass+fail))" "$fail"
exit "$((fail>0))"
