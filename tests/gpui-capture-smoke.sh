#!/usr/bin/env bash
# Capture real owned Wayland clients in a private Hyprland session, never an offscreen renderer.
set -euo pipefail
ulimit -c 0
repo=$(cd "$(dirname "$0")/.." && pwd -P)
if [[ ${FLEA_GPUI_CAPTURE_CLEAN:-} != 1 ]]; then
    exec setsid -w env -i PATH=/usr/local/bin:/usr/bin:/bin FLEA_GPUI_CAPTURE_CLEAN=1 \
        CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repo/target}" FLEA_GPUI_SCENES="${FLEA_GPUI_SCENES:-0}" /bin/bash "$repo/tests/gpui-capture-smoke.sh" </dev/null
fi
# Only these typed functions build compositor commands; cursor parking goes through hypr_cursor_move below.
. "$repo/tests/lib/hypr-dispatch.sh"
parent=$repo/.superpowers/g010c
mkdir -p "$parent"
root=$(mktemp -d "$parent/run.XXXXXX")
: > "$root/.flea-test-sandbox"
logs=$(mktemp -d "$parent/evidence.XXXXXX")
: > "$logs/.flea-test-sandbox"
helper=$repo/tests/gpui-capture-check.py
owned=()
cleanup() {
    local rc=$? pid report
    trap - EXIT INT TERM
    for pid in "${owned[@]}"; do
        [[ $pid =~ ^[0-9]+$ && $pid -gt 1 ]] || continue
        kill -TERM -- "-$pid" 2>/dev/null || true
    done
    for report in "$root/runtime"/hypr/*/hyprland.log; do
        [[ ! -f $report ]] || cp "$report" "$logs/compositor-detail.log"
    done
    for report in "$root/cache"/hyprland/hyprlandCrashReport*.txt; do
        [[ ! -f $report ]] || cp "$report" "$logs/${report##*/}"
    done
    sleep 0.2
    for pid in "${owned[@]}"; do
        kill -KILL -- "-$pid" 2>/dev/null || true
        wait "$pid" 2>/dev/null || true
    done
    python3 -B "$helper" artifacts "$logs" || true
    printf 'GPUI_CAPTURE_LOG_DIR %s\n' "$logs"
    case $root in
        "$parent"/run.?*)
            [[ $root == /* && -n $root && -f $root/.flea-test-sandbox ]] && rm -rf -- "$root" ;;
    esac
    exit "$rc"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
fail() { printf 'GPUI_CAPTURE FAIL %s\n' "$*" >&2; exit 1; }
stop_owned() {
    local pid=$1 index attempt
    kill -TERM -- "-$pid" 2>/dev/null || true
    for ((attempt=0; attempt<50; attempt++)); do
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done
    kill -KILL -- "-$pid" 2>/dev/null || true
    wait "$pid" 2>/dev/null || true
    for index in "${!owned[@]}"; do
        [[ ${owned[index]} != "$pid" ]] || unset 'owned[index]'
    done
}
for tool in Hyprland hyprctl grim jq python3 setsid dbus-daemon cmp timeout; do
    command -v "$tool" >/dev/null || fail "missing executable: $tool"
done
python3 -B -c 'from PIL import Image' || fail 'missing python-pillow'
target=${CARGO_TARGET_DIR:-$repo/target}
[[ $target == /* ]] || target=$repo/$target
bin=$target/debug/flea-gui
ipc=$target/debug/flea
[[ -x $bin && -x $ipc ]] || fail "build this worktree first: $bin and $ipc"
cd "$root"
mkdir -p "$root/"{runtime,home,config,cache,state,data,fixture}
chmod 700 "$root/runtime"
# Keep Unix socket paths short even when the worktree has a long absolute name.
exec 9<"$root"
runtime=/proc/$$/fd/9/runtime
private=(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u GPUI_HEADLESS
    -u VK_DRIVER_FILES -u VK_ICD_FILENAMES -u VK_ADD_DRIVER_FILES
    -u VK_LOADER_DRIVERS_SELECT -u VK_LOADER_DRIVERS_DISABLE -u WGPU_BACKEND
    HOME="$root/home" XDG_RUNTIME_DIR="$runtime" XDG_CONFIG_HOME="$root/config"
    XDG_CACHE_HOME="$root/cache" XDG_STATE_HOME="$root/state" XDG_DATA_HOME="$root/data"
    USER=gm LOGNAME=gm SHELL=/bin/bash LANG=en_US.UTF-8 TZ=UTC TMPDIR="$root" LIBSEAT_BACKEND=noop)
python3 -B "$helper" controls "$root" | tee "$logs/controls.log"
printf 'one\n' > "$root/fixture/alpha.txt"
printf 'two\n' > "$root/fixture/beta.txt"
touch -d '2025-06-15 10:00:00 UTC' "$root/fixture/"*.txt
"${private[@]}" "$ipc" --ui-state '{"display":{"textSize":{"mode":14}},"updates":{"autoCheck":false}}' >/dev/null
setsid "${private[@]}" dbus-daemon --session --nofork --address="unix:path=$runtime/bus" > "$logs/dbus.log" 2>&1 &
owned+=("$!")
for ((attempt=0; attempt<100; attempt++)); do
    [[ -S $runtime/bus ]] && break
    sleep 0.1
done
[[ -S $runtime/bus ]] || fail 'private D-Bus did not start'
private+=(DBUS_SESSION_BUS_ADDRESS="unix:path=$runtime/bus")
cat > "$root/config/hyprland.lua" <<'LUA'
hl.monitor({ output = "FLEA", mode = "2560x1440@120", position = "0x0", scale = 1 })
hl.monitor({ output = "Virtual-1", disabled = true })
hl.window_rule({ match = { class = "^com[.]thisisgm[.]flea$" }, float = true, center = true, size = { 1100, 700 } })
hl.config({ animations = { enabled = false }, input = { repeat_rate = 0 },
    decoration = { rounding = 0, shadow = { enabled = false } },
    misc = { disable_hyprland_logo = true, disable_splash_rendering = true } })
LUA
setsid "${private[@]}" Hyprland --config "$root/config/hyprland.lua" > "$logs/hyprland.log" 2>&1 &
hypr_pid=$!
owned+=("$hypr_pid")
signature=
for ((attempt=0; attempt<100; attempt++)); do
    for socket in "$runtime"/hypr/*/.socket.sock; do
        [[ -S $socket ]] || continue
        signature=${socket%/.socket.sock}
        signature=${signature##*/}
        break
    done
    [[ -n $signature ]] && break
    kill -0 "$hypr_pid" 2>/dev/null || break
    sleep 0.1
done
if [[ -z $signature ]]; then
    tail -60 "$logs/hyprland.log" >&2
    fail 'hard wall: Hyprland did not open its private control socket'
fi
private+=(HYPRLAND_INSTANCE_SIGNATURE="$signature")
hctl() { "${private[@]}" timeout 5 hyprctl "$@"; }
output_create_rc=0
"${private[@]}" timeout 30 hyprctl output create headless FLEA > "$logs/output-create.log" 2>&1 || output_create_rc=$?
printf '%s\n' "$output_create_rc" > "$logs/output-create.rc"
if [[ $output_create_rc != 0 ]]; then
    cat "$logs/output-create.log" >&2
    fail "hard wall: Hyprland could not create FLEA (exit=$output_create_rc)"
fi
display=
for socket in "$runtime"/wayland-*; do
    [[ -S $socket ]] && { display=${socket##*/}; break; }
done
[[ -n $display ]] || fail 'hard wall: Hyprland did not open a Wayland socket'
private+=(WAYLAND_DISPLAY="$display")
hctl monitors -j > "$logs/monitors.json"
hctl configerrors > "$logs/configerrors.log"
if grep -q '[^[:space:]]' "$logs/configerrors.log"; then
    cat "$logs/configerrors.log" >&2
    fail 'private Hyprland configuration has errors'
fi
hctl getoption animations:enabled -j | jq -e '.bool == false' >/dev/null
lvp=
for manifest in /usr/share/vulkan/icd.d/lvp_icd*.json; do
    [[ -f $manifest ]] && { lvp=$manifest; break; }
done
[[ -n $lvp ]] || fail 'missing lavapipe ICD manifest'
wait_client() {
    local pid=$1 out=$2 tries=$3
    for ((poll=0; poll<tries; poll++)); do
        hctl clients -j > "$logs/clients.json" || return 1
        if python3 -B "$helper" client "$logs/clients.json" "$pid" > "$out" 2> "$logs/map-error.log"; then
            return 0
        fi
        kill -0 "$pid" 2>/dev/null || break
        sleep 0.1
    done
    cat "$logs/map-error.log" >&2
    return 1
}
if [[ ${FLEA_GPUI_SCENES:-0} == 1 ]]; then
    "${private[@]}" VK_DRIVER_FILES="$lvp" VK_ICD_FILENAMES="$lvp" \
        LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe FLEA_GPUI_CAPTURE=1 \
        FLEA_BIN="$ipc" FLEA_TEST_FRONTEND=gpui \
        python3 -B "$repo/tests/gpui-visual-scenes.py" "$root" "$logs"
    exit 0
fi
setsid sleep 30 &
never_pid=$!
owned+=("$never_pid")
if wait_client "$never_pid" "$root/never.json" 10 > "$logs/never-maps.log" 2>&1; then
    fail 'never-maps control was accepted'
fi
grep -q 'GPUI_CAPTURE_REJECT window_never_mapped' "$logs/never-maps.log" || fail 'wrong never-maps rejection'
printf 'GPUI_CAPTURE_CONTROL name=never-maps rejected=%s\n' "$(cat "$logs/never-maps.log")"
stop_owned "$never_pid"
for backend in vulkan gl; do
    driver=$lvp
    [[ $backend == vulkan ]] || driver=$root/no-vulkan-icd.json
    setsid "${private[@]}" VK_DRIVER_FILES="$driver" VK_ICD_FILENAMES="$driver" \
        LIBGL_ALWAYS_SOFTWARE=1 GALLIUM_DRIVER=llvmpipe FLEA_GPUI_CAPTURE=1 FLEA_BIN="$ipc" \
        "$bin" "$root/fixture" > "$logs/$backend-app.log" 2>&1 &
    app_pid=$!
    owned+=("$app_pid")
    if ! wait_client "$app_pid" "$logs/$backend-client.json" 200; then
        cat "$logs/$backend-app.log" >&2
        fail "hard wall: $backend client never mapped"
    fi
    ready=false
    for ((attempt=0; attempt<100; attempt++)); do
        if [[ $("${private[@]}" timeout 5 "$ipc" ipc --pid "$app_pid" call flea total 2> "$logs/ipc-error.log") == 2 ]]; then
            ready=true
            break
        fi
        sleep 0.1
    done
    [[ $ready == true ]] || {
        cat "$logs/$backend-app.log" >&2
        fail "$backend fixture did not reach the owned IPC surface"
    }
    HYPRLAND_INSTANCE_SIGNATURE="$signature" XDG_RUNTIME_DIR="$runtime" hypr_cursor_move 2559 1439 > "$logs/$backend-pointer-park.log"
    hctl cursorpos -j | jq -e '.x == 2559 and .y == 1439' > "$logs/$backend-pointer-check.log"
    geometry=$(jq -r '"\(.at[0]),\(.at[1]) \(.size[0])x\(.size[1])"' "$logs/$backend-client.json")
    "${private[@]}" timeout 10 grim -g "$geometry" "$root/a.png"
    stable=false
    for ((attempt=1; attempt<=20; attempt++)); do
        sleep 1
        "${private[@]}" timeout 10 grim -g "$geometry" "$root/b.png"
        if cmp -s "$root/a.png" "$root/b.png" && \
            python3 -B "$helper" frame "$root/b.png" "$logs/$backend-client.json" > "$logs/$backend-frame.log" 2>&1; then
            cp "$root/b.png" "$logs/$backend.png"
            stable=true
            break
        fi
        cp "$root/b.png" "$root/a.png"
    done
    [[ $stable == true ]] || {
        cat "$logs/$backend-frame.log" >&2 2>/dev/null || true
        fail "$backend frame did not become nonuniform and byte-identical in 20 tries"
    }
    printf 'GPUI_CAPTURE_PAIR backend=%s tries=%s interval=1s\n' "$backend" "$attempt"
    wait_client "$app_pid" "$logs/$backend-final-client.json" 1 || fail "$backend client disappeared during capture"
    for field in at size monitor; do
        [[ $(jq -c ".$field" "$logs/$backend-final-client.json") == "$(jq -c ".$field" "$logs/$backend-client.json")" ]] || \
            fail "$backend client $field changed during capture"
    done
    python3 -B "$helper" check "$logs" "$backend"
    grep '^GPUI_RENDERER ' "$logs/$backend-app.log"
    printf 'GPUI_CAPTURE_STABLE backend=%s tries=%s interval=1s png=%s\n' "$backend" "$attempt" "$logs/$backend.png"
    stop_owned "$app_pid"
done
