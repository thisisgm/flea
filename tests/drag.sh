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
  # R7 stops the backend it owns; a stopped process ignores TERM until it is continued.
  [ -n "${BACKEND_PID:-}" ] && kill -CONT "$BACKEND_PID" 2>/dev/null
  [ -n "${FLEA_PID:-}" ] && kill -- -"$FLEA_PID" 2>/dev/null
  [ -n "${FLEA_PID:-}" ] && kill "$FLEA_PID" 2>/dev/null
  sleep 0.5
  sandbox_remove "$SB" 2>/dev/null
  # R7's tmpfs root: its own mktemp, its own marker, and the pattern checked again before the delete.
  case "${XDEV:-}" in /dev/shm/flea-drag-xdev-*) [ -f "$XDEV/$SANDBOX_MARKER" ] && rm -rf -- "$XDEV" ;; esac
}
trap cleanup EXIT

# ---------------------------------------------------------------- fixture
sandbox_make "$SB"
mkdir -p "$HOMEDIR/.local/state/omarchy" "$HOMEDIR/aaa" "$HOMEDIR/bbb"
ln -sfn "$HOME/.local/state/omarchy/current" "$HOMEDIR/.local/state/omarchy/current"
for f in r1a r1b r2 r3 r4; do printf '%s payload\n' "$f" > "$HOMEDIR/$f.txt"; done
# R6 shows hidden files in a second tab on the same directory, so every index below this one shifts.
printf 'hidden\n' > "$HOMEDIR/.r0hidden"

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
glide_to $(( sx + 40 )) $(( WY + 700 )); sleep 0.5
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
omarchy-drive key --window flea t >/dev/null 2>&1; sleep 0.5
omarchy-drive key --window flea : >/dev/null 2>&1; sleep 0.3
omarchy-drive key --window flea "$HOMEDIR/bbb" >/dev/null 2>&1; sleep 0.2
omarchy-drive key --window flea -k Return >/dev/null 2>&1; sleep 0.6
check "the second tab shows bbb" "$(ipc path)" "$HOMEDIR/bbb"
omarchy-drive key --window flea 1 >/dev/null 2>&1; sleep 0.6
check "and the first tab is the home listing again" "$(ipc path)" "$HOMEDIR"
set -- $(screen_centre r1a.txt); sx=$1; sy=$2
set -- $(ipc tabCentre 1); tx=$(( WX + $1 )); ty=$(( WY + $2 ))
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
# The rest outlives the switch by a second: the pressed row's delegate is released by the re-list
# while the drag still runs, and the QDrag used to die with it (quickshell SIGSEGV, 2026-09-07).
glide_to "$tx" "$ty"; sleep 1.6
check "resting on the second tab selected it" "$(ipc tabIndex)" "1"
glide_to "$tx" $(( WY + 700 )); sleep 0.6
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
omarchy-drive key --window flea 1 >/dev/null 2>&1; sleep 0.6
check "the home tab is current again" "$(ipc path)" "$HOMEDIR"
omarchy-drive key --window flea t >/dev/null 2>&1; sleep 0.8
omarchy-drive key --window flea . >/dev/null 2>&1
for i in $(seq 1 40); do [ "$(ipc showHidden)" = "true" ] && [ -n "$(rowidx .r0hidden)" ] && break; sleep 0.1; done
# .cache and .local are the window's own, so the dotfile's row is pinned as after every folder, not a number.
hidden_row=$(rowidx .r0hidden || echo none)
check "the third tab lists the hidden file" "$([ "$hidden_row" != none ] && echo listed || echo missing)" "listed"
check "and every folder sorts ahead of it" "$([ "$(rowidx aaa)" -lt "$hidden_row" ] && [ "$(rowidx bbb)" -lt "$hidden_row" ] && echo yes || echo no)" "yes"
check "so aaa is no longer row 0 on this tab" "$([ "$(rowidx aaa)" -gt 0 ] && echo shifted || echo same)" "shifted"
# aaa's centre is read here, on the tab the drop lands on, under whatever dotdirs sort ahead of it.
set -- $(screen_centre aaa); fx=$1; fy=$2
omarchy-drive key --window flea 1 >/dev/null 2>&1; sleep 0.8
check "and the home tab does not" "$(rowidx .r0hidden || echo none)" "none"
# Escape drops the restored selection; aaa already holds R3's copy, so the drop is judged by its delta.
omarchy-drive key --window flea -k Escape >/dev/null 2>&1; sleep 0.3
aaa_before=$(ls -A "$HOMEDIR/aaa" | tr '\n' ' ')
set -- $(screen_centre r1b.txt); sx=$1; sy=$2
set -- $(ipc tabCentre 2); tx=$(( WX + $1 )); ty=$(( WY + $2 ))
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
omarchy-drive key --window flea : >/dev/null 2>&1; sleep 0.3
omarchy-drive key --window flea "$XDEV/big" >/dev/null 2>&1; sleep 0.2
omarchy-drive key --window flea -k Return >/dev/null 2>&1
for i in $(seq 1 40); do [ "$(ipc path)" = "$XDEV/big" ] && [ "$(ipc listInFlight)" = false ] && break; sleep 0.25; done
check "the third tab lists the tmpfs directory" "$(ipc path)" "$XDEV/big"
omarchy-drive key --window flea 1 >/dev/null 2>&1; sleep 0.8
check "the home tab is current again" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 40); do rowidx r7.txt >/dev/null 2>&1 && break; sleep 0.25; done
# The one backend this suite owns: the instance's child running FLEA_BIN --backend, ui/Backend.qml's command.
BACKEND_PID=""
for p in $(pgrep -P "$MYPID"); do
  [ "$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)" = "$FLEA_BIN --backend " ] && BACKEND_PID=$p
done
check "the suite found the one backend it owns" "$([ -n "$BACKEND_PID" ] && echo found || echo none)" "found"
set -- $(screen_centre r7.txt); sx=$1; sy=$2
set -- $(ipc tabCentre 2); tx=$(( WX + $1 )); ty=$(( WY + $2 ))
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
omarchy-drive key --window flea 1 >/dev/null 2>&1; sleep 0.8
check "the home tab is current" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 40); do rowidx r8.txt >/dev/null 2>&1 && break; sleep 0.25; done
set -- $(screen_centre r8.txt); sx=$1; sy=$2
set -- $(ipc tabCentre 2); tx=$(( WX + $1 )); ty=$(( WY + $2 ))
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$tx" "$ty"
for i in $(seq 1 40); do [ "$(ipc path)" = "$XDEV/big" ] && [ "$(ipc listInFlight)" = false ] && rowidx dest >/dev/null 2>&1 && break; sleep 0.1; done
check "resting on the tmpfs tab listed it in full" "$(ipc path)" "$XDEV/big"
set -- $(screen_centre dest); fx=$1; fy=$2
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
echo "$((pass + fail)) checks, $fail failed"
[ "$fail" = 0 ] || exit 1
