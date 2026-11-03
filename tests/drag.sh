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

drag_case=${1:-all}
if [[ "$drag_case" == headless ]]; then
  exec /bin/bash "$repo/tests/raildrop.sh"
fi
capture_root=""
case "$drag_case" in
  all)
    sandbox_root_ok
    mkdir -p "$SANDBOX_ROOT" || exit 1
    SB=$(mktemp -d "$SANDBOX_ROOT/flea-drag-char-XXXXXXXX") || exit 1
    : > "$SB/$SANDBOX_MARKER" || exit 1
    ;;
  raildrop)
    sandbox_root_ok
    mkdir -p "$SANDBOX_ROOT" || exit 1
    capture_root=$(mktemp -d "$SANDBOX_ROOT/native.XXXXXXXX") || exit 1
    printf 'raildrop native captures\n' > "$capture_root/$SANDBOX_MARKER" || exit 1
    FIXTURE_ROOT="$capture_root"
    SB="$capture_root/fixture"
    printf 'RAILDROP_EVIDENCE_ROOT=%s\n' "$capture_root"
    ;;
  pindrop)
    sandbox_root_ok
    mkdir -p "$SANDBOX_ROOT" || exit 1
    capture_root=$(mktemp -d "$SANDBOX_ROOT/native.XXXXXXXX") || exit 1
    printf 'pindrop native captures\n' > "$capture_root/$SANDBOX_MARKER" || exit 1
    FIXTURE_ROOT="$capture_root"
    SB="$capture_root/fixture"
    printf 'PINDROP_EVIDENCE_ROOT=%s\n' "$capture_root"
    ;;
  look235)
    look235_started=$SECONDS
    FIXTURE_ROOT="$repo/.superpowers/look235"
    sandbox_root_ok
    mkdir -p "$SANDBOX_ROOT" || exit 1
    capture_root=$(mktemp -d "$SANDBOX_ROOT/run.XXXXXXXX") || exit 1
    printf 'look235 native captures\n' > "$capture_root/$SANDBOX_MARKER" || exit 1
    FIXTURE_ROOT="$capture_root"
    SB="$capture_root/fixture"
    printf 'LOOK235_EVIDENCE_ROOT=%s\n' "$capture_root"
    ;;
  *) printf 'Usage: %s [all|headless|raildrop|pindrop|look235]\n' "$0" >&2; exit 2 ;;
esac
HOMEDIR=$SB/home
pass=0
fail=0
button_down=false
control_down=false
shift_held=false
RECV_PID=""
RECV_PIDS=()
declare -A RECV_REAPED=()
pointer_tolerance=4
hyprland_instance_lines=1
receiver_stderr_lines=5
outbound_data_device_lines=25
cleanup_stderr_lines=80
cleanup_drop_event_lines=40
# Whether the compositor delivered the drop and the source heard dnd_finished or a cancel.
cleanup_drop_events='wl_data_(device|source)#[0-9]+\.(drop|dnd_drop_performed|dnd_finished|cancelled)'

export XDG_RUNTIME_DIR=/run/user/$(id -u)
export WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-wayland-1}
export HYPRLAND_INSTANCE_SIGNATURE=$(ls -t "$XDG_RUNTIME_DIR"/hypr/ | head -n "$hyprland_instance_lines")
export YDOTOOL_SOCKET=$XDG_RUNTIME_DIR/.ydotool_socket

ok()   { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad()  { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }
note() { printf '     %s\n' "$*"; }
# A missing drop body prints what the receiver logged and the source's data-device traffic.
outbound_evidence() {
  grep -q 'body<<' "$RECV_LOG" && return 0
  note "receiver log: $(tr '\n' '|' < "$RECV_LOG")"
  note "receiver stderr: $(tail -n "$receiver_stderr_lines" "$1" 2>/dev/null | tr '\n' '|')"
  grep -E 'wl_data_(source|offer|device)' "$SB/flea.log" | tail -n "$outbound_data_device_lines" | while IFS= read -r line; do
    note "$line"
  done
}
check() { if [ "$2" = "$3" ]; then ok "$1"; else bad "$1"; note "expected [$3]"; note "got      [$2]"; fi; }
die() { bad "$*"; exit 1; }

. "$repo/tests/lib/hypr-dispatch.sh"

receiver_processes() {
  python3 -B - "$1" "$repo/tests/drag-receiver.py" "$SB/receiver.log" "${@:2}" <<'PY'
import os, select, signal, sys
from pathlib import Path

# Sample input: argv[1:] is ["stop", "/tree/tests/drag-receiver.py", "/run/receiver.log", "123"].
mode, script, log, *pids = sys.argv[1:]
term_seconds, kill_seconds = 3, 2

def owned(pid):
    try:
        # Sample input: cmdline is b"python3\0/tree/tests/drag-receiver.py\0/run/receiver.log\0".
        arguments = Path("/proc", str(pid), "cmdline").read_bytes().split(b"\0")
        return os.fsencode(script) in arguments and os.fsencode(log) in arguments
    except (FileNotFoundError, ProcessLookupError):
        return False

for raw_pid in pids:
    # Sample input: receiver PID argument "123" becomes integer 123.
    pid = int(raw_pid)
    if not owned(pid):
        continue
    if mode == "assert":
        print(f"receiver pid={pid} is still running")
        raise SystemExit(1)
    try:
        descriptor = os.pidfd_open(pid)
        try:
            # The descriptor pins the PID; recheck this run's exact argv before sending a signal.
            if not owned(pid):
                continue
            signal.pidfd_send_signal(descriptor, signal.SIGTERM)
            forced = not select.select([descriptor], [], [], term_seconds)[0]
            if forced:
                signal.pidfd_send_signal(descriptor, signal.SIGKILL)
                if not select.select([descriptor], [], [], kill_seconds)[0]:
                    print(f"receiver pid={pid} survived SIGKILL")
                    raise SystemExit(1)
            print(f"DRAG_RECEIVER_DRAIN pid={pid} forced_kill={str(forced).lower()}")
        finally:
            os.close(descriptor)
    except ProcessLookupError:
        pass
PY
}

receiver_dialogs_gone() {
  local receiver_pid=$1 dialogs dialog_pid
  dialogs=$(python3 -B - <<'PY'
import os
from pathlib import Path

dialog_name = "hyprland-dialog"
application_id = b"com.thisisgm.FleaDragReceiver"
for process in Path("/proc").iterdir():
    if not process.name.isdigit():
        continue
    try:
        if process.stat().st_uid != os.getuid():
            continue
        # Sample input: /proc/456/comm contains "hyprland-dialog\n".
        if (process / "comm").read_text().strip() != dialog_name:
            continue
        if application_id in (process / "cmdline").read_bytes():
            print(process.name)
    except (FileNotFoundError, ProcessLookupError):
        continue
PY
  ) || { bad "could not check receiver $receiver_pid compositor dialogs"; return 1; }
  if [ -n "$dialogs" ]; then
    while IFS= read -r dialog_pid; do
      bad "receiver $receiver_pid left hyprland-dialog pid=$dialog_pid"
    done <<< "$dialogs"
    return 1
  fi
  ok "receiver $receiver_pid left no compositor dialog"
}

stop_receiver() {
  local pid=$1
  [ "${RECV_REAPED[$pid]:-false}" = true ] && return 0
  receiver_processes stop "$pid" || { bad "receiver pid=$pid could not be stopped"; return 1; }
  wait "$pid" 2>/dev/null || true
  RECV_REAPED[$pid]=true
  receiver_dialogs_gone "$pid"
}

stop_receivers() {
  local pid status=0
  for pid in "${RECV_PIDS[@]}"; do
    stop_receiver "$pid" || status=1
  done
  return "$status"
}

assert_receivers_gone() {
  if receiver_processes assert "${RECV_PIDS[@]}"; then
    ok "no receiver from this run remains"
  else
    bad "a receiver from this run remains"
    return 1
  fi
}

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
  local status=$? drained=true journal found
  trap - EXIT
  if [ "$button_down" = true ]; then
    ydotool key 1:1 1:0 >/dev/null 2>&1 || { bad "cleanup could not cancel the held drag"; status=1; }
  fi
  stop_receivers || { status=1; drained=false; }
  assert_receivers_gone || { status=1; drained=false; }
  stop_owned_processes || { status=1; drained=false; }
  # Teardown is bounded even when the owned application cannot drain; failed teardown retains its fixtures.
  if [ "$button_down" = true ]; then
    ydotool click 0x80 >/dev/null 2>&1 || { bad "cleanup could not release the pointer"; status=1; }
  fi
  if [ "$control_down" = true ]; then
    ydotool key 29:0 >/dev/null 2>&1 || { bad "cleanup could not release Ctrl"; status=1; }
  fi
  if [ "$shift_held" = true ]; then
    ydotool key 42:0 >/dev/null 2>&1 || { bad "cleanup could not release Shift"; status=1; }
  fi
  if [ -f "$SB/flea.log" ]; then
    note "native stderr from $SB/flea.log"
    # WAYLAND_DEBUG traces every request. Keep the drag facts and the application's own lines.
    if grep -q '^\[' "$SB/flea.log"; then
      grep -E 'origin window|start_drag' "$SB/flea.log" || true
      grep -E "$cleanup_drop_events" "$SB/flea.log" | tail -n "$cleanup_drop_event_lines" || true
      grep -v -E '^\[' "$SB/flea.log" | tail -n "$cleanup_stderr_lines"
    else
      cat -- "$SB/flea.log"
    fi
  fi
  if [ "$drained" = true ]; then
    # The operator's own journal entries are allowed; paths from this run are not.
    journal="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/flea/undo-journal"
    if [ -f "$journal" ]; then
      found=0
      grep -qF -- "$SB" "$journal" 2>/dev/null || found=$?
      case $found in
        0) bad "run leaked sandbox paths into the operator journal $journal from $SB"; status=1 ;;
        1) ;;
        *) bad "could not read the operator journal $journal to check it for sandbox paths from $SB"; status=1 ;;
      esac
    fi
    sandbox_remove "$SB" 2>/dev/null || status=1
    # The shared rail/R7 tmpfs root names this run in its marker; the runtime parent is never a target.
    if [[ -n "${XDEV:-}" ]]; then
      if [[ "${XDG_RUNTIME_DIR:-}" == "/run/user/$(id -u)" && "$XDEV" == /* \
          && "$XDEV" == "$XDG_RUNTIME_DIR"/flea-drag-xdev-???????? \
          && -f "$XDEV/$SANDBOX_MARKER" && "$(< "$XDEV/$SANDBOX_MARKER")" == "$SB" ]]; then
        FIXTURE_ROOT="$XDG_RUNTIME_DIR" sandbox_remove "$XDEV" || status=1
      else
        bad "cleanup refused unowned tmpfs fixture $XDEV"; status=1
      fi
    fi
  else
    bad "cleanup did not drain; fixtures kept at $SB ${XDEV:-}"
  fi
  exit "$status"
}
trap cleanup EXIT

# ---------------------------------------------------------------- fixture
sandbox_make "$SB"
# The session runtime dir stays real for the compositor, so the journal needs its own knob.
UNDO_DIR=$SB/flea-undo
mkdir -p "$UNDO_DIR" || die "the suite undo dir could not be created at $UNDO_DIR"
chmod 0700 "$UNDO_DIR" || die "the suite undo dir could not be locked down"
export FLEA_UNDO_DIR="$UNDO_DIR"
export XDG_CONFIG_HOME="$HOMEDIR/.config" XDG_STATE_HOME="$HOMEDIR/.local/state"
export XDG_DATA_HOME="$HOMEDIR/.local/share" XDG_CACHE_HOME="$HOMEDIR/.cache"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_STATE_HOME/omarchy" "$XDG_DATA_HOME" "$XDG_CACHE_HOME" "$HOMEDIR/aaa" "$HOMEDIR/bbb"
ln -sfn "$HOME/.local/state/omarchy/current" "$HOMEDIR/.local/state/omarchy/current"
for f in r1a r1b r2 r3 r4; do printf '%s payload\n' "$f" > "$HOMEDIR/$f.txt"; done
# R6 shows hidden files in a second tab on the same directory, so every index below this one shifts.
printf 'hidden\n' > "$HOMEDIR/.r0hidden"
if [[ "$drag_case" == look235 ]]; then
  mkdir "$HOMEDIR/omarchy" || exit 1
  seed=$(jq -nc --arg path "$HOMEDIR/omarchy" '{view:"list",keys:"default",updates:{autoCheck:false},
    places:{rail:"shown",autoHide:false,showHome:true,showTrash:true,showNetwork:false,showDevices:false,
      favourites:[{label:"omarchy",path:$path}]}}') || exit 1
  HOME="$HOMEDIR" "$FLEA_BIN" --ui-state "$seed" >/dev/null || exit 1
else
  [[ -d "$XDG_RUNTIME_DIR" && -O "$XDG_RUNTIME_DIR" && ! -L "$XDG_RUNTIME_DIR" ]] \
    || die "the real runtime directory is unavailable or not owned"
  XDEV=$(mktemp -d "$XDG_RUNTIME_DIR/flea-drag-xdev-XXXXXXXX") || exit 1
  printf '%s\n' "$SB" > "$XDEV/$SANDBOX_MARKER" || exit 1
  xdev_source_dev=$(stat -c %d -- "$HOMEDIR") || die "cannot stat the source fixture"
  xdev_dest_dev=$(stat -c %d -- "$XDEV") || die "cannot stat the tmpfs fixture"
  [[ "$xdev_source_dev" != "$xdev_dest_dev" ]] || die "the tmpfs fixture is on the source filesystem"
  [[ "$(stat -f -c %T -- "$XDEV")" == tmpfs ]] || die "the runtime fixture is not tmpfs"
  printf 'DRAG_XDEV source_dev=%s destination_dev=%s filesystem=tmpfs path=%s\n' "$xdev_source_dev" "$xdev_dest_dev" "$XDEV"
  mkdir -p "$XDEV/big" "$SB/bin" || exit 1
  # Sample input: one removable lsblk volume with a tmpfs mountpoint, the suite's existing other-device stand-in.
  jq -nc --arg path "$XDEV/big" '{blockdevices:[{name:"sdz1",path:"/dev/sdz1",label:"USB",mountpoints:[$path],rm:true,ro:false,tran:"usb",size:128000000000,type:"part",fstype:"tmpfs"}]}' > "$SB/lsblk.json" || exit 1
  cat > "$SB/bin/lsblk" <<'SH'
#!/usr/bin/env bash
cat -- "$FLEA_DRAG_LSBLK"
SH
  chmod +x "$SB/bin/lsblk" || exit 1
  export PATH="$SB/bin:$PATH" FLEA_DRAG_LSBLK="$SB/lsblk.json"
  seed=$(jq -nc --arg path "$HOMEDIR/aaa" '{view:"list",keys:"default",updates:{autoCheck:false},places:{rail:"shown",autoHide:false,showHome:true,showTrash:true,showNetwork:false,showDevices:true,favourites:[{label:"Favorite",path:$path}]}}') || exit 1
  HOME="$HOMEDIR" "$FLEA_BIN" --ui-state "$seed" >/dev/null || exit 1
fi

# ---------------------------------------------------------------- pointer
warp() { glide_to "$1" "$2"; }
move_rel() { ydotool mousemove -x "$1" -y "$2" >/dev/null 2>&1 || die "relative pointer motion failed"; }
press()   { pressed_path=$(ipc path); owned_path "$pressed_path"; button_down=true; ydotool click 0x40 >/dev/null 2>&1 || die "pointer press failed"; }
release() { owned_path "$pressed_path"; ydotool click 0x80 >/dev/null 2>&1 || die "pointer release failed"; button_down=false; }
# evdev KEY_LEFTCTRL. Held through ydotool because a compositor keybind must not swallow it.
ctrl_down() { control_down=true; ydotool key 29:1 >/dev/null 2>&1 || die "Ctrl press failed"; }
ctrl_up()   { ydotool key 29:0 >/dev/null 2>&1 || die "Ctrl release failed"; control_down=false; }
# ydotool key 42 is Shift (29 above is Ctrl); held across the lift so the offer reads it.
shift_down() { shift_held=true; ydotool key 42:1 >/dev/null 2>&1 || die "Shift press failed"; }
shift_up()   { ydotool key 42:0 >/dev/null 2>&1 || die "Shift release failed"; shift_held=false; }

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
' "$repo/ui/boot/shell.qml" "$FLEA_PID"
}
ipc() { qs ipc -i "$MYID" call flea "$@" 2>&1; }
native_key() {
  local result=0
  omarchy-drive key --window flea "$@" || result=$?
  (( result == 0 )) || die "native key delivery failed with status $result: $*"
}
# Sample input: '{"notice":""}' stays JSON; '/home/x' or an observer error becomes one JSON string.
evidence_json() {
  if [[ -n "$1" ]] && jq -e type >/dev/null 2>&1 <<< "$1"; then printf '%s' "$1"; else jq -Rn --arg text "$1" '$text'; fi
}

# Sample output: DRAG_EXPECT_FAIL {"reader":"lastMessage","expected":"Copied 2 items · z undoes","observed":"","statusActivityState":{"notice":""},...}
# One line of every transfer, status bar, tab and window fact the seam can read, printed only when a read fails and never fatal.
expect_evidence() {
  local reader value clients active args=()
  for reader in statusActivityState statusFooterState collideState dualState keyDeliveryState path tabCount tabIndex \
                lastMessage stickyMessage statusPrimary statusError; do
    value=$(ipc "$reader") || value="observer exit $?: $value"
    args+=(--argjson "$reader" "$(evidence_json "$value")")
  done
  clients=$(hyprctl clients -j 2>&1 | jq -c --argjson pid "${MYPID:-0}" \
    '[.[] | select(.pid == $pid) | {address, at, size, floating, focus: .focusHistoryID}]' 2>&1) || clients="hyprctl clients failed: $clients"
  active=$(hyprctl activewindow -j 2>&1 | jq -c '{address, class}' 2>&1) || active="hyprctl activewindow failed: $active"
  args+=(--argjson clients "$(evidence_json "$clients")" --argjson activeWindow "$(evidence_json "$active")")
  printf 'DRAG_EXPECT_FAIL %s\n' "$(jq -nc --arg reader "$1" --arg expected "$2" --arg observed "$3" '$ARGS.named' "${args[@]}" 2>&1)"
}

# One line straight after a release: a notice that came and went reads differently from one never said.
after_drop_line() {
  local state dual path tab
  state=$(ipc statusActivityState) || state="observer exit $?: $state"
  dual=$(ipc dualState) || dual="observer exit $?: $dual"
  path=$(ipc path) || path="observer exit $?: $path"
  tab=$(ipc tabIndex) || tab="observer exit $?: $tab"
  printf 'DRAG_R9_AFTER_DROP %s\n' "$(jq -nc --argjson state "$(evidence_json "$state")" --argjson dual "$(evidence_json "$dual")" \
    --arg path "$path" --arg tab "$tab" \
    '{notice: ($state.notice? // null), errors: ($state.errors? // null), running: [$state | try .activities[].running catch empty],
      currentPane: ($dual.focused? // null), path: $path, tabIndex: $tab}' 2>&1)"
}

expect_ipc() {
  local reader="$1" expected="$2" observed attempt
  for ((attempt=1; attempt<=40; attempt++)); do
    observed=$(ipc "$reader") || { expect_evidence "$reader" "$expected" "$observed"; die "native observer failed: $reader"; }
    if [[ "$observed" == "$expected" ]]; then ok "$reader = $expected"; return; fi
    sleep 0.25
  done
  expect_evidence "$reader" "$expected" "$observed"
  die "$reader expected [$expected], observed [$observed]"
}

# Sample output: DRAG_R7 phase=before-Return reader=pathBarText value=/run/user/1000/flea-drag-xdev-AbCdEf12/big
walk_state() {
  local leg="$1" phase="$2" reader value
  for reader in listInFlight listRequests tabCount tabIndex tabLabels path keyDeliveryState pathBarOpen pathBarText lastMessage statusError; do
    value=$(ipc "$reader") || die "$leg $phase observer failed: $reader: $value"
    printf 'DRAG_%s phase=%s reader=%s value=%q\n' "$leg" "$phase" "$reader" "$value"
  done
}

r5_evidence() {
  local reader value status line
  for reader in lastMessage statusError listInFlight tabIndex path statusActivityState; do
    status=0
    value=$(ipc "$reader") || status=$?
    note "R5 $reader: [$value] (observer exit $status)"
  done
  note "R5 floor point used: [${r5_floor_point:-unmeasured}]"
  note "R5 window geometry used (x y width height): [$WX $WY $WW $WH]"
  note "R5 last $r5_data_device_lines data-device lines:"
  grep -E 'wl_data_(source|offer|device)' "$SB/flea.log" | tail -n "$r5_data_device_lines" | while IFS= read -r line; do note "$line"; done
}

r5_wait_listing() {
  local attempt tab path loading
  for ((attempt=1; attempt<=r5_poll_attempts; attempt++)); do
    if ! tab=$(ipc tabIndex) || ! path=$(ipc path) || ! loading=$(ipc listInFlight); then
      r5_evidence
      die "R5 destination listing observer failed"
    fi
    if [[ "$tab" == "$r5_target_tab" && "$path" == "$HOMEDIR/bbb" && "$loading" == false ]]; then
      ok "R5 destination tab and listing settled before measuring the floor"
      return 0
    fi
    sleep "$r5_poll_seconds"
  done
  r5_evidence
  die "R5 destination listing did not settle: tab=[$tab] path=[$path] listInFlight=[$loading]"
}

# Read the same target-owned activity as expect_feedback before releasing the held drag.
r5_wait_feedback() {
  local attempt state owner="$HOMEDIR/bbb" line="Move 1 item to $HOMEDIR/bbb · Ctrl copies, Shift moves"
  for ((attempt=1; attempt<=r5_poll_attempts; attempt++)); do
    state=$(ipc statusActivityState) || { r5_evidence; die "R5 drag activity observer failed"; }
    # Sample input: {"activities":[{"running":false,"ownerPath":"/run/home/bbb","text":"Move 1 item to /run/home/bbb · Ctrl copies, Shift moves"}]}.
    if jq -e --arg owner "$owner" --arg line "$line" '
        [.activities[] | select(.running | not) | {ownerPath,text}] ==
        [{ownerPath:$owner,text:$line}]' <<< "$state" >/dev/null; then
      ok "R5 floor feedback owner=[$owner] line=[$line]"
      return 0
    fi
    sleep "$r5_poll_seconds"
  done
  r5_evidence
  die "R5 floor feedback did not name bbb: observed $state"
}

# The product entry resolves the UI, renderer and backend identity before execing Quickshell.
QSG_RHI_BACKEND="${QSG_RHI_BACKEND:-vulkan}" HOME="$HOMEDIR" FLEA_TEST_RUN_ROOT="$SB" \
  WAYLAND_DEBUG=1 \
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
[ -n "$MYID" ] || { echo "no instance of $repo/ui/boot/shell.qml came up"; exit 1; }
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
    n=$(ipc visibleRowName "$i")
    if [ "$n" = "$1" ]; then echo "$i"; return 0; fi
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

# Seconds a row or tab centre may take to answer after a swap (50 polls of 0.1 s), and the poll gap.
centre_poll_attempts=50
centre_poll_seconds=0.1
# centre_fail_line <reader> <name> : one DRAG_CENTRE_FAIL line with what the row lookup, the centre read and the window said.
centre_fail_line() {
  local idx="" centre="" listing=""
  if [[ "$1" = screen_centre ]]; then
    idx=$(rowidx "$2" 2>&1) || idx="none: $idx"
    [[ "$idx" =~ ^[0-9]+$ ]] && centre=$(ipc rowCentre "$idx" 2>&1)
  else
    centre=$(ipc tabCentre "$2" 2>&1)
  fi
  listing=$(jq -nc --arg inFlight "$(ipc listInFlight 2>&1)" --arg total "$(ipc total 2>&1)" --arg view "$(ipc viewMode 2>&1)" \
    --arg path "$(ipc path 2>&1)" '$ARGS.named')
  printf 'DRAG_CENTRE_FAIL %s\n' "$(jq -nc --arg reader "$1" --arg name "$2" --arg rowidx "$idx" --arg centre "$centre" \
    --argjson window "$(jq -nc --argjson w "${WW:-0}" --argjson h "${WH:-0}" '{width: $w, height: $h}')" \
    --argjson listing "$listing" '$ARGS.named')"
}
# await_centre <xvar> <yvar> <reader> <name> : polls until a visible centre answers, then sets both variables in this shell, and ends the suite otherwise.
await_centre() {
  local await_x=$1 await_y=$2 await_reader=$3 await_name=$4 await_point await_try await_kind
  for (( await_try = 0; await_try < centre_poll_attempts; await_try++ )); do
    if await_point=$("$await_reader" "$await_name"); then
      read -r "$await_x" "$await_y" <<< "$await_point"
      return 0
    fi
    sleep "$centre_poll_seconds"
  done
  centre_fail_line "$await_reader" "$await_name"
  [[ "$await_reader" = screen_tab_centre ]] && await_kind=tab || await_kind=row
  die "the $await_kind $await_name has no visible screen centre"
}
# centre_into <xvar> <yvar> <row name> and tab_centre_into <xvar> <yvar> <tab index> : a failure is a die in the caller, never an unbound $1.
centre_into() { await_centre "$1" "$2" screen_centre "$3"; }
tab_centre_into() { await_centre "$1" "$2" screen_tab_centre "$3"; }

native_tab() {
  local index="$1" x y
  expect_ipc listInFlight false
  tab_centre_into x y "$index"
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

# Native file-drop proof and the two mid-drag captures share the suite's existing pointer driver.
raildrop_case() {
  local label group name destination verb state index point x y sx sy before evidence top focus
  expect_ipc listInFlight false
  native_key -k Escape
  before=$(ipc total)
  evidence="${capture_root:-$SB}/raildrop-evidence"
  mkdir -p "$evidence" || die "raildrop: cannot create evidence directory"
  for label in Favorite USB; do
    if [[ "$label" == Favorite ]]; then
      group=favourite; name=rail-move.txt; destination="$HOMEDIR/aaa"; verb=Move
    else
      group=device; name=rail-copy.txt; destination="$XDEV/big"; verb=Copy
    fi
    state=$(ipc railEntries) || die "raildrop: rail entries unavailable"
    # Sample input: [{"label":"USB","group":"device","path":"/run/user/1000/flea-drag-xdev-AbCdEf12/big"}] selects the unique row's index.
    index=$(jq -er --arg label "$label" --arg group "$group" 'to_entries | map(select(.value.label == $label and .value.group == $group)) | if length == 1 then .[0].key else error("rail target missing or ambiguous") end' <<< "$state") || die "raildrop: no unique $label row"
    point=$(ipc railRowCentre "$index")
    [[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || die "raildrop: $label has no geometry"
    read -r x y <<< "$point"
    printf '%s payload\n' "$name" > "$HOMEDIR/$name"
    expect_ipc total $((before + 1))
    centre_into sx sy "$name"
    warp "$sx" "$sy"; sleep 0.4
    press; sleep 0.3
    glide_to "$((WX + x))" "$((WY + y))"
    if [[ "$verb" == Move ]]; then
      expect_ipc stickyMessage "Move 1 item to Favorite · Ctrl copies, Shift moves"
    else
      expect_ipc stickyMessage "Copy 1 item to USB"
    fi
    state=$(ipc railDetails) || die "raildrop: cue geometry unavailable"
    # Sample input: railDetails.rows has {label:"Favorite",detail:"move here",labelRect:{x:42,width:40},detailRect:{x:140,width:60}}.
    jq -e --arg label "$label" --arg cue "$([[ "$verb" == Move ]] && echo 'move here' || echo 'copy here')" '[.rows[] | select(.label == $label)] | length == 1 and (.[0] | .detail == $cue and .indicatorVisible == false and .labelRect.x + .labelRect.width < .detailRect.x)' <<< "$state" >/dev/null || die "raildrop: $label drop cue failed: $state"
    [[ "$button_down" == true ]] || die "raildrop: capture lost its held pointer"
    printf '%s\n' "$state" > "$evidence/$label.railDetails.json"
    ipc statusActivityState > "$evidence/$label.statusActivityState.json" || die "raildrop: status capture failed"
    if [[ "$drag_case" == raildrop ]]; then
      grim -g "$WX,$WY ${WW}x${WH}" "$evidence/$label.png" || die "raildrop: screenshot failed"
    fi
    printf 'RAILDROP_HOVER row=%s verb=%s cue=%s held=true\n' "$label" "$verb" "$([[ "$verb" == Move ]] && echo move || echo copy)"
    release
    wait_for "$destination/$name" present || die "raildrop: $label drop never landed"
    if [[ "$verb" == Move ]]; then
      [[ ! -e "$HOMEDIR/$name" ]] || die "raildrop: same-device favorite copied instead of moving"
      expect_ipc total "$before"
    else
      [[ -e "$HOMEDIR/$name" ]] || die "raildrop: other-device USB removed the source"
    fi
    cmp -s "$destination/$name" <(printf '%s payload\n' "$name") || die "raildrop: landed content changed"
    printf 'RAILDROP_DROP row=%s verb=%s source=%s payload=exact\n' "$label" "$verb" "$([[ -e "$HOMEDIR/$name" ]] && echo kept || echo gone)"
    state=$(ipc keyDeliveryState) || die "raildrop: key delivery state unavailable"
    focus=$(ipc focusView) || die "raildrop: focus view unavailable"
    # Sample input (tests/fixtures/undo-journal-golden.json): {"v":2,"gen":7,"undo":[{"op":"move","steps":[{"k":"m","from":"/a/x","to":"/b/x"}]}],"redo":[]}.
    top=$(jq -c '.undo[-1] // null' "$UNDO_DIR/undo-journal" 2>/dev/null) || top='"unavailable"'
    printf 'RAILDROP_BEFORE_UNDO row=%s view=%s focus=%s top=%s\n' "$label" "$focus" "$state" "$top"
    native_key z
    wait_for "$destination/$name" absent || die "raildrop: undo kept $label destination"
    wait_for "$HOMEDIR/$name" present || die "raildrop: undo lost the source"
    owned_path "$HOMEDIR/$name"; rm -- "$HOMEDIR/$name"
    expect_ipc total "$before"
    printf 'RAILDROP_UNDO row=%s source=restored destination=gone\n' "$label"
  done
}

if [[ "$drag_case" == raildrop ]]; then
  raildrop_case
  printf 'RAILDROP_NATIVE DONE failed=%s evidence=%s\n' "$fail" "$capture_root/raildrop-evidence"
  exit "$fail"
fi

# Native pin proof: one folder held over the FAVORITES heading arms and pins on release, a file
# over the same heading arms nothing, and undo has nothing to say about a pin because no file moved.
pindrop_case() {
  local sx sy x y point state index pin_home evidence before
  expect_ipc listInFlight false
  mkdir -p "$HOMEDIR/pin-source.dir"
  printf 'pin payload\n' > "$HOMEDIR/pin-source.dir/inside.txt"
  pin_home="$HOMEDIR/pin-source.dir"
  evidence="${capture_root:-$SB}/pindrop-evidence"
  mkdir -p "$evidence" || die "pindrop: cannot create evidence directory"
  point=$(ipc railPinCentre)
  # Sample input: "412 88" is the FAVORITES heading's centre in window coordinates.
  [[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || die "pindrop: the FAVORITES heading has no geometry: $point"
  read -r x y <<< "$point"
  # A click on the heading is inert (no row sits under it) and lands the window's focus, so the
  # suite's own ydotool keys reach the app without omarchy-drive, which not every box carries.
  warp "$((WX + x))" "$((WY + y))"; sleep 0.2
  press; release
  ydotool key 1 >/dev/null 2>&1 || die "pindrop: the Escape tap failed"

  # A file over the heading arms nothing: the kind answer says file, so the cue never lights.
  printf 'file payload\n' > "$HOMEDIR/plain-file.txt"
  centre_into sx sy plain-file.txt
  warp "$sx" "$sy"; sleep 0.4
  press; sleep 0.3
  glide_to "$((WX + x))" "$((WY + y))"
  state=$(ipc railPinState)
  jq -e '.armed == false' <<< "$state" >/dev/null || die "pindrop: a file armed the heading: $state"
  printf '%s\n' "$state" > "$evidence/file.state.json"
  release
  wait_for "$HOMEDIR/plain-file.txt" present || die "pindrop: the refused file drop moved the file"
  rm -- "$HOMEDIR/plain-file.txt"

  # One folder over the heading arms the pin, and the drop pins it, labelled by its own name.
  before=$(ipc railEntries) || die "pindrop: rail entries unavailable"
  centre_into sx sy pin-source.dir
  warp "$sx" "$sy"; sleep 0.4
  press; sleep 0.3
  glide_to "$((WX + x))" "$((WY + y))"
  expect_ipc railPinState "{\"armed\":true,\"pending\":\"$pin_home\",\"kind\":\"dir\"}"
  ipc railPinState > "$evidence/folder.state.json"
  [[ "$button_down" == true ]] || die "pindrop: capture lost its held pointer"
  release
  expect_ipc favouritesSaving false
  state=$(ipc railEntries) || die "pindrop: rail entries unavailable after the pin"
  jq -e --arg label pin-source.dir --arg home "$pin_home" 'to_entries | map(select(.value.label == $label and .value.group == "favourite" and .value.path == $home)) | length == 1' <<< "$state" >/dev/null || die "pindrop: the pin never landed: $state"
  index=$(jq -er --arg label pin-source.dir --arg home "$pin_home" 'to_entries | map(select(.value.label == $label and .value.group == "favourite" and .value.path == $home)) | if length == 1 then .[0].value.favouriteIndex else error("pin row missing or ambiguous") end' <<< "$state") || die "pindrop: no unique pinned favourite"
  printf '%s\n' "$state" > "$evidence/folder.railEntries.json"

  # A pin is a favourite, not a move: the folder stayed home and undo says nothing about it.
  [[ -d "$pin_home" ]] || die "pindrop: the pinned folder left home"
  ydotool key 44 >/dev/null 2>&1 || die "pindrop: the undo tap failed"
  sleep 1
  state=$(ipc railEntries)
  jq -e --arg label pin-source.dir 'any(.[]; .label == $label and .group == "favourite")' <<< "$state" >/dev/null || die "pindrop: undo unpinned the favourite: $state"
  [[ -d "$pin_home" ]] || die "pindrop: undo moved the pinned folder"

  # The menu's own removal path unpins, and the rail reads back the way it began.
  ipc favouriteRemove "$index" >/dev/null || die "pindrop: favouriteRemove refused"
  expect_ipc favouritesSaving false
  state=$(ipc railEntries)
  printf '%s\n' "$state" > "$evidence/after-unpin.railEntries.json"
  printf '%s\n' "$before" > "$evidence/before.railEntries.json"
  [[ "$state" == "$before" ]] || { diff <(jq -S . "$evidence/before.railEntries.json") <(jq -S . "$evidence/after-unpin.railEntries.json") | head -8 >&2; die "pindrop: the rail did not read back (diff above, before vs after-unpin)"; }
  rm -rf -- "$pin_home"
  printf 'PINDROP file=refused folder=pinned armed=true undo=silent unpin=restored\n'
}

if [[ "$drag_case" == pindrop ]]; then
  pindrop_case
  printf 'PINDROP_NATIVE DONE failed=%s evidence=%s\n' "$fail" "$capture_root/pindrop-evidence"
  exit "$fail"
fi

if [[ "$drag_case" == look235 ]]; then
  . "$repo/tests/drag-look235-case.sh"
  exit "$fail"
fi

# ---------------------------------------------------------------- R0
echo
echo "== R0: with Settings up, no drag runs in the pane underneath it =="
# Issue 120 (tgienger): a drag begun over the open panel moved files in the listing behind it,
# because a row's DragHandler carries CanTakeOverFromItems and takes the grab from the panel's own
# ground. PR 124 (tcraid0) is the fix, and the pane being disabled is the only thing that stops it.
r0_before=$(ipc total)
aaa_before_r0=$(ls -A "$HOMEDIR/aaa" | tr '\n' ' ')
printf 's1 payload\n' > "$HOMEDIR/s1.txt"
expect_ipc total $((r0_before + 1))
centre_into a1x a1y aaa
native_key -M ctrl -k comma -m ctrl
expect_ipc settingsOpen true
# R0 owns ground, so derive its press from the visible row outside the measured card and rail.
r0_index=$(rowidx s1.txt) || die "R0 could not find s1.txt's row"
r0_row=$(ipc rowRect "$r0_index") || die "R0 could not read s1.txt's row rectangle"
r0_card=$(ipc settingsCardRect) || die "R0 could not read Settings' card rectangle"
r0_area=$(ipc listAreaRect) || die "R0 could not read the listing viewport"
r0_rail=$(ipc railState) || die "R0 could not read the pane rail"
r0_point=$(python3 "$repo/tests/lib/settings-ground-point.py" "$r0_row" "$r0_card" "$r0_area" "$r0_rail" "$WIN" "$pointer_tolerance") \
    || die "R0 has no ground press point over s1.txt outside Settings and the rail, see DRAG_R0_GROUND_REFUSED coordinates"
read -r s1x s1y <<< "$r0_point"
warp "$s1x" "$s1y"; sleep 0.4
# Recheck the card and rail after pointer motion, before any button is held.
r0_index=$(rowidx s1.txt) || die "R0 could not find s1.txt's row before the press"
# Sample input: "1234, 567" from hyprctl cursorpos, the comma stripped before read.
read -r r0_actual_x r0_actual_y <<< "$(hyprctl cursorpos | tr -d ',')"
python3 "$repo/tests/lib/settings-ground-point.py" "$(ipc rowRect "$r0_index")" "$(ipc settingsCardRect)" \
    "$(ipc listAreaRect)" "$(ipc railState)" "$WIN" 1 "$r0_actual_x $r0_actual_y" >/dev/null \
    || die "R0 press point is not on the panel ground, see DRAG_R0_GROUND_REFUSED coordinates"
r0_folder_index=$(rowidx aaa) || die "R0 could not find aaa's unchanged destination row"
python3 "$repo/tests/lib/settings-ground-point.py" "$(ipc rowRect "$r0_folder_index")" "$(ipc settingsCardRect)" \
    "$(ipc listAreaRect)" "$(ipc railState)" "$WIN" 1 "$a1x $a1y" >/dev/null \
    || die "R0's unchanged aaa destination is not on the panel ground, see DRAG_R0_GROUND_REFUSED coordinates"
printf 'DRAG_R0_GROUND press=%s,%s observed=%s,%s row=%q card=%q rail=%q destination=%s,%s\n' "$s1x" "$s1y" "$r0_actual_x" "$r0_actual_y" "$r0_row" "$r0_card" "$r0_rail" "$a1x" "$a1y"
press; sleep 0.3
glide_to "$a1x" "$a1y"; sleep 0.5
release; sleep 0.8
check "a drag through the open panel moves nothing" \
      "$([ -e "$HOMEDIR/aaa/s1.txt" ] && echo moved || echo clean)" "clean"
check "and the file the pointer began on is where it was" \
      "$([ -e "$HOMEDIR/s1.txt" ] && echo still-there || echo gone)" "still-there"
# The release lands on the panel's own ground, which is a click outside the card, so it closes on it.
check "the gesture belonged to the panel, which closed on the release" "$(ipc settingsOpen)" "false"
check "and the folder the drag crossed holds exactly what it held" \
      "$(ls -A "$HOMEDIR/aaa" | tr '\n' ' ')" "$aaa_before_r0"
# Both paths, because the case that fails is the one where the file is in the folder, and a fixture
# left with a stray row in aaa is a fixture every case after this one counts wrongly.
owned_path "$HOMEDIR/s1.txt"; rm -f "$HOMEDIR/s1.txt"
owned_path "$HOMEDIR/aaa/s1.txt"; rm -f "$HOMEDIR/aaa/s1.txt"
expect_ipc total "$r0_before"

# ---------------------------------------------------------------- R2
echo
echo "== R2: the drop lands where the pointer is, not one frame stale =="
# ui/List.qml positions the ghost by assignment and never by a binding, because Drag moves are posted
# and Drag.drop() flushes the pending one first. A stale ghost drops into a folder the drag merely
# crossed, so this drag crosses aaa deliberately and finishes on bbb.
centre_into sx sy r2.txt
centre_into ax ay aaa
centre_into bx by bbb
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

# ---------------------------------------------------------------- RT
echo
echo "== RT: a row dropped on the rail's Trash row goes to the trash =="
# The rail's Trash row sends dd's trash request by path into this run's own $XDG_DATA_HOME/Trash; the baseline count waits for a settled listing.
expect_ipc listInFlight false
rt_before=$(ipc total)
printf 't1 payload\n' > "$HOMEDIR/t1.txt"
expect_ipc total $((rt_before + 1))
trash_index=""
IFS='|' read -r -a rail_labels <<< "$(ipc railLabels)"
for i in "${!rail_labels[@]}"; do [ "${rail_labels[$i]}" = Trash ] && trash_index=$i && break; done
[ -n "$trash_index" ] || die "RT: the rail carries no Trash row: $(ipc railLabels)"
point=$(ipc railRowCentre "$trash_index")
[[ "$point" =~ ^[0-9]+\ [0-9]+$ ]] || die "RT: the Trash row has no geometry: $point"
read -r tx ty <<< "$point"
centre_into sx sy t1.txt
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$((WX + tx))" "$((WY + ty))"; sleep 0.5
release; sleep 0.4
wait_for "$HOMEDIR/t1.txt" absent
check "the dropped file left its folder" \
      "$([ -e "$HOMEDIR/t1.txt" ] && echo still-there || echo gone)" "gone"
check "and it is in this run's Trash, not deleted" \
      "$([ -e "$XDG_DATA_HOME/Trash/files/t1.txt" ] && echo trashed || echo missing)" "trashed"
expect_ipc total "$rt_before"
raildrop_case

# ---------------------------------------------------------------- R3
echo
echo "== R3: ctrl decides copy versus move, and the lift is where it is read =="
# Lift reads Ctrl here, so the copy-alone offer drops a copy while Drag.active ignores later keys.
centre_into sx sy r3.txt
centre_into ax ay aaa
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
centre_into sx sy r4.txt
centre_into bx by bbb
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$bx" "$by"; sleep 0.8
MID=$(ipc stickyMessage)
release; sleep 0.6
check "the line names the folder under the pointer" "$MID" "Move 1 item to bbb · Ctrl copies, Shift moves"

# ---------------------------------------------------------------- R1
echo
echo "== R1: only a release over a valid folder may transfer =="
# ui/List.qml reads the grab transition and not active, because a release and a grab another item
# stole flip active the same way and only a release may drop. A synthetic pointer cannot steal a
# grab, so what is asserted here is the invariant that rule exists to protect, not the steal itself.
centre_into sx sy r1a.txt
centre_into fx fy r1b.txt
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$fx" "$fy"; sleep 0.5
release; sleep 0.8
check "a release over a file row transfers nothing" \
      "$([ -e "$HOMEDIR/r1a.txt" ] && echo kept || echo GONE)" "kept"
check "and the gesture leaves no status line behind" "$(ipc stickyMessage)" ""

centre_into sx sy r1b.txt
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
r5_target_tab=1
r5_poll_attempts=40
r5_poll_seconds=0.25
r5_delegate_rest_seconds=1.6
r5_pointer_settle_seconds=0.4
r5_press_settle_seconds=0.3
r5_drop_settle_seconds=0.6
r5_data_device_lines=15
r5_floor_point=""
# The rest-to-select rule. The second tab is walked into bbb through the path bar, the first tab is shown again,
# then r1a.txt is lifted, rested on the second tab past ui/TabBar.qml's hoverSwitchMs, and released
# on the empty floor under the rows. The marker resolves the drop by path, because after the switch
# the row indices name bbb's own rows; a same-filesystem move is what a plain drag means.
export PATH="$HOME/.local/bin:$PATH"
walk_state R5 before-t
expect_ipc tabCount 1
expect_ipc tabIndex 0
native_key t
walk_state R5 after-t
expect_ipc tabCount 2
expect_ipc tabIndex 1
native_key :
expect_ipc pathBarOpen true
native_key "$HOMEDIR/bbb"
walk_state R5 before-Return
native_key -k Return
walk_state R5 after-Return
expect_ipc pathBarOpen false
expect_ipc path "$HOMEDIR/bbb"
expect_ipc listInFlight false
check "the second tab shows bbb" "$(ipc path)" "$HOMEDIR/bbb"
walk_state R5 before-tab-click
native_tab 0
walk_state R5 after-tab-click
expect_ipc tabIndex 0
expect_ipc path "$HOMEDIR"
expect_ipc listInFlight false
check "and the first tab is the home listing again" "$(ipc path)" "$HOMEDIR"
centre_into sx sy r1a.txt
tab_centre_into tx ty "$r5_target_tab"
warp "$sx" "$sy"; sleep "$r5_pointer_settle_seconds"
press; sleep "$r5_press_settle_seconds"
# The rest outlives the switch by a second: the pressed row's delegate is released by the re-list
# while the drag still runs, and the QDrag used to die with it (quickshell SIGSEGV, 2026-09-07).
glide_to "$tx" "$ty"; sleep "$r5_delegate_rest_seconds"
r5_wait_listing
check "resting on the second tab selected it" "$(ipc tabIndex)" "$r5_target_tab"
r5_floor_point=$(floor_centre) || { r5_evidence; die "R5 has no measured destination listing floor"; }
# Sample input: R5's destination floor centre is "300 500".
read -r fx fy <<< "$r5_floor_point"
glide_to "$fx" "$fy"
r5_wait_feedback
release; sleep "$r5_drop_settle_seconds"
wait_for "$HOMEDIR/bbb/r1a.txt" present
r5_before_drop_fail=$fail
check "the file landed on the second tab's floor" \
      "$([ -e "$HOMEDIR/bbb/r1a.txt" ] && echo bbb || echo missing)" "bbb"
check "as a move, so the source is gone" \
      "$([ -e "$HOMEDIR/r1a.txt" ] && echo still-there || echo moved)" "moved"
if (( fail > r5_before_drop_fail )); then r5_evidence; fi
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
centre_into fx fy aaa
native_tab 0
check "and the home tab does not" "$(rowidx .r0hidden || echo none)" "none"
# Escape drops the restored selection; aaa already holds R3's copy, so the drop is judged by its delta.
native_key -k Escape; sleep 0.3
aaa_before=$(ls -A "$HOMEDIR/aaa" | tr '\n' ' ')
centre_into sx sy r1b.txt
tab_centre_into tx ty 2
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
echo "== R7: a loading tab refuses the drop; after its listing lands, a cross-device drag copies =="
# Stop only this suite's backend so the hovered current tab must refuse before its listing can land.
r7_home_tab=0
r7_tmpfs_tab=2
r7_poll_attempts=40
r7_poll_seconds=0.1
r7_row_poll_seconds=0.25
r7_pointer_settle_seconds=0.4
r7_press_settle_seconds=0.3
r7_release_settle_seconds=0.5
r7_stopped_state=T
# Sample input, /proc/<pid>/stat: '123 (flea) T 1 123 ...'; the owned backend's state is field 3.
backend_state() { cut -d' ' -f3 "/proc/$BACKEND_PID/stat"; }
check "the tmpfs root is another filesystem than the fixture" \
      "$([ "$(stat -c %d "$XDEV")" != "$(stat -c %d "$HOMEDIR")" ] && echo other || echo same)" "other"
# Passes once the current tab lists the payload and no listing is out; the in-flight read is the last one before the caller's key.
r7_payload_listed() {
  local attempt flight=unread total
  for ((attempt=1; attempt<=r7_poll_attempts; attempt++)); do
    # rowidx reads a failed observer as an absent row, so the row count is read here first: a failed read or a reply that is no count ends the suite by name.
    total=$(ipc total) && [[ "$total" =~ ^[0-9]+$ ]] || die "R7 row count unavailable: $total"
    if rowidx r7.txt >/dev/null; then
      flight=$(ipc listInFlight) || die "R7 listing state unavailable"
      if [[ "$flight" == false ]]; then ok "R7 the payload is listed and no listing is out"; return; fi
    fi
    sleep "$r7_poll_seconds"
  done
  walk_state R7 unsettled
  die "R7 the payload never settled in the listing: listed $(rowidx r7.txt >/dev/null && echo yes || echo no), in flight $flight"
}
printf 'r7 payload\n' > "$HOMEDIR/r7.txt"
# R6 left the third tab current; it is walked into the tmpfs directory through the path bar, as R5 walked into bbb.
check "the third tab is current" "$(ipc tabIndex)" "$r7_tmpfs_tab"
native_key :
expect_ipc pathBarOpen true
native_key "$XDEV/big"
walk_state R7 before-Return
# The watcher's re-read waits while anything holds the rows (Anchor.busy) and a path entered while a listing is out is refused, so Return waits for the payload's row.
r7_payload_listed
native_key -k Return
walk_state R7 after-Return
expect_ipc pathBarOpen false
expect_ipc path "$XDEV/big"
expect_ipc listInFlight false
check "the third tab lists the tmpfs directory" "$(ipc path)" "$XDEV/big"
native_tab "$r7_home_tab"
check "the home tab is current again" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 "$r7_poll_attempts"); do rowidx r7.txt >/dev/null 2>&1 && break; sleep "$r7_row_poll_seconds"; done
# The one backend this suite owns: the instance's child running FLEA_BIN --backend, ui/Backend.qml's command.
BACKEND_PID=""
for p in $(pgrep -P "$MYPID"); do
  [ "$(tr '\0' ' ' < "/proc/$p/cmdline" 2>/dev/null)" = "$FLEA_BIN --backend " ] && BACKEND_PID=$p
done
check "the suite found the one backend it owns" "$([ -n "$BACKEND_PID" ] && echo found || echo none)" "found"
[[ -n "$BACKEND_PID" ]] || die "R7 cannot hold the listing without its owned backend"
centre_into sx sy r7.txt
tab_centre_into tx ty "$r7_tmpfs_tab"
warp "$sx" "$sy"; sleep "$r7_pointer_settle_seconds"
press; sleep "$r7_press_settle_seconds"
kill -STOP "$BACKEND_PID" || die "R7 could not stop its owned backend"
for i in $(seq 1 "$r7_poll_attempts"); do
  [ "$(backend_state)" = "$r7_stopped_state" ] && break
  sleep "$r7_poll_seconds"
done
check "the owned backend stopped before the hover switch" "$(backend_state)" "$r7_stopped_state"
[ "$(backend_state)" = "$r7_stopped_state" ] || die "R7 backend did not stop"
glide_to "$tx" "$ty"
for i in $(seq 1 "$r7_poll_attempts"); do [ "$(ipc tabIndex)" = "$r7_tmpfs_tab" ] && [ "$(ipc listInFlight)" = true ] && break; sleep "$r7_poll_seconds"; done
check "resting on the tmpfs tab selected it" "$(ipc tabIndex)" "$r7_tmpfs_tab"
check "and its listing is out against the stopped backend" "$(ipc listInFlight)" "true"
release; sleep "$r7_release_settle_seconds"
check "the release was refused with the listing still out" "$(ipc listInFlight)" "true"
check "and the backend was still stopped at that point" "$(backend_state)" "$r7_stopped_state"
expect_ipc lastMessage "A directory is already loading."
expect_ipc statusError false
check "the refused drop leaves the tmpfs directory empty" "$(ls -A "$XDEV/big")" ""
check "and the refused source survives byte for byte" \
      "$(printf 'r7 payload\n' | cmp -s - "$HOMEDIR/r7.txt" && echo same || echo differs)" "same"
check "and the window survived the refusal" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"
kill -CONT "$BACKEND_PID" || die "R7 could not continue its owned backend"
expect_ipc path "$XDEV/big"
expect_ipc listInFlight false
check "nothing lands after the refused listing finishes" "$(ls -A "$XDEV/big")" ""
check "and the source still survives after the listing finishes" \
      "$(printf 'r7 payload\n' | cmp -s - "$HOMEDIR/r7.txt" && echo same || echo differs)" "same"
check "and the window survived the resumed listing" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"
# A fresh identity after the refusal checks distinguishes this lift from any wrongly queued first drop.
printf 'r7 second payload\n' > "$HOMEDIR/r7-second.txt" || die "R7 could not write the second drag's source"
native_tab "$r7_home_tab"
check "the second drag starts from the home listing" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 "$r7_poll_attempts"); do rowidx r7-second.txt >/dev/null 2>&1 && break; sleep "$r7_row_poll_seconds"; done
centre_into sx sy r7-second.txt
tab_centre_into tx ty "$r7_tmpfs_tab"
warp "$sx" "$sy"; sleep "$r7_pointer_settle_seconds"
press; sleep "$r7_press_settle_seconds"
glide_to "$tx" "$ty"
expect_ipc tabIndex "$r7_tmpfs_tab"
expect_ipc path "$XDEV/big"
expect_ipc listInFlight false
release; sleep "$r7_release_settle_seconds"
wait_for "$XDEV/big/r7-second.txt" present || die "R7 second drag did not reach the tmpfs tab"
check "the second drag landed on the tmpfs tab" \
      "$([ -e "$XDEV/big/r7-second.txt" ] && echo landed || echo missing)" "landed"
check "byte for byte" "$(cmp -s "$HOMEDIR/r7-second.txt" "$XDEV/big/r7-second.txt" && echo same || echo differs)" "same"
check "as a copy, so the source survives" \
      "$([ -e "$HOMEDIR/r7-second.txt" ] && echo kept || echo GONE)" "kept"
check "and the first drag's file never reached the destination" \
      "$([ -e "$XDEV/big/r7.txt" ] && echo landed || echo absent)" "absent"
check "and the window survived" "$(ipc total >/dev/null 2>&1 && echo alive || echo gone)" "alive"

# ---------------------------------------------------------------- R8
echo
echo "== R8: the line over a folder on another filesystem says copy, and the drop is one =="
# ui/List.qml's verbAt reads the source device off the marker, stamped at the lift: after the hover
# switch the pane's own dirDev is the destination's, and read from there the line said move over a
# folder the drop would copy into. The same dragCopy drives the row's "copy here" badge.
mkdir -p "$XDEV/big/dest"
printf 'r8 payload\n' > "$HOMEDIR/r8.txt"
native_tab 0
check "the home tab is current" "$(ipc path)" "$HOMEDIR"
for i in $(seq 1 40); do rowidx r8.txt >/dev/null 2>&1 && break; sleep 0.25; done
centre_into sx sy r8.txt
tab_centre_into tx ty 2
warp "$sx" "$sy"; sleep 0.4
press; sleep 0.3
glide_to "$tx" "$ty"
for i in $(seq 1 40); do [ "$(ipc path)" = "$XDEV/big" ] && [ "$(ipc listInFlight)" = false ] && rowidx dest >/dev/null 2>&1 && break; sleep 0.1; done
check "resting on the tmpfs tab listed it in full" "$(ipc path)" "$XDEV/big"
centre_into fx fy dest
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
if [ "$fail" != 0 ]; then
  note "stopping before R9 because earlier drag checks failed"
  echo "$((pass + fail)) checks, $fail failed"
  exit 1
fi

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
  local name="$1" first second px py expected
  first=$(rowidx "$name-a.txt") || die "first pair identity is not listed"
  second=$(rowidx "$name-b.txt") || die "second pair identity is not listed"
  centre_into px py "$name-a.txt"
  omarchy-drive click "$px" "$py" left >/dev/null || die "native plain selection failed"
  expect_ipc selectedIndices "$first"
  centre_into px py "$name-b.txt"
  omarchy-drive click "$px" "$py" left --mods ctrl >/dev/null || die "native additive selection failed"
  expected=$(jq -nr --argjson first "$first" --argjson second "$second" '[$first,$second] | sort | map(tostring) | join(",")')
  expect_ipc selectedIndices "$expected"
  expect_ipc selectionCount 2
}

begin_pair() {
  local name="$1" verb="$2" px py
  mark_pair "$name"
  centre_into px py "$name-a.txt"
  glide_to "$px" "$py"
  [[ "$verb" != Copy ]] || ctrl_down
  press
}

target_points() {
  local point x y
  centre_into folder_x folder_y folder
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
  [[ "$verb" != Move ]] || suffix=' · Ctrl copies, Shift moves'
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
    tab_centre_into tx ty 1
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
      after_drop_line
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
  [[ "$side" == 0 ]] || native_key -M shift -k Tab -m shift
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
    native_key -M shift -k Tab -m shift
  else
    source="$right"; destination="$left"; name=feedback-right; verb=Move
    dual_destination_side=0
    native_key -M shift -k Tab -m shift
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

# ---------------------------------------------------------------- R11
echo
echo "== R11: a press that travels on the chrome strip moves the window =="
# ui/boot/shell.qml asks for no decorations, so the compositor never gave this window a title bar and the
# strip is it. The window is floated and placed first: a tiled window has nowhere of its own to move
# to, and a full-screen float puts the strip under the Omarchy bar, which owns those pixels.
r11_geometry() {
  hyprctl clients -j | jq -er --argjson pid "$MYPID" \
    '[.[] | select(.pid == $pid)] | if length == 1 then .[0] else error("owned window missing or ambiguous") end
     | "\(.at[0]) \(.at[1]) \(.size[0]) \(.size[1]) \(.floating)"'
}
# Sample input: [{"pid":4242,"address":"0x55d0c0ffee00","floating":true}] answers 0x55d0c0ffee00 for pid 4242.
r11_addr=$(hyprctl clients -j | jq -er --argjson pid "$MYPID" '[.[] | select(.pid == $pid)] | if length == 1 then .[0].address else error("owned window missing or ambiguous") end') || die "R11 window address unavailable"
[[ "$r11_addr" =~ ^0x[0-9a-fA-F]+$ ]] || die "R11 window address is invalid"
r11_target_width=1200
r11_target_height=800
r11_target_x=400
r11_target_y=300
hypr_window_float "$r11_addr" "on" || die "R11 could not float the window"
sleep 0.5
hypr_window_resize "$r11_addr" "$r11_target_width" "$r11_target_height" || die "R11 could not resize the window"
sleep 0.4
hypr_window_move "$r11_addr" "$r11_target_x" "$r11_target_y" || die "R11 could not move the window"
sleep 0.8
# Captured and checked before it is split, because a here-string always hands read one line.
geometry=$(r11_geometry) || die "R11 window geometry unavailable"
[ -n "$geometry" ] || die "R11 window geometry is empty"
read -r wx wy ww wh floating <<< "$geometry"
check "the window is floating where this case put it" "$floating $wx $wy" "true $r11_target_x $r11_target_y"
point=$(ipc pathCentre) || die "R11 path area has no geometry"
read -r cx cy <<< "$point"
# Inside this window's own chrome, never the shell bar at the top of the screen: the press point is
# the path area's centre mapped through the window's origin, and it is printed so the row can say so.
check "the press lands inside the window" \
      "$([ "$cy" -lt "$(ipc chromeHeight)" ] && [ "$((wy + cy))" -gt "$wy" ] && echo inside || echo "outside at $((wy + cy))")" "inside"
note "press at $((wx + cx)),$((wy + cy)) with the window at $wx,$wy and its chrome $(ipc chromeHeight) tall"
glide_to "$((wx + cx))" "$((wy + cy))"
sleep 0.3
press
sleep 0.3
for _ in $(seq 1 12); do move_rel 8 4; sleep 0.05; done
sleep 0.4
release
sleep 1
geometry=$(r11_geometry) || die "R11 window geometry unavailable after the drag"
[ -n "$geometry" ] || die "R11 window geometry is empty after the drag"
read -r ax ay _ _ _ <<< "$geometry"
check "the window followed the pointer" "$([ "$ax" -gt "$wx" ] && [ "$ay" -gt "$wy" ] && echo moved || echo "stayed at $ax,$ay")" "moved"
note "moved dx=$((ax - wx)) dy=$((ay - wy))"
# The strip is still a strip: a click that does not travel reaches the control under it.
geometry=$(r11_geometry) || die "R11 window geometry unavailable before the click"
[ -n "$geometry" ] || die "R11 window geometry is empty before the click"
read -r bx by _ _ _ <<< "$geometry"
point=$(ipc chromeButtonCentre arrow-up) || die "R11 the up control has no geometry"
read -r ux uy <<< "$point"
here=$(ipc path)
omarchy-drive click "$((bx + ux))" "$((by + uy))" left >/dev/null || die "R11 could not click the up control"
sleep 0.8
check "a click with no travel still reached the control under the strip" \
      "$([ "$(ipc path)" != "$here" ] && echo climbed || echo "stayed at $(ipc path)")" "climbed"
geometry=$(r11_geometry) || die "R11 window geometry unavailable after the click"
[ -n "$geometry" ] || die "R11 window geometry is empty after the click"
read -r cx2 cy2 _ _ _ <<< "$geometry"
check "and that click moved nothing" "$cx2 $cy2" "$bx $by"

# With the path editor up the same gesture is the editor's own, so the strip stops being a title bar.
native_key -M ctrl -k l -m ctrl
expect_ipc pathBarOpen true
glide_to "$((cx2 + cx))" "$((cy2 + cy))"
sleep 0.3
press
sleep 0.3
for _ in $(seq 1 12); do move_rel 8 4; sleep 0.05; done
sleep 0.4
release
sleep 1
geometry=$(r11_geometry) || die "R11 window geometry unavailable after the editor drag"
[ -n "$geometry" ] || die "R11 window geometry is empty after the editor drag"
read -r ex ey _ _ _ <<< "$geometry"
check "a drag while the path editor is up moves no window" "$ex $ey" "$cx2 $cy2"
# And the editor is still up, so the press was ignored by the strip rather than dismissing it.
expect_ipc pathBarOpen true
native_key -k Escape
expect_ipc pathBarOpen false

hypr_window_float "$r11_addr" "off" || die "R11 could not tile the window"
sleep 0.5
echo

# ---------------------------------------------------------------- outbound
echo
echo "== outbound: one file dragged into a second process =="
# In-window cases never leave the process, so this receiver client proves wl_data_device.start_drag reached another client (a missing window is a failed launch, not a drag that left).
printf 'outbound payload\n' > "$HOMEDIR/outbound.txt"
printf 'inner payload\n' > "$HOMEDIR/inner.txt"
native_key :; sleep 0.3
native_key "$HOMEDIR"; sleep 0.2
native_key -k Return
for i in $(seq 1 40); do
  [ "$(ipc path)" = "$HOMEDIR" ] && [ "$(ipc listInFlight)" = false ] && rowidx outbound.txt >/dev/null 2>&1 && break
  sleep 0.25
done
check "the outbound case is looking at the fixture" "$(ipc path)" "$HOMEDIR"

RECV_LOG=$SB/receiver.log
: > "$RECV_LOG"
# Outbound geometry, named once (receiver size rides FLEA_RECV_W/H), and the expected offer mask (1 is Gdk COPY for the plain lift).
recv_w=420; recv_h=320
recv_x=1100; recv_y=80
flea_x=40; flea_y=80; flea_w=1000; flea_h=720
want_actions=1
FLEA_RECV_W=$recv_w FLEA_RECV_H=$recv_h setsid python3 -B "$repo/tests/drag-receiver.py" "$RECV_LOG" >"$SB/receiver-err.log" 2>&1 &
RECV_PID=$!
RECV_PIDS+=("$RECV_PID")
RECV_ADDR=""
# Sample input, hyprctl clients -j: '[{"pid": 123, "address": "0xabc", "title": "flea-drag-receiver"}]'.
for i in $(seq 1 40); do
  RECV_ADDR=$(hyprctl clients -j | python3 -c '
import json, sys
# Sample input: the plain receiver PID argument is "123".
pid = int(sys.argv[1])
# Sample input: plain receiver clients are [{"pid":123,"address":"0xabc","title":"flea-drag-receiver"}].
hits = [w for w in json.load(sys.stdin) if w.get("pid") == pid]
print(hits[0]["address"] if len(hits) == 1 else "")
' "$RECV_PID") || true
  [ -n "$RECV_ADDR" ] && break
  sleep 0.25
done
if [ -z "$RECV_ADDR" ]; then
  bad "the receiver is absent"
  note "the drag is not claimed to have left"
  note "receiver stderr: $(cat "$SB/receiver-err.log" 2>/dev/null)"
else
  ok "the receiver window is up"
  hypr_window_focus "$RECV_ADDR" || die "outbound could not focus the receiver"
  sleep 0.3
  hypr_window_float "$RECV_ADDR" "on" || die "outbound could not float the receiver"
  sleep 0.3
  hypr_window_resize "$RECV_ADDR" "$recv_w" "$recv_h" || die "outbound could not resize the receiver"
  sleep 0.3
  hypr_window_move "$RECV_ADDR" "$recv_x" "$recv_y" || die "outbound could not move the receiver"
  sleep 0.4
  # Sample input, hyprctl clients -j: '[{"pid": 456, "address": "0xdef"}]'.
  FLEA_ADDR=$(hyprctl clients -j | python3 -c '
import json, sys
# Sample input: the outbound source PID argument is "456".
pid = int(sys.argv[1])
# Sample input: outbound source clients are [{"pid":456,"address":"0xdef"}].
hits = [w for w in json.load(sys.stdin) if w.get("pid") == pid]
print(hits[0]["address"] if len(hits) == 1 else "")
' "$MYPID")
  [[ "$FLEA_ADDR" =~ ^0x[0-9a-fA-F]+$ ]] || die "outbound Flea window address is invalid"
  hypr_window_focus "$FLEA_ADDR" || die "outbound could not focus Flea"
  sleep 0.4
  hypr_window_float "$FLEA_ADDR" "on" || die "outbound could not float Flea"
  sleep 0.4
  hypr_window_resize "$FLEA_ADDR" "$flea_w" "$flea_h" || die "outbound could not resize Flea"
  sleep 0.4
  hypr_window_move "$FLEA_ADDR" "$flea_x" "$flea_y" || die "outbound could not move Flea"
  sleep 0.6
  geometry=$(r11_geometry) || die "outbound window geometry unavailable"
  # Sample input: the outbound window geometry is "40 80 1000 720 true".
  read -r WX WY WW WH _ <<< "$geometry"

  # Edge: a release that stays inside Flea must not be a drop on the receiver, and it still moves.
  centre_into sx sy inner.txt
  centre_into ax ay aaa
  warp "$sx" "$sy"; sleep 0.4
  press; sleep 0.3
  glide_to "$ax" "$ay"; sleep 0.5
  release; sleep 0.5
  wait_for "$HOMEDIR/aaa/inner.txt" present
  check "a release inside Flea still moves the file" \
        "$([ -e "$HOMEDIR/aaa/inner.txt" ] && echo moved || echo missing)" "moved"
  check "and the other process logged nothing" \
        "$(grep -c 'body<<' "$RECV_LOG" || true)" "0"

  centre_into sx sy outbound.txt
  # Sample input, hyprctl clients -j: '{"address": "0xabc", "at": [1100, 80], "size": [420, 320]}'.
  set -- $(hyprctl clients -j | python3 -c '
import json, sys
addr = sys.argv[1]
# Sample input: plain receiver geometry is [{"address":"0xabc","at":[1100,80],"size":[420,320]}].
for w in json.load(sys.stdin):
    if w.get("address") == addr:
        x, y = w["at"]; w_, h = w["size"]
        print(x + w_ // 2, y + h // 2)
        break
' "$RECV_ADDR")
  # A pair holds the receiver centre x and y.
  [ $# -eq 2 ] || die "the receiver $RECV_ADDR has no geometry in hyprctl clients"
  rx=$1; ry=$2
  warp "$sx" "$sy"; sleep 0.4
  press; sleep 0.3
  glide_to "$rx" "$ry"; sleep 0.6
  release; sleep 0.5
  for i in $(seq 1 40); do
    grep -q 'body<<' "$RECV_LOG" && break
    sleep 0.25
  done
  outbound_evidence "$SB/receiver-err.log"
  check "the other process received the file URI" \
        "$(python3 -c '
import pathlib, sys
text = pathlib.Path(sys.argv[1]).read_text() if pathlib.Path(sys.argv[1]).exists() else ""
needle = "file://" 
name = sys.argv[2]
# Sample input: the plain receiver body is "body<<\nfile:///run/home/outbound.txt\n>>".
start = text.find("body<<")
end = text.find(">>", start)
body = text[start:end] if start >= 0 else ""
print("received" if needle in body and name in body else "missing")
' "$RECV_LOG" "outbound.txt")" "received"
  # Sample input, receiver.log: 'actions=1\nformats=text/uri-list\nbody<<\nfile:///x/outbound.txt\n>>'.
  check "the receiver saw the copy-alone offer" \
        "$(grep '^actions=' "$RECV_LOG" | cut -d= -f2)" "$want_actions"
  check "and the original is still in the folder" \
        "$([ -e "$HOMEDIR/outbound.txt" ] && echo kept || echo GONE)" "kept"
  # A Shift lift offers move alone (2 is Gdk MOVE); the receiver finishes MOVE and Flea still keeps the original.
  printf 'shift payload\n' > "$HOMEDIR/outbound-shift.txt"
  for i in $(seq 1 40); do
    rowidx outbound-shift.txt >/dev/null 2>&1 && break
    sleep 0.25
  done
  : > "$RECV_LOG"
  want_actions=2
  # End the first receiver so the Shift lookup can match only its own.
  stop_receiver "$RECV_PID" || die "plain receiver teardown failed"
  FLEA_RECV_W=$recv_w FLEA_RECV_H=$recv_h setsid python3 -B "$repo/tests/drag-receiver.py" "$RECV_LOG" >"$SB/receiver-shift-err.log" 2>&1 &
  RECV_PID=$!
  RECV_PIDS+=("$RECV_PID")
  RECV_ADDR=""
  # Sample input, hyprctl clients -j: '[{"pid": 123, "address": "0xabc", "title": "flea-drag-receiver"}]'.
  for i in $(seq 1 40); do
    RECV_ADDR=$(hyprctl clients -j | python3 -c '
import json, sys
# Sample input: the Shift receiver PID argument is "789".
pid = int(sys.argv[1])
# Sample input: Shift receiver clients are [{"pid":789,"address":"0xghi","title":"flea-drag-receiver"}].
hits = [w for w in json.load(sys.stdin) if w.get("pid") == pid]
print(hits[0]["address"] if len(hits) == 1 else "")
' "$RECV_PID") || true
    [ -n "$RECV_ADDR" ] && break
    sleep 0.25
  done
  if [ -z "$RECV_ADDR" ]; then
    bad "the Shift receiver is absent"
  else
    hypr_window_focus "$RECV_ADDR" || die "outbound could not focus the Shift receiver"
    sleep 0.3
    hypr_window_float "$RECV_ADDR" "on" || die "outbound could not float the Shift receiver"
    sleep 0.3
    hypr_window_resize "$RECV_ADDR" "$recv_w" "$recv_h" || die "outbound could not resize the Shift receiver"
    sleep 0.3
    hypr_window_move "$RECV_ADDR" "$recv_x" "$recv_y" || die "outbound could not move the Shift receiver"
    sleep 0.4
    hypr_window_focus "$FLEA_ADDR" || die "outbound could not focus Flea after placing the Shift receiver"
    sleep 0.4
    centre_into sx sy outbound-shift.txt
    # Sample input, hyprctl clients -j: '{"address": "0xabc", "at": [1100, 80], "size": [420, 320]}'.
    set -- $(hyprctl clients -j | python3 -c '
import json, sys
addr = sys.argv[1]
# Sample input: Shift receiver geometry is [{"address":"0xghi","at":[1100,80],"size":[420,320]}].
for w in json.load(sys.stdin):
    if w.get("address") == addr:
        x, y = w["at"]; w_, h = w["size"]
        print(x + w_ // 2, y + h // 2)
        break
' "$RECV_ADDR")
    # A pair holds the Shift receiver centre x and y.
    [ $# -eq 2 ] || die "the Shift receiver $RECV_ADDR has no geometry in hyprctl clients"
    rx=$1
    ry=$2
    warp "$sx" "$sy"
    sleep 0.4
    shift_down
    sleep 0.2
    press
    sleep 0.3
    glide_to "$rx" "$ry"
    sleep 0.6
    release
    sleep 0.5
    shift_up
    sleep 0.2
    for i in $(seq 1 40); do
      grep -q 'body<<' "$RECV_LOG" && break
      sleep 0.25
    done
    outbound_evidence "$SB/receiver-shift-err.log"
    check "the other process received the Shift-dragged file URI" \
          "$(python3 -c '
import pathlib, sys
text = pathlib.Path(sys.argv[1]).read_text() if pathlib.Path(sys.argv[1]).exists() else ""
needle = "file://"
name = sys.argv[2]
# Sample input: the Shift receiver body is "body<<\nfile:///run/home/outbound-shift.txt\n>>".
start = text.find("body<<")
end = text.find(">>", start)
body = text[start:end] if start >= 0 else ""
print("received" if needle in body and name in body else "missing")
' "$RECV_LOG" "outbound-shift.txt")" "received"
    # Sample input, receiver.log: 'actions=2\nformats=text/uri-list\nbody<<\nfile:///x/outbound-shift.txt\n>>'.
    check "the receiver saw the move-alone offer" \
          "$(grep '^actions=' "$RECV_LOG" | cut -d= -f2)" "$want_actions"
    check "and the original is still in the folder after a MOVE finish" \
          "$([ -e "$HOMEDIR/outbound-shift.txt" ] && echo kept || echo GONE)" "kept"
  fi
  if grep -q "Couldn't start a drag because the origin window could not be found." "$SB/flea.log"; then
    printf 'DRAG_OUTBOUND record=missing-origin\n'
  elif grep -q 'start_drag' "$SB/flea.log"; then
    printf 'DRAG_OUTBOUND record=start_drag\n'
  else
    printf 'DRAG_OUTBOUND record=no-start-line\n'
  fi
fi

stop_receivers || die "outbound receiver teardown failed"
assert_receivers_gone || die "outbound receiver drain failed"
printf 'DRAG_SHARED routes=List-Grid,Grid-activeColumns,dual-left-right,dual-right-left real_relative_input=ok index_only=not_exercised transfer_preemption=not_exercised\n'
echo "$((pass + fail)) checks, $fail failed"
[ "$fail" = 0 ] || exit 1
