#!/usr/bin/env bash
# Native drag regression checks through the identified product launcher and owned fixtures.
#
# Motion goes through uinput and never through hl.dsp.cursor.move. That warp emits wl_pointer.motion
# with no wl_pointer.frame, and Qt dispatches buffered pointer events only on frame, so a drag driven
# that way is never seen by the application at all: measured on this box, 24 motions and 0 frames.
# omarchy-drive drag interpolates with that warp, which is why this suite does not use it.
set -u
set -o pipefail

repo="$(cd "$(dirname "$0")/.." && pwd)"
# Without this the UI resolves "flea" from PATH, which is the installed package and not this tree.
export FLEA_BIN="${FLEA_BIN:-$repo/target/release/flea}"
export FLEA_UI="$repo/ui"
. "$repo/tools/flea-sandbox-guard"

SB=$FIXTURE_ROOT/flea-drag-char-$$
HOMEDIR=$SB/home
pass=0
fail=0
button_down=false
control_down=false
pointer_tolerance=4

export XDG_RUNTIME_DIR=/run/user/$(id -u)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1}
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t "$XDG_RUNTIME_DIR"/hypr/ | head -1)
export YDOTOOL_SOCKET=$XDG_RUNTIME_DIR/.ydotool_socket

ok()   { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad()  { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }
note() { printf '     %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1"; note "expected [$3]"; note "got      [$2]"; fi; }
die() { bad "$*"; exit 1; }

stop_owned_processes() {
  [[ -n "${FLEA_PID:-}" ]] || return 0
  python3 - "$SB" "$FLEA_PID" <<'PY'
import os, signal, sys, time
from pathlib import Path

root, session = Path(sys.argv[1]), int(sys.argv[2])
if not root.is_absolute() or not (root / ".flea-test-sandbox").is_file():
    raise RuntimeError("drag cleanup: ownership root is missing; processes and fixtures kept")
marker = b"FLEA_TEST_RUN_ROOT=" + os.fsencode(root)
drain_seconds, kill_wait_seconds, poll_seconds = 30, 5, 0.05

def owned_processes(pid=None):
    processes = [Path("/proc", str(pid))] if pid else Path("/proc").iterdir()
    owned = []
    for process in processes:
        if not process.name.isdigit():
            continue
        number = int(process.name)
        try:
            if os.getsid(number) != session:
                continue
            if process.stat().st_uid != os.getuid():
                raise RuntimeError(f"drag cleanup: session process {number} has another owner")
            # /proc/stat follows "pid (comm) state ..."; a zombie cannot receive input or write fixtures.
            if (process / "stat").read_text().rsplit(")", 1)[1].split()[0] == "Z":
                continue
            try:
                environment = (process / "environ").read_bytes()
            except PermissionError as error:
                # Exit can revoke environ access after the live-state check; only a confirmed zombie is safe to skip.
                if (process / "stat").read_text().rsplit(")", 1)[1].split()[0] == "Z":
                    continue
                raise RuntimeError(f"drag cleanup: live session process {number} has unreadable environ; fixtures kept") from error
            if marker not in environment.split(b"\0"):
                raise RuntimeError(f"drag cleanup: session process {number} lacks this run's marker")
            owned.append(number)
        except (FileNotFoundError, ProcessLookupError):
            continue
    return owned

def signal_owned(pid, value):
    try:
        descriptor = os.pidfd_open(pid)
        try:
            if pid in owned_processes(pid):
                signal.pidfd_send_signal(descriptor, value)
        finally:
            os.close(descriptor)
    except ProcessLookupError:
        pass

def drain(seconds):
    deadline = time.monotonic() + seconds
    remaining = owned_processes()
    while remaining and time.monotonic() < deadline:
        time.sleep(poll_seconds)
        remaining = owned_processes()
    return remaining

for pid in owned_processes():
    signal_owned(pid, signal.SIGCONT)
    signal_owned(pid, signal.SIGTERM)
# Match ui.sh/TUI: the backend has a 25-second drain limit, with a 30-second observation deadline.
remaining = drain(drain_seconds)
if remaining:
    for pid in remaining:
        signal_owned(pid, signal.SIGKILL)
    survivors = drain(kill_wait_seconds)
    raise RuntimeError(f"drag cleanup: processes {remaining} exceeded drain; SIGKILL survivors={survivors}; fixtures kept")
print("DRAG_DRAIN owned_processes=0 forced_kill=false")
PY
}

cleanup() {
  local status=$? drained=true
  trap - EXIT
  if [ "$button_down" = true ]; then
    ydotool key 1:1 1:0 >/dev/null 2>&1 || { bad "cleanup could not cancel the held drag"; status=1; }
  fi
  stop_owned_processes || { status=1; drained=false; }
  # Teardown is bounded even when the owned application cannot drain; failed teardown retains its fixtures.
  if [ "$button_down" = true ]; then
    ydotool click 0x80 >/dev/null 2>&1 || { bad "cleanup could not release the pointer"; status=1; }
  fi
  if [ "$control_down" = true ]; then
    ydotool key 29:0 >/dev/null 2>&1 || { bad "cleanup could not release Ctrl"; status=1; }
  fi
  if [ -f "$SB/flea.log" ]; then
    note "native stderr from $SB/flea.log"
    cat -- "$SB/flea.log"
  fi
  if [ "$drained" = true ]; then
    sandbox_remove "$SB" 2>/dev/null
    # R7's tmpfs root has its own mktemp and marker, checked again before deletion.
    case "${XDEV:-}" in /dev/shm/flea-drag-xdev-*) FIXTURE_ROOT=/dev/shm sandbox_remove "$XDEV" ;; esac
  else
    bad "cleanup did not drain; fixtures kept at $SB ${XDEV:-}"
  fi
  exit "$status"
}
trap cleanup EXIT

# ---------------------------------------------------------------- fixture
sandbox_make "$SB"
export XDG_CONFIG_HOME="$HOMEDIR/.config" XDG_STATE_HOME="$HOMEDIR/.local/state"
export XDG_DATA_HOME="$HOMEDIR/.local/share" XDG_CACHE_HOME="$HOMEDIR/.cache"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME/omarchy" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$HOMEDIR/aaa" "$HOMEDIR/bbb"
ln -sfn "$HOME/.local/state/omarchy/current" "$HOMEDIR/.local/state/omarchy/current"
for f in r1a r1b r2 r3 r4; do printf '%s payload\n' "$f" > "$HOMEDIR/$f.txt"; done
# R6 shows hidden files in a second tab on the same directory, so every index below this one shifts.
printf 'hidden\n' > "$HOMEDIR/.r0hidden"

# ---------------------------------------------------------------- pointer
warp() { glide_to "$1" "$2"; }
move_rel() { ydotool mousemove -x "$1" -y "$2" >/dev/null 2>&1 || die "relative pointer motion failed"; }
press()   { pressed_path=$(ipc path); owned_path "$pressed_path"; button_down=true; ydotool click 0x40 >/dev/null 2>&1 || die "pointer press failed"; }
release() { owned_path "$pressed_path"; ydotool click 0x80 >/dev/null 2>&1 || die "pointer release failed"; button_down=false; }
# evdev KEY_LEFTCTRL. Held through ydotool because a compositor keybind must not swallow it.
ctrl_down() { control_down=true; ydotool key 29:1 >/dev/null 2>&1 || die "Ctrl press failed"; }
ctrl_up()   { ydotool key 29:0 >/dev/null 2>&1 || die "Ctrl release failed"; control_down=false; }

# glide_to x y : converge on an absolute target with real frame-carrying motion. libinput accelerates
# relative motion about 2x here, so each step is half the remaining distance and re-read, never trusted.
glide_to() {
  local tx=$1 ty=$2 i cx cy dx dy
  [[ "$tx $ty" =~ ^-?[0-9]+\ -?[0-9]+$ ]] || die "invalid native pointer target"
  for i in $(seq 1 16); do
    set -- $(hyprctl cursorpos | tr -d ",")
    cx=$1; cy=$2
    dx=$(( tx - cx )); dy=$(( ty - cy ))
    if [ "${dx#-}" -le "$pointer_tolerance" ] && [ "${dy#-}" -le "$pointer_tolerance" ]; then return 0; fi
    move_rel $(( dx / 2 )) $(( dy / 2 ))
    sleep 0.05
  done
  die "pointer did not reach $tx,$ty; observed $cx,$cy"
}

# ---------------------------------------------------------------- the app
# The instance id and the process id together: the id addresses IPC, the pid finds this suite's own
# window. Matching the window by class alone aborted three runs beside another lane's Flea, which is
# right to refuse but needlessly blind, because the pid is already in hand.
myid() {
  qs list --all --json 2>/dev/null | python3 -c '
import json, sys
hits = [i for i in json.load(sys.stdin) if i["config_path"] == sys.argv[1] and i["pid"] == int(sys.argv[2])]
if len(hits) != 1:
    sys.exit(1)
print("%s %s" % (hits[0]["id"], hits[0]["pid"]))
' "$repo/ui/shell.qml" "$FLEA_PID"
}
ipc() { qs ipc -i "$MYID" call flea "$@" 2>&1; }
native_key() {
  local result=0
  omarchy-drive key --window flea "$@" || result=$?
  (( result == 0 )) || die "native key delivery failed with status $result: $*"
}
expect_ipc() {
  local reader="$1" expected="$2" observed attempt
  for ((attempt=1; attempt<=40; attempt++)); do
    observed=$(ipc "$reader") || die "native observer failed: $reader"
    if [[ "$observed" == "$expected" ]]; then ok "$reader = $expected"; return; fi
    sleep 0.25
  done
  die "$reader expected [$expected], observed [$observed]"
}

r5_state() {
  local phase="$1" reader value
  for reader in tabCount tabIndex tabLabels path keyDeliveryState pathBarOpen; do
    value=$(ipc "$reader") || die "R5 $phase observer failed: $reader: $value"
    printf 'DRAG_R5 phase=%s reader=%s value=%q\n' "$phase" "$reader" "$value"
  done
}

# The product entry resolves the UI, renderer and backend identity before execing Quickshell.
QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" HOME="$HOMEDIR" FLEA_TEST_RUN_ROOT="$SB" \
  setsid "$FLEA_BIN" --gui "$HOMEDIR" >"$SB/flea.log" 2>&1 &
FLEA_PID=$!
MYID=""
MYPID=""
for i in $(seq 1 60); do
  pair=$(myid) || { sleep 0.5; continue; }
  set -- $pair; MYID=$1; MYPID=$2
  [ -n "$MYID" ] && [ "$(ipc ready)" = "true" ] && break
  sleep 0.5
done
[ -n "$MYID" ] || { echo "no instance of $repo/ui/shell.qml came up"; exit 1; }
[ "$(ipc path)" = "$HOMEDIR" ] || { echo "ipc answered '$(ipc path)', not the fixture $HOMEDIR"; exit 1; }
[ "$(ipc themeLoaded)" = "true" ] || { echo "theme did not load in the fixture home"; exit 1; }

# Two guards, and both are needed. The pid finds this suite's own window, because matching on class
# alone is ambiguous beside another lane's Flea. The refusal is separate and stands anyway: this
# suite drives a real pointer across the screen, so a second Flea window changes the tiling under it
# and can take the drop. One run beside a foreign Flea reported the window 30px high and failed R2
# for no reason but that, which is a wrong answer, not a flaky one.
FLEACOUNT=$(hyprctl clients -j | python3 -c '
import json, sys
print(sum(1 for w in json.load(sys.stdin) if w["class"] == "com.thisisgm.flea"))')
[ "$FLEACOUNT" = "1" ] || { echo "refusing: $FLEACOUNT Flea windows are open, and this suite needs the screen to itself"; exit 1; }
WIN=$(hyprctl clients -j | python3 -c '
import json, sys
hits = [w for w in json.load(sys.stdin) if str(w["pid"]) == sys.argv[1]]
if len(hits) != 1:
    sys.exit(1)
print(hits[0]["at"][0], hits[0]["at"][1], hits[0]["size"][0], hits[0]["size"][1])
' "$MYPID") || { echo "no window belonging to this suite (pid $MYPID)"; exit 1; }
set -- $WIN; WX=$1; WY=$2; WW=$3; WH=$4

# rowidx <name> : the listing index whose row is called name, refusing rather than guessing.
rowidx() {
  local i n total
  total=$(ipc total)
  for i in $(seq 0 $((total - 1))); do
    n=$(ipc rowAt "$i")
    case "$n" in "$1|"*) echo "$i"; return 0;; esac
  done
  return 1
}
# screen_centre <name> : absolute pointer coordinates of that row's centre, read after tiling.
screen_centre() {
  local idx c
  idx=$(rowidx "$1") || return 1
  c=$(ipc rowCentre "$idx")
  [[ "$c" =~ ^[0-9]+\ [0-9]+$ ]] || return 1
  set -- $c
  (( $1 > 0 && $2 > 0 && $1 < WW && $2 < WH )) || return 1
  echo $(( WX + $1 )) $(( WY + $2 ))
}

screen_tab_centre() {
  local point x y
  point=$(ipc tabCentre "$1") || return 1
  [[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || return 1
  read -r x y <<< "$point"
  (( x > 0 && y > 0 && x < WW && y < WH )) || return 1
  printf '%s %s\n' "$((WX + x))" "$((WY + y))"
}

native_tab() {
  local index="$1" point x y
  expect_ipc listInFlight false
  point=$(screen_tab_centre "$index") || die "tab $index has no visible native target"
  read -r x y <<< "$point"
  warp "$x" "$y"
  press
  release
  expect_ipc tabIndex "$index"
  expect_ipc listInFlight false
}

floor_refused() {
  local reader value status
  printf 'DRAG_FLOOR_REFUSED stage=%q window_snapshot=%q area=%q last_row=%q total=%q\n' \
    "$1" "$WX $WY $WW $WH" "$2" "$3" "$4"
  for reader in path viewMode listInFlight viewContentY dualState; do
    status=0
    value=$(ipc "$reader") || status=$?
    printf 'DRAG_FLOOR_STATE reader=%s status=%s value=%q\n' "$reader" "$status" "$value"
  done
  return 1
} >&2

# The active listing's empty tail, including Columns' narrower floor, measured before any release.
floor_centre() {
  local x y width height rx ry rw rh bottom area="" last="" total=""
  area=$(ipc listAreaRect) || { floor_refused "listing observer failed" "$area" "$last" "$total"; return 1; }
  read -r x y width height <<< "$area"
  [[ "$x $y $width $height" =~ ^[0-9]+(\ [0-9]+){3}$ ]] \
    || { floor_refused "invalid listing rectangle" "$area" "$last" "$total"; return 1; }
  (( width > 0 && height > 0 && x + width <= WW && y + height <= WH )) \
    || { floor_refused "listing outside window" "$area" "$last" "$total"; return 1; }
  bottom=$y
  total=$(ipc total) || { floor_refused "total observer failed" "$area" "$last" "$total"; return 1; }
  [[ "$total" =~ ^[0-9]+$ ]] || { floor_refused "invalid total" "$area" "$last" "$total"; return 1; }
  if (( total > 0 )); then
    last=$(ipc rowRect "$((total - 1))") || { floor_refused "row observer failed" "$area" "$last" "$total"; return 1; }
    read -r rx ry rw rh <<< "$last"
    [[ "$rx $ry $rw $rh" =~ ^[0-9]+(\ [0-9]+){3}$ ]] \
      || { floor_refused "invalid row rectangle" "$area" "$last" "$total"; return 1; }
    (( rw > 0 && rh > 0 && rx >= x && ry >= y && rx + rw <= x + width )) \
      || { floor_refused "row outside listing" "$area" "$last" "$total"; return 1; }
    bottom=$((ry + rh))
    x=$rx; width=$rw
  fi
  (( y + height - bottom > 2 * pointer_tolerance )) \
    || { floor_refused "insufficient empty floor" "$area" "$last" "$total"; return 1; }
  printf '%s %s\n' "$((WX + x + width / 2))" "$((WY + (bottom + y + height) / 2))"
}

owned_path() {
  local target
  [[ -n "$1" && "$1" == /* ]] || die "file operation path is not absolute"
  target=$(realpath -m -- "$1") || die "file operation path could not be resolved"
  [[ "$target" == "$SB/"* && -f "$SB/$SANDBOX_MARKER" ]] && return
  [[ -n "${XDEV:-}" && "$target" == "$XDEV/"* && -f "$XDEV/$SANDBOX_MARKER" ]] && return
  die "file operation path escaped this run: $target"
}

echo "== fixture $SB, instance $MYID, window at $WX,$WY size $WW,$WH, $(ipc total) rows =="

# wait_for <path> <present|absent>
wait_for() {
  local i
  for i in $(seq 1 40); do
    if [ "$2" = present ] && [ -e "$1" ]; then return 0; fi
    if [ "$2" = absent ] && [ ! -e "$1" ]; then return 0; fi
    sleep 0.25
  done
  return 1
}

# ---------------------------------------------------------------- R2
echo
echo "== R2: the drop lands where the pointer is, not one frame stale =="
# ui/List.qml positions the ghost by assignment and never by a binding, because Drag moves are posted
# and Drag.drop() flushes the pending one first. A stale ghost drops into a folder the drag merely
# crossed, so this drag crosses aaa deliberately and finishes on bbb.
set -- $(screen_centre r2.txt); sx=$1; sy=$2
set -- $(screen_centre aaa);    ax=$1; ay=$2
set -- $(screen_centre bbb);    bx=$1; by=$2
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$ax" "$ay"; sleep 0.4
glide_to "$bx" "$by"; sleep 0.5
release; sleep 0.4
wait_for "$HOMEDIR/bbb/r2.txt" present
check "the file lands in the folder the drag ended on" \
      "$([ -e "$HOMEDIR/bbb/r2.txt" ] && echo bbb || echo missing)" "bbb"
check "and not in the folder it merely crossed" \
      "$([ -e "$HOMEDIR/aaa/r2.txt" ] && echo "aaa STALE" || echo clean)" "clean"
check "a plain drag is a move, so the source is gone" \
      "$([ -e "$HOMEDIR/r2.txt" ] && echo still-there || echo moved)" "moved"

# ---------------------------------------------------------------- R3
echo
echo "== R3: ctrl decides copy versus move, and the lift is where it is read =="
# The modifier used to ride drag.proposedAction, which Qt recomputes from the live keyboard, so ctrl
# pressed after the final motion still reached the drop. It cannot any more: the drag advertises
# Qt.CopyAction alone so Chromium stops reporting dropEffect move, and Qt clamps a DragEvent's
# proposedAction to what the source advertised. Measured on Qt 6.11.2 from the DropArea itself, the
# receiver read proposedAction 2 of supported 3 under copy|move and 1 of 1 under copy alone, and
# Copy|Link reads 1 of 5, so no pair of actions both discriminates ctrl and keeps the copy promise.
# ui/js/Drag.js's own row marker carries it instead, baked when the DragHandler activates, so ctrl
# is held from before the press here and a ctrl pressed mid-drag now leaves the drag a move.
set -- $(screen_centre r3.txt); sx=$1; sy=$2
set -- $(screen_centre aaa);    ax=$1; ay=$2
warp "$sx" "$sy"; sleep 0.4
ctrl_down; sleep 0.3
press; sleep 0.3
glide_to "$ax" "$ay"; sleep 0.6
release; sleep 0.3
ctrl_up; sleep 0.4
wait_for "$HOMEDIR/aaa/r3.txt" present
check "ctrl held from the lift makes it a copy" \
      "$([ -e "$HOMEDIR/aaa/r3.txt" ] && echo copied || echo missing)" "copied"
check "and the source survives, which is what copy means" \
      "$([ -e "$HOMEDIR/r3.txt" ] && echo kept || echo GONE)" "kept"

# ---------------------------------------------------------------- R4
echo
echo "== R4: the status line names the folder under the pointer =="
# sayDrag looks the row up directly rather than through a bound property, because a binding on
# dropIndex is not refreshed yet inside onDropIndexChanged and the line read "to a folder" over a
# folder whose frame was already up.
set -- $(screen_centre r4.txt); sx=$1; sy=$2
set -- $(screen_centre bbb);    bx=$1; by=$2
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$bx" "$by"; sleep 0.8
MID=$(ipc stickyMessage)
release; sleep 0.6
check "the line names the folder under the pointer" "$MID" "Move 1 item to bbb · ctrl at lift copies"

# ---------------------------------------------------------------- R1
echo
echo "== R1: only a release over a valid folder may transfer =="
# ui/List.qml reads the grab transition and not active, because a release and a grab another item
# stole flip active the same way and only a release may drop. A synthetic pointer cannot steal a
# grab, so what is asserted here is the invariant that rule exists to protect, not the steal itself.
set -- $(screen_centre r1a.txt); sx=$1; sy=$2
set -- $(screen_centre r1b.txt); fx=$1; fy=$2
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$fx" "$fy"; sleep 0.5
release; sleep 0.8
check "a release over a file row transfers nothing" \
      "$([ -e "$HOMEDIR/r1a.txt" ] && echo kept || echo GONE)" "kept"
check "and the gesture leaves no status line behind" "$(ipc stickyMessage)" ""

set -- $(screen_centre r1b.txt); sx=$1; sy=$2
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
point=$(floor_centre) || die "R1 has no measured empty listing floor"
read -r fx fy <<< "$point"
glide_to "$fx" "$fy"; sleep 0.5
release; sleep 0.8
check "a release over empty space transfers nothing" \
      "$([ -e "$HOMEDIR/r1b.txt" ] && echo kept || echo GONE)" "kept"

# ---------------------------------------------------------------- R5
echo
echo "== R5: a drag resting on a tab selects it, and the drop lands on that tab's floor =="
# GM's ruling. The second tab is walked into bbb through the path bar, the first tab is shown again,
# then r1a.txt is lifted, rested on the second tab past ui/TabBar.qml's hoverSwitchMs, and released
# on the empty floor under the rows. The marker resolves the drop by path, because after the switch
# the row indices name bbb's own rows; a same-filesystem move is what a plain drag means.
export PATH="$HOME/.local/bin:$PATH"
r5_state before-t
expect_ipc tabCount 1
expect_ipc tabIndex 0
native_key t
r5_state after-t
expect_ipc tabCount 2
expect_ipc tabIndex 1
native_key :
expect_ipc pathBarOpen true
native_key "$HOMEDIR/bbb"
r5_state before-Return
native_key -k Return
r5_state after-Return
expect_ipc pathBarOpen false
expect_ipc path "$HOMEDIR/bbb"
expect_ipc listInFlight false
check "the second tab shows bbb" "$(ipc path)" "$HOMEDIR/bbb"
r5_state before-tab-click
native_tab 0
r5_state after-tab-click
expect_ipc tabIndex 0
expect_ipc path "$HOMEDIR"
expect_ipc listInFlight false
check "and the first tab is the home listing again" "$(ipc path)" "$HOMEDIR"
point=$(screen_centre r1a.txt) || die "R5 source r1a.txt is not visible"
read -r sx sy <<< "$point"
point=$(screen_tab_centre 1) || die "R5 destination tab is not visible"
read -r tx ty <<< "$point"
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
# The rest outlives the switch by a second: the pressed row's delegate is released by the re-list
# while the drag still runs, and the QDrag used to die with it (quickshell SIGSEGV, 2026-09-07).
glide_to "$tx" "$ty"; sleep 1.6
check "resting on the second tab selected it" "$(ipc tabIndex)" "1"
point=$(floor_centre) || die "R5 has no measured destination listing floor"
read -r fx fy <<< "$point"
glide_to "$fx" "$fy"; sleep 0.6
release; sleep 0.6
wait_for "$HOMEDIR/bbb/r1a.txt" present
check "the file landed on the second tab's floor" \
      "$([ -e "$HOMEDIR/bbb/r1a.txt" ] && echo bbb || echo missing)" "bbb"
check "as a move, so the source is gone" \
      "$([ -e "$HOMEDIR/r1a.txt" ] && echo still-there || echo moved)" "moved"
check "and the window survived the drop" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"

# ---------------------------------------------------------------- R6
echo
echo "== R6: a tab on the same directory re-lists under the drag, and the drop still names the lifted file =="
# R5 left bbb's tab current, so the home tab is selected first and a third tab is opened from it; that
# tab shows hidden files, where .local, aaa and bbb sort ahead of .r0hidden and every text row shifts.
# A drop resolved by the lifted index would move the row now sitting there; by path it moves r1b.txt.
native_tab 0
check "the home tab is current again" "$(ipc path)" "$HOMEDIR"
native_key t; sleep 0.8
native_key .
for i in $(seq 1 40); do [ "$(ipc showHidden)" = "true" ] && [ -n "$(rowidx .r0hidden)" ] && break; sleep 0.1; done
# .cache and .local are the window's own, so the dotfile's row is pinned as after every folder, not a number.
hidden_row=$(rowidx .r0hidden || echo none)
check "the third tab lists the hidden file" "$([ "$hidden_row" != none ] && echo listed || echo missing)" "listed"
check "and every folder sorts ahead of it" "$([ "$(rowidx aaa)" -lt "$hidden_row" ] && [ "$(rowidx bbb)" -lt "$hidden_row" ] && echo yes || echo no)" "yes"
check "so aaa is no longer row 0 on this tab" "$([ "$(rowidx aaa)" -gt 0 ] && echo shifted || echo same)" "shifted"
# Three tabs distinguish direction from wraparound; a two-tab toggle could pass with reversed keys.
expect_ipc keymapPreset default
expect_ipc tabCount 3
native_key -M ctrl -k Page_Down -m ctrl
expect_ipc tabIndex 0
expect_ipc path "$HOMEDIR"
expect_ipc listInFlight false
expect_ipc showHidden false
native_key -M ctrl -k Page_Down -m ctrl
expect_ipc tabIndex 1
expect_ipc path "$HOMEDIR/bbb"
expect_ipc listInFlight false
native_key -M ctrl -k Page_Up -m ctrl
expect_ipc tabIndex 0
expect_ipc path "$HOMEDIR"
expect_ipc listInFlight false
native_key -M ctrl -k Page_Up -m ctrl
expect_ipc tabIndex 2
expect_ipc path "$HOMEDIR"
expect_ipc listInFlight false
expect_ipc showHidden true
printf 'GUI_TAB_KEYS preset=default context=listing next=wrap,forward previous=backward,wrap retained-hidden=true\n'
# aaa's centre is read here, on the tab the drop lands on, under whatever dotdirs sort ahead of it.
point=$(screen_centre aaa) || die "R6 destination aaa is not visible"
read -r fx fy <<< "$point"
native_tab 0
check "and the home tab does not" "$(rowidx .r0hidden || echo none)" "none"
# Escape drops the restored selection; aaa already holds R3's copy, so the drop is judged by its delta.
native_key -k Escape; sleep 0.3
aaa_before=$(ls -A "$HOMEDIR/aaa" | tr '\n' ' ')
point=$(screen_centre r1b.txt) || die "R6 source r1b.txt is not visible"
read -r sx sy <<< "$point"
point=$(screen_tab_centre 2) || die "R6 destination tab is not visible"
read -r tx ty <<< "$point"
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$tx" "$ty"; sleep 1.2
check "resting on the same-directory tab selected it" "$(ipc tabIndex)" "2"
glide_to "$fx" "$fy"; sleep 0.6
release; sleep 0.6
wait_for "$HOMEDIR/aaa/r1b.txt" present
check "the lifted file landed in the folder under the drop" \
      "$([ -e "$HOMEDIR/aaa/r1b.txt" ] && echo aaa || echo missing)" "aaa"
check "and no other file moved" "$(ls -A "$HOMEDIR/aaa" | grep -vxF r1b.txt | tr '\n' ' ')" "$aaa_before"
check "and the window survived" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"
# ---------------------------------------------------------------- R7
echo
echo "== R7: a drop on a tab whose listing is still out is a copy, never a cross-device move =="
# ui/TabBar.qml reads the destination device as unknown while pane.listInFlight, because dirDev is then
# the directory the hover switch just left; with a tmpfs tab the stale device made the drop a move, and
# a move across devices copies and then deletes the source. The window is held open, not raced: the
# suite's own backend is stopped before the switch, so the listing it asks for cannot come back until
# the drop has been taken, and the backend is continued only then.
XDEV=$(mktemp -d /dev/shm/flea-drag-xdev-XXXXXX)
: > "$XDEV/$SANDBOX_MARKER"
mkdir -p "$XDEV/big/dest"
check "the tmpfs root is another filesystem than the fixture" \
      "$([ "$(stat -c %d "$XDEV")" != "$(stat -c %d "$HOMEDIR")" ] && echo other || echo same)" "other"
printf 'r7 payload\n' > "$HOMEDIR/r7.txt"
# R6 left the third tab current; it is walked into the tmpfs directory through the path bar, as R5 walked into bbb.
check "the third tab is current" "$(ipc tabIndex)" "2"
native_key :; sleep 0.3
native_key "$XDEV/big"; sleep 0.2
native_key -k Return
for i in $(seq 1 40); do [ "$(ipc path)" = "$XDEV/big" ] && [ "$(ipc listInFlight)" = false ] && break; sleep 0.25; done
check "the third tab lists the tmpfs directory" "$(ipc path)" "$XDEV/big"
native_tab 0
check "the home tab is current again" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 40); do rowidx r7.txt >/dev/null 2>&1 && break; sleep 0.25; done
# The one backend this suite owns: the instance's child running FLEA_BIN --backend, ui/Backend.qml's command.
BACKEND_PID=""
for p in $(pgrep -P "$MYPID"); do
  [ "$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)" = "$FLEA_BIN --backend " ] && BACKEND_PID=$p
done
check "the suite found the one backend it owns" "$([ -n "$BACKEND_PID" ] && echo found || echo none)" "found"
point=$(screen_centre r7.txt) || die "R7 source r7.txt is not visible"
read -r sx sy <<< "$point"
point=$(screen_tab_centre 2) || die "R7 destination tab is not visible"
read -r tx ty <<< "$point"
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
kill -STOP "$BACKEND_PID"
glide_to "$tx" "$ty"
for i in $(seq 1 40); do [ "$(ipc tabIndex)" = 2 ] && [ "$(ipc listInFlight)" = true ] && break; sleep 0.1; done
check "resting on the tmpfs tab selected it" "$(ipc tabIndex)" "2"
check "and its listing is out against the stopped backend" "$(ipc listInFlight)" "true"
release; sleep 0.5
check "the drop was taken with the listing still out" "$(ipc listInFlight)" "true"
check "and the backend was still stopped at that point" "$(cut -d' ' -f3 "/proc/$BACKEND_PID/stat")" "T"
kill -CONT "$BACKEND_PID"
wait_for "$XDEV/big/r7.txt" present
check "the file landed on the tmpfs tab" \
      "$([ -e "$XDEV/big/r7.txt" ] && echo landed || echo missing)" "landed"
check "byte for byte" "$(cmp -s "$HOMEDIR/r7.txt" "$XDEV/big/r7.txt" && echo same || echo differs)" "same"
check "as a copy, so the source survives" \
      "$([ -e "$HOMEDIR/r7.txt" ] && echo kept || echo GONE)" "kept"
check "and the window survived" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"

# ---------------------------------------------------------------- R8
echo
echo "== R8: the line over a folder on another filesystem says copy, and the drop is one =="
# ui/List.qml's verbAt reads the source device off the marker, stamped at the lift: after the hover
# switch the pane's own dirDev is the destination's, and read from there the line said move over a
# folder the drop would copy into. The same dragCopy drives the row's "copy here" badge.
printf 'r8 payload\n' > "$HOMEDIR/r8.txt"
native_tab 0
check "the home tab is current" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 40); do rowidx r8.txt >/dev/null 2>&1 && break; sleep 0.25; done
point=$(screen_centre r8.txt) || die "R8 source r8.txt is not visible"
read -r sx sy <<< "$point"
point=$(screen_tab_centre 2) || die "R8 destination tab is not visible"
read -r tx ty <<< "$point"
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$tx" "$ty"
for i in $(seq 1 40); do [ "$(ipc path)" = "$XDEV/big" ] && [ "$(ipc listInFlight)" = false ] && rowidx dest >/dev/null 2>&1 && break; sleep 0.1; done
check "resting on the tmpfs tab listed it in full" "$(ipc path)" "$XDEV/big"
point=$(screen_centre dest) || die "R8 destination folder is not visible"
read -r fx fy <<< "$point"
glide_to "$fx" "$fy"; sleep 0.6
check "the line over the folder says copy" "$(ipc stickyMessage)" "Copy 1 item to dest"
release; sleep 0.6
wait_for "$XDEV/big/dest/r8.txt" present
check "the file landed in that folder" \
      "$([ -e "$XDEV/big/dest/r8.txt" ] && echo landed || echo missing)" "landed"
check "byte for byte" "$(cmp -s "$HOMEDIR/r8.txt" "$XDEV/big/dest/r8.txt" && echo same || echo differs)" "same"
check "as a copy, so the source survives" \
      "$([ -e "$HOMEDIR/r8.txt" ] && echo kept || echo GONE)" "kept"
check "and the window survived" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"
echo
[ "$fail" = 0 ] || exit 1

# ---------------------------------------------------------------- shared source/target ownership
dual_destination_side=""
expect_feedback() {
  local owner="$1" line="$2" state attempt
  for ((attempt=1; attempt<=40; attempt++)); do
    state=$(ipc statusActivityState) || die "drag activity observer failed"
    if jq -e --arg owner "$owner" --arg line "$line" '
        [.activities[] | select(.running | not) | {ownerPath,text}] ==
        (if $line == "" then [] else [{ownerPath:$owner,text:$line}] end)' <<< "$state" >/dev/null; then
      ok "drag ownership owner=[$owner] line=[$line] state=$state"
      return
    fi
    sleep 0.25
  done
  if [[ -n "${dual_destination_side:-}" ]]; then dual_drag_diagnostic; fi
  die "drag ownership owner=[$owner] line=[$line], observed $state"
}

navigate() {
  owned_path "$1"
  native_key -M ctrl -k l -m ctrl "$1" -k Return
  expect_ipc path "$1"
  expect_ipc listInFlight false
}

choose_view() {
  local mode="$1" chord
  case "$mode" in list) chord=1 ;; columns) chord=2 ;; grid) chord=3 ;; *) die "unknown drag view: $mode" ;; esac
  native_key -M ctrl -k "$chord" -m ctrl
  expect_ipc viewMode "$mode"
  expect_ipc listInFlight false
}

make_pair() {
  local directory="$1" name="$2" part
  owned_path "$directory"
  mkdir -p "$directory" || die "could not create pair fixture: $directory"
  for part in a b; do
    owned_path "$directory/$name-$part.txt"
    printf '%s-%s payload\n' "$name" "$part" > "$directory/$name-$part.txt" || die "could not write pair fixture"
  done
}

mark_pair() {
  local name="$1" first second point px py expected
  first=$(rowidx "$name-a.txt") || die "first pair identity is not listed"
  second=$(rowidx "$name-b.txt") || die "second pair identity is not listed"
  point=$(screen_centre "$name-a.txt") || die "first pair identity is not visible"
  read -r px py <<< "$point"
  omarchy-drive click "$px" "$py" left >/dev/null || die "native plain selection failed"
  expect_ipc selectedIndices "$first"
  point=$(screen_centre "$name-b.txt") || die "second pair identity is not visible"
  read -r px py <<< "$point"
  omarchy-drive click "$px" "$py" left --mods ctrl >/dev/null || die "native additive selection failed"
  expected=$(jq -nr --argjson first "$first" --argjson second "$second" '[$first,$second] | sort | map(tostring) | join(",")')
  expect_ipc selectedIndices "$expected"
  expect_ipc selectionCount 2
}

begin_pair() {
  local name="$1" verb="$2" point px py
  mark_pair "$name"
  point=$(screen_centre "$name-a.txt") || die "marked drag source is not visible"
  read -r px py <<< "$point"
  glide_to "$px" "$py"
  [[ "$verb" != Copy ]] || ctrl_down
  press
}

target_points() {
  local point x y
  point=$(screen_centre folder) || die "target folder is not visible"
  read -r folder_x folder_y <<< "$point"
  point=$(floor_centre) || die "target has no measured empty listing floor"
  read -r floor_x floor_y <<< "$point"
  point=$(ipc chromeButtonCentre sliders) || die "neutral chrome target is unavailable"
  [[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || die "neutral chrome target has no valid geometry"
  read -r x y <<< "$point"
  (( x > 0 && x < WW && y > 0 && y < WH )) || die "neutral chrome target is outside the owned window"
  neutral_x=$((WX + x)); neutral_y=$((WY + y))
}

dual_target_points() {
  local phase="$1" geometry footer point
  owned_path "$destination"
  geometry=$(ipc dragPaneGeometry "$dual_destination_side" 0) || die "dual drag geometry observer failed"
  footer=$(ipc statusFooterState) || die "dual drag footer observer failed"
  if [[ "$phase" == saved-before ]]; then dual_before="$geometry"; else dual_after="$geometry"; fi
  printf 'DRAG_DUAL_GEOMETRY phase=%s state=%s\n' "$phase" "$geometry"
  point=$(python3 - "$geometry" "$destination" "$dual_destination_side" "$phase" "$WX" "$WY" "$WW" "$WH" "$pointer_tolerance" "$footer" <<'PY'
import json, re, sys

state = json.loads(sys.argv[1])
destination, side, phase = sys.argv[2:5]
wx, wy, width, height, tolerance = map(int, sys.argv[5:10])
footer = json.loads(sys.argv[10])
def require(condition, message):
    if not condition:
        raise SystemExit("dual drag geometry: " + message)

require(state.get("side") == int(side) and state.get("active") is True and state.get("path") == destination,
        "destination pane identity changed")
require(state.get("focused") is (phase == "saved-before") and state.get("view") == "list" and state.get("loading") is False,
        "destination focus, view or listing readiness changed")
require(state.get("total") == 3, "fixture no longer contains its folder and two files")
folder, last = state.get("folder", {}), state.get("last", {})
require(folder.get("index") == 0 and folder.get("name") == "folder" and folder.get("directory") is True,
        "first fixture row is not the destination folder")
require(last.get("index") == 2 and bool(last.get("name")), "last fixture row identity is unavailable")
def rectangle(value):
    # Native rectOf output: "1365 108 1172 37", rounded at its actual edges.
    require(isinstance(value, str) and re.fullmatch(r"[0-9]+(?: [0-9]+){3}", value), "invalid native rectangle")
    x, y, w, h = map(int, value.split())
    require(w > 0 and h > 0 and x + w <= width and y + h <= height, "rectangle is outside the owned window")
    return x, y, w, h

ax, ay, aw, ah = rectangle(state.get("area"))
fx, fy, fw, fh = rectangle(folder.get("rect"))
lx, ly, lw, lh = rectangle(last.get("rect"))
for x, y, w, h in [(fx, fy, fw, fh), (lx, ly, lw, lh)]:
    require(x >= ax and y >= ay and x + w <= ax + aw and y + h <= ay + ah, "row is outside the destination listing")
bottom = ly + lh
require(ay + ah - bottom > 2 * tolerance, "destination has insufficient empty floor")
# Leave vertically into the informational footer; a diagonal to Sliders can cross the other pane's folder.
sx, sy, sw, sh = rectangle(footer.get("frame"))
outside_x, outside_y = lx + lw // 2, sy + sh // 2
require(ax + tolerance < outside_x < ax + aw - tolerance, "outside route is too close to a pane boundary")
require(sy >= ay + ah, "footer overlaps the destination listing")
require(sx + tolerance < outside_x < sx + sw - tolerance and sy + tolerance < outside_y < sy + sh - tolerance,
        "footer cannot contain the outside waypoint and pointer tolerance")
print(wx + fx + (fw + 1) // 2, wy + fy + (fh + 1) // 2, wx + outside_x, wy + (bottom + ay + ah) // 2,
      wx + outside_x, wy + outside_y)
PY
  ) || { dual_drag_diagnostic; die "dual target geometry refused; state=$geometry"; }
  read -r folder_x folder_y floor_x floor_y neutral_x neutral_y <<< "$point"
  printf 'DRAG_DUAL_POINTS phase=%s folder=%s,%s floor=%s,%s outside=%s,%s footer=%s\n' \
    "$phase" "$folder_x" "$folder_y" "$floor_x" "$floor_y" "$neutral_x" "$neutral_y" "$footer"
}

dual_drag_diagnostic() {
  local evidence window address pointer current
  [[ "$(myid)" == "$MYID $MYPID" ]] || { bad "dual drag diagnostic lost its owned instance"; return 1; }
  evidence=$(mktemp -d /tmp/flea-drag-failure.XXXXXX) || return 1
  [[ "$evidence" == /tmp/flea-drag-failure.* && -d "$evidence" && ! -L "$evidence" ]] || return 1
  printf 'dual drag failure evidence\n' > "$evidence/.flea-test-sandbox"
  printf '%s\n' "${dual_before:-}" > "$evidence/saved-before.json"
  printf '%s\n' "${dual_after:-}" > "$evidence/fresh-after.json"
  pointer=$(hyprctl cursorpos -j) || { bad "dual drag diagnostic cursor read failed"; return 1; }
  current=$(ipc dragPaneGeometry "$dual_destination_side" 0) || return 1
  window=$(hyprctl clients -j | jq -ce --argjson pid "$MYPID" '[.[] | select(.pid == $pid)] | if length == 1 then .[0] else error("owned window missing or ambiguous") end') || return 1
  address=$(jq -er .address <<< "$window") || return 1
  printf '%s\n' "$pointer" > "$evidence/pointer.json"
  printf '%s\n' "$current" > "$evidence/current.json"
  printf '%s\n' "$window" > "$evidence/window.json"
  printf 'DRAG_DUAL_MISMATCH evidence=%s pointer=%s current=%s\n' "$evidence" "$pointer" "$current"
  [[ -f "$evidence/.flea-test-sandbox" && ! -e "$evidence/window.png" && ! -L "$evidence/window.png" ]] || return 1
  omarchy-drive shot "$evidence/window.png" "$address" || { bad "dual drag failure screenshot failed"; return 1; }
  [[ -s "$evidence/window.png" ]] || { bad "dual drag failure screenshot is missing"; return 1; }
}

visit_targets() {
  local destination="$1" verb="$2" suffix=""
  [[ "$verb" != Move ]] || suffix=' · ctrl at lift copies'
  glide_to "$folder_x" "$folder_y"
  expect_feedback "$destination" "$verb 2 items to folder$suffix"
  glide_to "$floor_x" "$floor_y"
  expect_feedback "$destination" "$verb 2 items to $destination$suffix"
  glide_to "$neutral_x" "$neutral_y"
  expect_feedback "$destination" "$verb 2 items to a folder$suffix"
  glide_to "$folder_x" "$folder_y"
  expect_feedback "$destination" "$verb 2 items to folder$suffix"
}

pair_result() {
  local source="$1" destination="$2" name="$3" action="$4" part file target
  if [[ "$action" != cancel ]]; then
    for part in a b; do
      owned_path "$destination/$name-$part.txt"
      wait_for "$destination/$name-$part.txt" present || die "committed pair did not reach $destination"
    done
    if [[ "$action" == Copy ]]; then
      expect_ipc lastMessage 'Copied 2 items · z undoes'
    else
      expect_ipc lastMessage 'Moved 2 items · z undoes'
    fi
    expect_ipc stickyMessage ""
    expect_ipc statusError false
  fi
  for part in a b; do
    file="$source/$name-$part.txt"; target="$destination/$name-$part.txt"
    owned_path "$file"; owned_path "$target"
    if [[ "$action" == cancel ]]; then
      check "$name-$part cancellation preserves original bytes" \
        "$(printf '%s-%s payload\n' "$name" "$part" | cmp -s - "$file" && echo same || echo CHANGED)" same
      check "$name-$part cancellation creates no destination" "$([[ ! -e "$target" ]] && echo absent || echo PRESENT)" absent
    else
      check "$name-$part committed bytes" \
        "$(printf '%s-%s payload\n' "$name" "$part" | cmp -s - "$target" && echo same || echo CHANGED)" same
      if [[ "$action" == Copy ]]; then
        check "$name-$part copy retains exact source bytes" "$(cmp -s "$file" "$target" && echo same || echo CHANGED)" same
      else
        check "$name-$part move removes only its source" "$([[ ! -e "$file" ]] && echo moved || echo STILL_PRESENT)" moved
      fi
    fi
  done
}

cross_view_pair() {
  local name="$1" source_mode="$2" target_mode="$3" landing="$4" source destination phase point tx ty drop
  local folder_x folder_y floor_x floor_y neutral_x neutral_y
  source="$HOMEDIR/aaa/$name"; destination="$HOMEDIR/bbb/$name"
  make_pair "$source" "$name"
  owned_path "$destination/folder"
  mkdir -p "$destination/folder" || die "could not create cross-view target"
  native_tab 0; navigate "$source"; choose_view "$source_mode"
  native_tab 1; navigate "$destination"; choose_view "$target_mode"
  for phase in cancel commit; do
    native_tab 0
    expect_ipc path "$source"; expect_ipc viewMode "$source_mode"; expect_ipc listInFlight false
    point=$(ipc tabCentre 1) || die "destination tab is unavailable"
    [[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || die "destination tab has no valid geometry"
    read -r tx ty <<< "$point"; tx=$((WX + tx)); ty=$((WY + ty))
    begin_pair "$name" Copy
    glide_to "$tx" "$ty"
    expect_ipc tabIndex 1
    expect_ipc path "$destination"; expect_ipc viewMode "$target_mode"; expect_ipc listInFlight false
    target_points
    visit_targets "$destination" Copy
    if [[ "$phase" == cancel ]]; then
      native_key -k Escape
      expect_feedback "" ""
      release; ctrl_up
      expect_feedback "" ""
      pair_result "$source" "$destination/folder" "$name" cancel
      pair_result "$source" "$destination" "$name" cancel
    else
      drop="$destination/folder"
      if [[ "$landing" == floor ]]; then
        glide_to "$floor_x" "$floor_y"
        expect_feedback "$destination" "Copy 2 items to $destination"
        drop="$destination"
      fi
      owned_path "$source/$name-a.txt"; owned_path "$source/$name-b.txt"; owned_path "$drop"
      release; ctrl_up
      pair_result "$source" "$drop" "$name" Copy
      expect_feedback "" ""
    fi
  done
}

echo "== R9: List to Grid and Grid to active Columns keep one target-owned two-file line =="
cross_view_pair feedback-list-grid list grid floor
cross_view_pair feedback-grid-columns grid columns folder

echo "== R10: dual-pane copies and reverse moves keep destination feedback ownership =="
left="$HOMEDIR/aaa/feedback-dual-left"; right="$HOMEDIR/bbb/feedback-dual-right"
make_pair "$left" feedback-left
make_pair "$right" feedback-right
mkdir "$left/folder" "$right/folder" || die "could not create dual target folders"
native_tab 0
choose_view list
point=$(ipc chromeButtonCentre dual) || die "dual control is unavailable"
[[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || die "dual control has no valid geometry"
read -r px py <<< "$point"
omarchy-drive click "$((WX + px))" "$((WY + py))" left >/dev/null || die "dual control activation failed"
for direction in left right; do
  side=$(ipc dualState | jq -er '.focused') || die "dual focus is unavailable"
  [[ "$side" == 0 ]] || native_key -k Tab
  navigate "$left"
  native_key -k Tab
  navigate "$right"
  state=$(ipc dualState) || die "dual state is unavailable"
  jq -e --arg left "$left" --arg right "$right" '.active and .panes[0].path == $left and .panes[1].path == $right
      and all(.panes[]; .loading | not)' <<< "$state" >/dev/null || die "dual fixtures lost their independent listing identity: $state"
  if [[ "$direction" == left ]]; then
    source="$left"; destination="$right"; name=feedback-left; verb=Copy
    dual_destination_side=1
    dual_target_points saved-before
    native_key -k Tab
  else
    source="$right"; destination="$left"; name=feedback-right; verb=Move
    dual_destination_side=0
    native_key -k Tab
    dual_target_points saved-before
    native_key -k Tab
  fi
  expect_ipc path "$source"
  begin_pair "$name" "$verb"
  dual_target_points fresh-after
  visit_targets "$destination" "$verb"
  owned_path "$source/$name-a.txt"; owned_path "$source/$name-b.txt"; owned_path "$destination/folder"
  release
  [[ "$verb" != Copy ]] || ctrl_up
  pair_result "$source" "$destination/folder" "$name" "$verb"
  expect_feedback "" ""
done

printf 'DRAG_SHARED routes=List-Grid,Grid-activeColumns,dual-left-right,dual-right-left real_relative_input=ok index_only=not_exercised transfer_preemption=not_exercised\n'
echo "$((pass + fail)) checks, $fail failed"
[ "$fail" = 0 ] || exit 1
