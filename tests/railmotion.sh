#!/usr/bin/env bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
. "$(dirname "$0")/headless-commons.sh"
# The auto-hidden rail slides on x in the real window offscreen, once with motion and once with FLEA_REDUCED_MOTION=1.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL railmotion needs qs'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL railmotion needs a private D-Bus session'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL railmotion needs the built backend'; exit 1; }
sandbox_root_ok
probe=$(mktemp -d "$SANDBOX_ROOT/flea-railmotion.XXXXXX") || exit 1
: > "$probe/$SANDBOX_MARKER" || exit 1
qs_pid=""
stop_qs() {
    [[ -n "$qs_pid" ]] || return 0
    local tick
    kill -TERM -- "-$qs_pid" 2>/dev/null || true
    for ((tick = 0; tick < 20; tick++)); do
        kill -0 -- "-$qs_pid" 2>/dev/null || break
        sleep 0.1
    done
    kill -0 -- "-$qs_pid" 2>/dev/null && kill -KILL -- "-$qs_pid" 2>/dev/null
    wait "$qs_pid" 2>/dev/null || true
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        printf 'FAIL railmotion owned process group %s survived teardown\n' "$qs_pid"
        return 1
    fi
    qs_pid=""
}
cleanup() { stop_qs || return 1; sandbox_remove "$probe"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$probe"/{home,config,state,data,cache,runtime,bin} || exit 1
for d in Documents Downloads Pictures Music Videos; do mkdir "$probe/home/$d" && printf x > "$probe/home/$d/note.txt" || exit 1; done
printf x > "$probe/home/note.txt" || exit 1
chmod 700 "$probe/runtime"
# No disk, so the rail's device group answers empty and the same rows stand in both runs.
cat > "$probe/bin/lsblk" <<'SH'
#!/usr/bin/env bash
printf '{"blockdevices":[]}\n'
SH
chmod 700 "$probe/bin/lsblk" || exit 1
ln -s "$PWD/ui" "$probe/config/flea"
headless_commons "$probe" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
ln -s "$PWD/ui/boot/fleatab.qml" "$probe/config/fleatab.qml"
cp tests/railmotion.qml "$probe/config/shell.qml"
cp tests/RailIpc.qml "$probe/config/RailIpc.qml"
cp tests/RailDragHold.qml "$probe/config/RailDragHold.qml"
# Every leg starts from this state: the motion leg ends in Dual and saves it, so the next leg is seeded again.
seed_state() {
    env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state \
        '{"view":"list","preview":{"column":false,"thumbnails":"off"},"updates":{"autoCheck":false},"places":{"autoHide":true}}' >/dev/null
}
run_once() {
    local reduced=$1 tag=$2 log="$probe/qs-$2.log" tick tally
    : > "$log"
    seed_state || return 1
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" \
        XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        PATH="$probe/bin:$PATH" FLEA_REDUCED_MOTION="$reduced" \
        FLEA_PATH="$probe/home" FLEA_BIN="$bin" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
        QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 setsid dbus-run-session -- qs -p "$probe/config" >"$log" 2>&1 </dev/null &
    qs_pid=$!
    for ((tick = 0; tick < 600; tick++)); do
        grep -aq 'RAILMOTION .*DONE checks=[0-9]* failed=[0-9]*' "$log" && break
        sleep 0.1
    done
    stop_qs || return 1
    grep -aE 'RAILMOTION|TypeError|ReferenceError|ERROR|Cannot assign' "$log"
    tally=$(grep -aoE 'RAILMOTION .*DONE checks=[0-9]* failed=[0-9]*' "$log" | tail -1)
    [[ -n "$tally" ]] || { printf 'FAIL railmotion %s produced no completion receipt inside 60s; log tail:\n' "$tag"; tail -20 "$log"; return 1; }
    [[ "$tally" == *' failed=0' ]] || return 1
    if grep -aqE 'TypeError|ReferenceError|ERROR|Cannot assign' "$log"; then
        printf 'FAIL railmotion %s logged an engine error beside its receipt\n' "$tag"
        return 1
    fi
}
run_once "" motion || exit 1
run_once 1 reduced || exit 1
printf 'railmotion: owned process group gone\n'
