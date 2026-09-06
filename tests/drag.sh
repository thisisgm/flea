#!/usr/bin/env bash
# Characterises the internal drag exactly as it behaves today, before any rewrite touches it. Each
# check is one of the four races ui/List.qml's own comments record, turned into a test rather than a
# note, so a rewrite that re-opens one fails here instead of being found by hand.
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
. "$repo/tools/flea-sandbox-guard"

SB=$FIXTURE_ROOT/flea-drag-char-$$
HOMEDIR=$SB/home
pass=0
fail=0

export XDG_RUNTIME_DIR=/run/user/$(id -u)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1}
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t "$XDG_RUNTIME_DIR"/hypr/ | head -1)
export YDOTOOL_SOCKET=$XDG_RUNTIME_DIR/.ydotool_socket

ok()   { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad()  { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }
note() { printf '     %s\n' "$*"; }
check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1"; note "expected [$3]"; note "got      [$2]"; fi; }

cleanup() {
  [ -n "${FLEA_PID:-}" ] && kill -- -"$FLEA_PID" 2>/dev/null
  [ -n "${FLEA_PID:-}" ] && kill "$FLEA_PID" 2>/dev/null
  sleep 0.5
  sandbox_remove "$SB" 2>/dev/null
}
trap cleanup EXIT

# ---------------------------------------------------------------- fixture
sandbox_make "$SB"
mkdir -p "$HOMEDIR/.local/state/omarchy" "$HOMEDIR/aaa" "$HOMEDIR/bbb"
ln -sfn "$HOME/.local/state/omarchy/current" "$HOMEDIR/.local/state/omarchy/current"
for f in r1a r1b r2 r3 r4; do printf '%s payload\n' "$f" > "$HOMEDIR/$f.txt"; done

# ---------------------------------------------------------------- pointer
warp() { hyprctl dispatch "hl.dsp.cursor.move({x = $1, y = $2})" >/dev/null; }
move_rel() { ydotool mousemove -x "$1" -y "$2" >/dev/null 2>&1; }
press()   { ydotool click 0x40 >/dev/null 2>&1; }
release() { ydotool click 0x80 >/dev/null 2>&1; }
# evdev KEY_LEFTCTRL. Held through ydotool because a compositor keybind must not swallow it.
ctrl_down() { ydotool key 29:1 >/dev/null 2>&1; }
ctrl_up()   { ydotool key 29:0 >/dev/null 2>&1; }

# glide_to x y : converge on an absolute target with real frame-carrying motion. libinput accelerates
# relative motion about 2x here, so each step is half the remaining distance and re-read, never trusted.
glide_to() {
  local tx=$1 ty=$2 i cx cy dx dy
  for i in $(seq 1 16); do
    set -- $(hyprctl cursorpos | tr -d ",")
    cx=$1; cy=$2
    dx=$(( tx - cx )); dy=$(( ty - cy ))
    if [ "${dx#-}" -le 4 ] && [ "${dy#-}" -le 4 ]; then return 0; fi
    move_rel $(( dx / 2 )) $(( dy / 2 ))
    sleep 0.05
  done
}

# ---------------------------------------------------------------- the app
# The instance id and the process id together: the id addresses IPC, the pid finds this suite's own
# window. Matching the window by class alone aborted three runs beside another lane's Flea, which is
# right to refuse but needlessly blind, because the pid is already in hand.
myid() {
  qs list --all --json 2>/dev/null | python3 -c '
import json, sys
hits = [i for i in json.load(sys.stdin) if i["config_path"] == sys.argv[1]]
if len(hits) != 1:
    sys.exit(1)
print("%s %s" % (hits[0]["id"], hits[0]["pid"]))
' "$repo/ui/shell.qml"
}
ipc() { qs ipc -i "$MYID" call flea "$@" 2>&1; }

# The renderer is stated because src/gui.rs owns that choice and a direct qs launch never runs it.
QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" HOME="$HOMEDIR" setsid qs -p "$repo/ui" >"$SB/flea.log" 2>&1 &
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
print(hits[0]["at"][0], hits[0]["at"][1])
' "$MYPID") || { echo "no window belonging to this suite (pid $MYPID)"; exit 1; }
set -- $WIN; WX=$1; WY=$2

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
  set -- $c
  echo $(( WX + $1 )) $(( WY + $2 ))
}

echo "== fixture $SB, instance $MYID, window at $WX,$WY, $(ipc total) rows =="

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
echo "== R3: ctrl decides copy versus move, and it is read off the keyboard =="
# The DragHandler updates its centroid on press and motion only, never on the release, so a ctrl
# pressed after the last motion is invisible to it and only Keys.onPressed can carry it. Pressing
# ctrl after the final motion is exactly what makes this the keyboard path rather than the centroid.
set -- $(screen_centre r3.txt); sx=$1; sy=$2
set -- $(screen_centre aaa);    ax=$1; ay=$2
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$ax" "$ay"; sleep 0.6
ctrl_down; sleep 0.5
release; sleep 0.3
ctrl_up; sleep 0.4
wait_for "$HOMEDIR/aaa/r3.txt" present
check "ctrl pressed after the last motion still makes it a copy" \
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
check "the line names the folder under the pointer" "$MID" "Move 1 item to bbb · ctrl copies"

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
glide_to $(( sx + 40 )) $(( WY + 700 )); sleep 0.5
release; sleep 0.8
check "a release over empty space transfers nothing" \
      "$([ -e "$HOMEDIR/r1b.txt" ] && echo kept || echo GONE)" "kept"

echo
echo "$((pass + fail)) checks, $fail failed"
[ "$fail" = 0 ] || exit 1
