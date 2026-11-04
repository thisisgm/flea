#!/bin/bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
# Drives two shipped WindowBody instances offscreen through a test seam that exists only in a sandbox copy.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
. tools/flea-sandbox-guard
sandbox_root_ok
box=$(mktemp -d "$SANDBOX_ROOT/flea-xwstate.XXXXXXXX") || exit 1
: > "$box/$SANDBOX_MARKER" || exit 1
# Poll budgets: state reads and file waits share one, window readiness has its own.
STATE_POLLS=60
STATE_POLL_SECONDS=0.05
READY_POLLS=100
READY_POLL_SECONDS=0.1
WINDOW_TIMEOUT_SECONDS=90
CALL_TIMEOUT_SECONDS=5
# The one notice a cut raises offscreen, where there is no clipboard to share.
ONCE_NOTICE='["Copied in this window only: WAYLAND_DISPLAY is not set, so there is no clipboard to use"]'
pass=0
fail=0
a_pid=
b_pid=
a_runner=
b_runner=
cleanup() {
    local pid
    for pid in "$a_pid" "$b_pid" "$a_runner" "$b_runner"; do
        [ -z "$pid" ] || kill "$pid" 2>/dev/null || true
    done
    [ -z "$a_runner" ] || wait "$a_runner" 2>/dev/null || true
    [ -z "$b_runner" ] || wait "$b_runner" 2>/dev/null || true
    sandbox_remove "$box"
}
trap cleanup EXIT
check() {
    if [[ "$2" == "$3" ]]; then
        printf 'ok   %s\n' "$1"
        pass=$((pass + 1))
    else
        printf 'FAIL %s: got %s, expected %s\n' "$1" "$3" "$2"
        fail=$((fail + 1))
    fi
}
broken() {
    printf 'FAIL xwstate harness: %s\n' "$*"
    local pid log
    for pid in "$a_pid" "$b_pid"; do
        [ -z "$pid" ] || call "$pid" state 2>/dev/null || true
    done
    for log in "$box/a.log" "$box/b.log"; do
        [ ! -f "$log" ] || tail -12 "$log"
    done
    exit 1
}
bin=$PWD/target/debug/flea
[ -x "$bin" ] || broken "candidate backend missing"
cp -a ui "$box/ui" || broken "could not copy shipped UI"
cp tests/xwstate-control.qml "$box/ui/xwstate-control.qml" || broken "could not copy the test seam"
mkdir -p "$box"/{home,state,config,data,cache,runtime,a,b} || exit 1
chmod 700 "$box/runtime" || exit 1
env XDG_STATE_HOME="$box/state" XDG_CONFIG_HOME="$box/config" "$bin" --ui-state '{}' >/dev/null \
    || broken "could not seed the launcher's initial state"
printf 'alpha\n' > "$box/a/alpha.txt"
printf 'zulu\n' > "$box/a/zulu.txt"
printf 'beta\n' > "$box/b/beta.txt"
printf 'omega\n' > "$box/b/omega.txt"
# Sample input: the boot file's single line "ShellRoot {", which the seam is inserted after.
python3 - "$box/ui/boot/shell.qml" <<'PY' || broken "could not install the test seam"
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
anchor = 'ShellRoot {\n'
if text.count(anchor) != 1:
    sys.exit("seam anchor 'ShellRoot {' occurs %d times in the boot file, expected once" % text.count(anchor))
seam = '''    Loader {
        source: "file://" + Quickshell.shellDir + "/../xwstate-control.qml"
        onLoaded: item.view = Qt.binding(function () { return bodyLoader.item })
    }
'''
text = text.replace(anchor, anchor + seam, 1)
path.write_text(text)
PY
launch() {
    exec env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
        HOME="$box/home" XDG_CONFIG_HOME="$box/config" XDG_STATE_HOME="$box/state" \
        XDG_DATA_HOME="$box/data" XDG_CACHE_HOME="$box/cache" XDG_RUNTIME_DIR="$box/runtime" \
        FLEA_PATH="$box/$1" FLEA_BIN="$bin" FLEA_HUNT_ROOT="$box" QSG_RHI_BACKEND=opengl \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
        timeout "$WINDOW_TIMEOUT_SECONDS" qs -p "$box/ui/boot" > "$box/$1.log" 2>&1
}
call() {
    env XDG_RUNTIME_DIR="$box/runtime" timeout "$CALL_TIMEOUT_SECONDS" "$FLEA_IPC_HELPER" --pid "$1" call hunt "$2" "${@:3}"
}
await_state() {
    local pid=$1 predicate=$2 attempt snapshot
    for attempt in $(seq 1 "$STATE_POLLS"); do
        snapshot=$(call "$pid" state 2>/dev/null) || return 1
        jq -e "$predicate" <<< "$snapshot" >/dev/null && return 0
        sleep "$STATE_POLL_SECONDS"
    done
    return 1
}
# Waits for a filesystem condition, given as a command: await_file test -d "$dir".
await_file() {
    local attempt
    for attempt in $(seq 1 "$STATE_POLLS"); do
        "$@" && return 0
        sleep "$STATE_POLL_SECONDS"
    done
    return 1
}
# A's alpha.txt sits in B (moved), or sits back in A (restored).
file_moved() { [ -f "$box/b/alpha.txt" ] && [ ! -e "$box/a/alpha.txt" ]; }
file_restored() { [ -f "$box/a/alpha.txt" ] && [ ! -e "$box/b/alpha.txt" ]; }
for name in a b; do
    launch "$name" &
    runner=$!
    if [[ "$name" == a ]]; then a_runner=$runner; else b_runner=$runner; fi
    pid=
    for attempt in $(seq 1 "$READY_POLLS"); do
        pid=$(sed -n 's/.*HUNT pid=\([0-9]*\).*/\1/p' "$box/$name.log" | head -1)
        # Stored as soon as it is read, so cleanup reaches a window that never turns ready.
        if [[ -n "$pid" ]]; then
            if [[ "$name" == a ]]; then a_pid=$pid; else b_pid=$pid; fi
        fi
        if [[ -n "$pid" ]] && [[ "$(call "$pid" ready 2>/dev/null)" == true ]]; then break; fi
        sleep "$READY_POLL_SECONDS"
    done
    if [[ -z "$pid" ]] || [[ "$(call "$pid" ready 2>/dev/null)" != true ]]; then
        tail -15 "$box/$name.log"
        broken "window $name never became ready"
    fi
done
printf 'xwstate: two shipped windows ready, pid A=%s B=%s\n' "$a_pid" "$b_pid"

# Complete the received tab's real list and rows path, rather than inspecting its saved snapshot.
[[ "$(call "$b_pid" receiveCursor)" == true ]] || broken "received cursor control was refused"
await_state "$b_pid" '.loading == false and (.path | endswith("/a"))' \
    || broken "received cursor control never listed"
check "cross-window received tab keeps the source cursor file" zulu.txt "$(call "$b_pid" state | jq -r .cursorName)"
call "$b_pid" closeTab >/dev/null || broken "could not restore B's original tab"
await_state "$b_pid" '.loading == false and (.path | endswith("/b"))' \
    || broken "B's original tab did not return"

# A's writer and B's file watcher must run, demonstrated by Favorites before checking keys.
call "$a_pid" changeKeys >/dev/null || broken "could not set keys"
call "$a_pid" changeFavourites >/dev/null \
    || broken "could not set Favorites control"
await_state "$b_pid" '.favourites | length == 1' || broken "Favorites watcher control never landed"
check "settings control applies keys in window A" windows "$(call "$a_pid" state | jq -r .keys)"
check "settings control persists keys in the shared file" windows "$(jq -r .keys "$box/state/flea/ui.json")"
check "cross-window settings keys reach idle window B" windows "$(call "$b_pid" state | jq -r .keys)"

call "$a_pid" selectSource >/dev/null || broken "could not retain A's source row for the mark check"
call "$a_pid" cutSource >/dev/null || broken "could not cut A's row"
await_state "$a_pid" '.clipboard.moving and (.clipboard.paths | length == 1)' \
    || broken "cut did not resolve the cursor row"
check "cut draws A's clipboard mark" scissors "$(call "$a_pid" state | jq -r .mark)"
# Offscreen there is no clipboard to share, so the cut stays in A; the native clipboard case covers two windows.
await_state "$a_pid" '.notices | length > 0' || true
check "A says the cut is kept in this window only, once" "$ONCE_NOTICE" "$(call "$a_pid" state | jq -c .notices)"
# A's second setting is a later cross-window event: once B has seen it, A's cut had its chance to reach B.
call "$a_pid" changeFavouritesAgain >/dev/null || broken "could not set the second Favorites control"
await_state "$b_pid" '(.favourites | length == 1) and .favourites[0].label == "Second"' \
    || broken "window B never saw A's second setting"
check "A's cut does not reach window B" true \
    "$(call "$b_pid" state | jq '.clipboard | type == "object" and (.paths | type == "array" and length == 0)')"
check "A still says the cut once after B saw A's later write" "$ONCE_NOTICE" \
    "$(call "$a_pid" state | jq -c .notices)"

# A pastes its own cut into the other fixture folder; the file moves and A's clipboard empties.
call "$a_pid" openFixture b >/dev/null || broken "could not open the paste folder in A"
await_state "$a_pid" '.loading == false and (.path | endswith("/b"))' || broken "A never listed the paste folder"
call "$a_pid" pasteCut >/dev/null || broken "could not paste in A"
await_state "$a_pid" '.clipboard.paths | length == 0' || true
await_file file_moved || true
check "A's paste of its own cut moves the actual file" moved "$(file_moved && echo moved || echo missing)"
check "A's spent cut empties A's clipboard" 0 "$(call "$a_pid" state | jq -r '.clipboard.paths | length')"
# Put the file and A's folder back so the return leg below starts from the original fixture.
call "$a_pid" undoLast >/dev/null || broken "could not undo A's paste"
await_file file_restored || broken "A's paste was not undone"
call "$a_pid" openFixture a >/dev/null || broken "could not return A to its folder"
await_state "$a_pid" '.loading == false and (.path | endswith("/a"))' || broken "A never listed its folder again"
check "A says the cut once, after its paste and the undo" "$ONCE_NOTICE" "$(call "$a_pid" state | jq -c .notices)"

# Supply B the same cut, then paste through its real collision, transfer and reply path.
call "$b_pid" seedCut >/dev/null || broken "could not seed return-leg control"
call "$b_pid" pasteCut >/dev/null || broken "could not paste in B"
await_state "$b_pid" '.clipboard.paths | length == 0' || broken "B did not spend the cut"
await_file file_moved || true
check "B's paste control moves the actual file" moved "$(file_moved && echo moved || echo missing)"

# Remove B's previous local operation before asking it to undo A's later one.
call "$b_pid" undoLast >/dev/null || broken "could not undo B's paste control"
await_file file_restored || true
check "local undo control reverses B's paste" restored "$(file_restored && echo restored || echo missing)"

# A's reversible operation must be undoable from B; the filesystem is the verdict.
call "$a_pid" newFolder >/dev/null || broken "could not create A's folder"
await_file test -d "$box/a/New Folder" || broken "A's new folder never landed"
call "$b_pid" undoLast >/dev/null || broken "could not request B's undo"
await_state "$b_pid" '.error or (.message | startswith("Removed"))' || true
check "cross-window undo from B reverses A's new folder" absent \
    "$([ -d "$box/a/New Folder" ] && echo present || echo absent)"
printf 'xwstate: B undo message=%s\n' "$(call "$b_pid" state | jq -r .message)"
# Diagnostic only: when B's undo left the folder, say whether A's own undo removes it.
if [ -d "$box/a/New Folder" ]; then
    call "$a_pid" undoLast >/dev/null || broken "could not request local undo control"
    if await_file test ! -d "$box/a/New Folder"; then removed=removes; else removed="does not remove"; fi
    printf "xwstate: B's undo left the folder; A's own undo %s it\n" "$removed"
fi

printf 'xwstate: %s checks, %s failed\n' "$((pass + fail))" "$fail"
[ "$fail" -eq 0 ]
