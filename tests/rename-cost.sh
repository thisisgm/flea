#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Every cursor key after a rename ends with Enter, Escape, the path field taking focus or a view switch reaches the pane's key handler and costs no more work than before it, in the real WindowBody, offscreen.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL rename-cost needs qs'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL rename-cost needs a private D-Bus session'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL rename-cost needs the built backend'; exit 1; }
sandbox_root_ok
probe=$(mktemp -d "$SANDBOX_ROOT/flea-rename-cost.XXXXXX") || exit 1
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
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        kill -KILL -- "-$qs_pid" 2>/dev/null || true
        for ((tick = 0; tick < 20; tick++)); do
            kill -0 -- "-$qs_pid" 2>/dev/null || break
            sleep 0.1
        done
    fi
    wait "$qs_pid" 2>/dev/null || true
    if kill -0 -- "-$qs_pid" 2>/dev/null; then
        printf 'FAIL rename-cost owned process group %s survived teardown\n' "$qs_pid"
        return 1
    fi
    qs_pid=""
}
cleanup() { stop_qs || return 1; sandbox_remove "$probe"; }
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
mkdir -p "$probe"/{home,config,state,data,cache,runtime} "$probe/home/fixture/sub" || exit 1
chmod 700 "$probe/runtime"
# 400 rows put a name that sorts last outside the window the pane holds around a cursor near the top.
for ((n = 0; n < 400; n++)); do printf -v name 'f%03d.txt' "$n"; printf '%s\n' "$name" > "$probe/home/fixture/$name"; done
mkdir -p "$probe/bin" || exit 1
printf '#!/bin/sh\nexit 0\n' > "$probe/bin/gio"
chmod +x "$probe/bin/gio"
env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state \
    '{"preview":{"column":false,"thumbnails":"off"},"updates":{"autoCheck":false}}' >/dev/null || exit 1
ln -s "$PWD/ui" "$probe/config/flea"
headless_commons "$probe" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
ln -s "$PWD/ui/boot/fleatab.qml" "$probe/config/fleatab.qml"
cp tests/rename-cost.qml "$probe/config/shell.qml"
cp tests/rename-cost.js "$probe/config/rename-cost.js"
log="$probe/qs.log"
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
    PATH="$probe/bin:$PATH" HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" \
    XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
    FLEA_PATH="$probe/home/fixture" FLEA_BIN="$bin" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 setsid dbus-run-session -- qs -p "$probe/config" >"$log" 2>&1 </dev/null &
qs_pid=$!
# Fifteen cases of two measured phases, three view legs or four control scrolls each, polled every 0.1 s for at most 390 s: 26 s per case, the budget the first seven cases had in 180 s.
receipt_ticks=3900
for ((tick = 0; tick < receipt_ticks; tick++)); do
    grep -aq 'RENAMECOST DONE checks=[0-9]* failed=[0-9]*' "$log" && break
    sleep 0.1
done
stop_qs || exit 1
printf 'rename-cost: owned process group gone\n'
grep -aE 'RENAMECOST|TypeError|ReferenceError|ERROR|Cannot assign' "$log"
tally=$(grep -ao 'RENAMECOST DONE checks=[0-9]* failed=[0-9]*' "$log" | tail -1)
[[ -n "$tally" ]] || { printf 'FAIL rename-cost produced no completion receipt inside 390s; log tail:\n'; tail -20 "$log"; exit 1; }
[[ "$tally" == *' failed=0' ]] || exit 1
if grep -aqE 'TypeError|ReferenceError|ERROR|Cannot assign' "$log"; then
    echo 'FAIL rename-cost logged an engine error beside its receipt'
    exit 1
fi
