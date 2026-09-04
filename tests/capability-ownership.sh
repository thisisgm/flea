#!/usr/bin/env bash
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1
repo=$PWD
tool="$repo/tools/flea-bench-capability"
test_root=$(mktemp -d) || exit 1
case $test_root in
  /*/*) ;;
  *) printf "FAIL: mktemp -d gave '%s', which is not an absolute path two components deep\n" "$test_root"; exit 1 ;;
esac
failures=0
checks=0
declare -a named_pids=()
declare -A named_comms=()
declare -A test_starts=()
declare -A test_boundaries=()
test_incarnation=0
late_fork_mode=no
rolling_fork_mode=no
test_unit_loaded=no
test_unit_active=no
test_unit_preexisting=no
test_stop_calls=0

cleanup() {
  local pid
  for pid in "${named_pids[@]}"; do
    stop_named "$pid"
  done
  [ ! -e "$test_root/preexisting/nemo" ] || unlink "$test_root/preexisting/nemo"
  [ ! -e "$test_root/post-launch/flea" ] || unlink "$test_root/post-launch/flea"
  [ ! -e "$test_root/post-launch-cleanup" ] || unlink "$test_root/post-launch-cleanup"
  [ ! -e "$test_root/owned/flea" ] || unlink "$test_root/owned/flea"
  [ ! -e "$test_root/late/flea" ] || unlink "$test_root/late/flea"
  [ ! -e "$test_root/absent-session/flea" ] || unlink "$test_root/absent-session/flea"
  [ ! -e "$test_root/session-root/flea" ] || unlink "$test_root/session-root/flea"
  [ ! -e "$test_root/reparented/detached" ] || unlink "$test_root/reparented/detached"
  [ ! -e "$test_root/late-guardian/guardian" ] || unlink "$test_root/late-guardian/guardian"
  [ ! -e "$test_root/late-parent/parent" ] || unlink "$test_root/late-parent/parent"
  [ ! -e "$test_root/late-child/child" ] || unlink "$test_root/late-child/child"
  [ ! -e "$test_root/late-captures" ] || unlink "$test_root/late-captures"
  [ ! -e "$test_root/rolling-guardian/guardian" ] || unlink "$test_root/rolling-guardian/guardian"
  [ ! -e "$test_root/rolling-parent/parent" ] || unlink "$test_root/rolling-parent/parent"
  [ ! -e "$test_root/rolling-child1/child1" ] || unlink "$test_root/rolling-child1/child1"
  [ ! -e "$test_root/rolling-child2/child2" ] || unlink "$test_root/rolling-child2/child2"
  [ ! -e "$test_root/rolling-captures" ] || unlink "$test_root/rolling-captures"
  [ ! -e "$test_root/escape-guardian/guardian" ] || unlink "$test_root/escape-guardian/guardian"
  [ ! -e "$test_root/escape-member/flea" ] || unlink "$test_root/escape-member/flea"
  [ ! -e "$test_root/stale-watch/flea" ] || unlink "$test_root/stale-watch/flea"
  [ ! -e "$test_root/interrupted/flea" ] || unlink "$test_root/interrupted/flea"
  [ ! -e "$test_root/term-state" ] || unlink "$test_root/term-state"
  [ ! -e "$test_root/term-report" ] || unlink "$test_root/term-report"
  [ ! -e "$test_root/preexisting-unit-started" ] || unlink "$test_root/preexisting-unit-started"
  [ ! -d "$test_root/preexisting" ] || rmdir "$test_root/preexisting"
  [ ! -d "$test_root/post-launch" ] || rmdir "$test_root/post-launch"
  [ ! -d "$test_root/owned" ] || rmdir "$test_root/owned"
  [ ! -d "$test_root/late" ] || rmdir "$test_root/late"
  [ ! -d "$test_root/absent-session" ] || rmdir "$test_root/absent-session"
  [ ! -d "$test_root/session-root" ] || rmdir "$test_root/session-root"
  [ ! -d "$test_root/reparented" ] || rmdir "$test_root/reparented"
  [ ! -d "$test_root/late-guardian" ] || rmdir "$test_root/late-guardian"
  [ ! -d "$test_root/late-parent" ] || rmdir "$test_root/late-parent"
  [ ! -d "$test_root/late-child" ] || rmdir "$test_root/late-child"
  [ ! -d "$test_root/rolling-guardian" ] || rmdir "$test_root/rolling-guardian"
  [ ! -d "$test_root/rolling-parent" ] || rmdir "$test_root/rolling-parent"
  [ ! -d "$test_root/rolling-child1" ] || rmdir "$test_root/rolling-child1"
  [ ! -d "$test_root/rolling-child2" ] || rmdir "$test_root/rolling-child2"
  [ ! -d "$test_root/escape-guardian" ] || rmdir "$test_root/escape-guardian"
  [ ! -d "$test_root/escape-member" ] || rmdir "$test_root/escape-member"
  [ ! -d "$test_root/stale-watch" ] || rmdir "$test_root/stale-watch"
  [ ! -d "$test_root/interrupted" ] || rmdir "$test_root/interrupted"
  [ ! -e "$test_root/sleeper" ] || unlink "$test_root/sleeper"
  rmdir "$test_root"
}
trap cleanup EXIT

printf '%s\n' '#include <unistd.h>' 'int main(void) { return sleep(60); }' \
  | cc -x c -o "$test_root/sleeper" - || exit 1

check() {
  local name=$1 expected=$2 actual=$3
  checks=$(( checks + 1 ))
  if [ "$expected" = "$actual" ]; then
    printf 'ok   %s\n' "$name"
  else
    printf 'FAIL %s: expected [%s], got [%s]\n' "$name" "$expected" "$actual"
    failures=$(( failures + 1 ))
  fi
}

named_sleep() {
  local comm=$1 label=$2 path="$test_root/$2/$1"
  mkdir -p "$test_root/$label" || exit 1
  cp "$test_root/sleeper" "$path" || exit 1
  "$path" &
  NAMED_PID=$!
  named_pids+=("$NAMED_PID")
  named_comms[$NAMED_PID]=$comm
  register_named_pid "$NAMED_PID"
  sleep 0.1
}

register_named_pid() {
  local pid=$1
  test_incarnation=$(( test_incarnation + 1 ))
  test_starts[$pid]=$test_incarnation
}

forget_named_pid() {
  local target=$1 pid
  local -a kept=()
  for pid in "${named_pids[@]}"; do
    [ "$pid" = "$target" ] || kept+=("$pid")
  done
  named_pids=("${kept[@]}")
  unset 'named_comms[$target]' 'test_starts[$target]' 'test_boundaries[$target]'
}

stop_named() {
  local pid=$1 command
  if named_running "$pid"; then
    command=$(ps -o command= -p "$pid" 2>/dev/null)
    case "$command" in *"$test_root"*) kill "$pid" 2>/dev/null || true ;; esac
  fi
  wait "$pid" 2>/dev/null || true
  forget_named_pid "$pid"
}

owned_flea_tree() {
  local child
  mkdir -p "$test_root/owned" || exit 1
  cp "$test_root/sleeper" "$test_root/owned/flea" || exit 1
  /bin/sh -c "'$test_root/owned/flea' & wait" >/dev/null 2>&1 &
  OWNED_TEST_ROOT=$!
  named_pids+=("$OWNED_TEST_ROOT")
  register_named_pid "$OWNED_TEST_ROOT"
  sleep 0.1
  child=$(command pgrep -P "$OWNED_TEST_ROOT" | head -1)
  [ -n "$child" ] || exit 1
  OWNED_TEST_BACKEND=$child
  named_pids+=("$OWNED_TEST_BACKEND")
  named_comms[$OWNED_TEST_BACKEND]=flea
  register_named_pid "$OWNED_TEST_BACKEND"
}

named_running() {
  local state
  state=$(ps -o state= -p "$1" 2>/dev/null) || return 1
  case "$state" in
    *Z*) return 1 ;;
  esac
  return 0
}

# Hermetic process lookup exposes only sandbox-named sleeps, never operator processes.
pgrep() {
  local comm=${!#} pid
  for pid in "${named_pids[@]}"; do
    [ "${named_comms[$pid]:-}" = "$comm" ] || continue
    kill -0 "$pid" 2>/dev/null && printf '%s\n' "$pid"
  done
}

fail() {
  printf 'FAIL: %s\n' "$*" >&2
  return 1
}

# Load exact production ownership functions without running live capability setup.
for function_name in \
  process_command process_identity read_unit_properties unit_name_available owned_unit_active \
  read_boundary_pids boundary_contains pids_for_target select_process_targets owned_recorded \
  owned_matches remember_owned_pid owned_alive capture_owned_boundary require_no_foreign_processes \
  start_unit_boundary start_owned_entrant stop_unit_boundary owned_unit_stopped \
  owned_boundary_empty stop_owned_entrants kill_entrants watch_owned remember_watch stop_watches \
  require_flea_enumeration cap_terminate
do
  function_body=$(sed -n "/^${function_name}()/,/^}/p" "$tool")
  [ -z "$function_body" ] || eval "$function_body"
done
unset function_body function_name

process_identity() {
  local pid=$1
  if [ "$late_fork_mode" = yes ] \
    && [ "$pid" = "$late_parent_pid" ] \
    && [ "$(< "$late_capture_file")" -ge 2 ]; then
    command kill "$pid" 2>/dev/null || true
    return 1
  fi
  if [ "$rolling_fork_mode" = yes ]; then
    if [ "$pid" = "$rolling_parent_pid" ] \
      && [ "$(< "$rolling_capture_file")" -ge 2 ]; then
      command kill "$pid" 2>/dev/null || true
      return 1
    fi
    if [ "$pid" = "$rolling_child1_pid" ] \
      && [ "$(< "$rolling_capture_file")" -ge 3 ]; then
      command kill "$pid" 2>/dev/null || true
      return 1
    fi
  fi
  named_running "$pid" || return 1
  PROCESS_STATE=S
  PROCESS_START=${test_starts[$pid]:-missing}
}

read_unit_properties() {
  local unit=$1
  UNIT_LOAD_STATE=not-found
  UNIT_ACTIVE_STATE=inactive
  UNIT_SUB_STATE=dead
  UNIT_MAIN_PID=0
  UNIT_CONTROL_GROUP=
  UNIT_INVOCATION=
  UNIT_KILL_MODE=
  if [ "$test_unit_preexisting" = yes ]; then
    UNIT_LOAD_STATE=loaded
    return 0
  fi
  if [ "$test_unit_loaded" = yes ] && [ "$unit" = "$test_unit_name" ]; then
    UNIT_LOAD_STATE=loaded
    UNIT_ACTIVE_STATE=$test_unit_active
    UNIT_SUB_STATE=running
    UNIT_MAIN_PID=$test_unit_main_pid
    UNIT_CONTROL_GROUP=$test_unit_control_group
    UNIT_INVOCATION=$test_unit_invocation
    UNIT_KILL_MODE=control-group
  fi
}

read_boundary_pids() {
  local pid capture
  if [ "$rolling_fork_mode" = yes ]; then
    capture=$(( $(< "$rolling_capture_file") + 1 ))
    printf '%s\n' "$capture" > "$rolling_capture_file"
    BOUNDARY_PIDS=()
    named_running "$rolling_guardian_pid" && BOUNDARY_PIDS+=("$rolling_guardian_pid")
    if [ "$capture" -le 2 ]; then
      named_running "$rolling_parent_pid" && BOUNDARY_PIDS+=("$rolling_parent_pid")
    elif [ "$capture" -eq 3 ]; then
      named_running "$rolling_child1_pid" && BOUNDARY_PIDS+=("$rolling_child1_pid")
    else
      named_running "$rolling_child2_pid" && BOUNDARY_PIDS+=("$rolling_child2_pid")
    fi
    return 0
  fi
  if [ "$late_fork_mode" = yes ]; then
    capture=$(( $(< "$late_capture_file") + 1 ))
    printf '%s\n' "$capture" > "$late_capture_file"
    BOUNDARY_PIDS=()
    named_running "$late_guardian_pid" && BOUNDARY_PIDS+=("$late_guardian_pid")
    if [ "$capture" -le 2 ]; then
      named_running "$late_parent_pid" && BOUNDARY_PIDS+=("$late_parent_pid")
    else
      named_running "$late_child_pid" && BOUNDARY_PIDS+=("$late_child_pid")
    fi
    return 0
  fi
  BOUNDARY_PIDS=()
  for pid in "${named_pids[@]}"; do
    [ "${test_boundaries[$pid]:-}" = "$OWNED_CONTROL_GROUP" ] || continue
    named_running "$pid" && BOUNDARY_PIDS+=("$pid")
  done
  return 0
}

stop_unit_boundary() {
  local unit=$1 pid
  [ "$unit" = "$test_unit_name" ] || return 1
  test_stop_calls=$(( test_stop_calls + 1 ))
  if [ "$late_fork_mode" = yes ]; then
    command kill "$late_parent_pid" 2>/dev/null || true
  fi
  if [ "$rolling_fork_mode" = yes ]; then
    command kill "$rolling_parent_pid" 2>/dev/null || true
    command kill "$rolling_child1_pid" 2>/dev/null || true
  fi
  for pid in "${named_pids[@]}"; do
    [ "$pid" = "$OWNED_ROOT_PID" ] && continue
    [ "${test_boundaries[$pid]:-}" = "$OWNED_CONTROL_GROUP" ] || continue
    named_running "$pid" && kill "$pid" 2>/dev/null || true
  done
  named_running "$OWNED_ROOT_PID" && kill "$OWNED_ROOT_PID" 2>/dev/null || true
  test_unit_active=inactive
  test_unit_loaded=no
}

owned_boundary_empty() {
  read_boundary_pids || return 1
  [ "${#BOUNDARY_PIDS[@]}" -eq 0 ]
}

begin_owned_boundary() {
  local root=$1
  OWNED_UNIT="test-$root.service"
  OWNED_UNIT_INVOCATION="invocation-${test_starts[$root]}"
  OWNED_CONTROL_GROUP="/test/$root"
  OWNED_ROOT_PID=$root
  test_unit_name=$OWNED_UNIT
  test_unit_invocation=$OWNED_UNIT_INVOCATION
  test_unit_control_group=$OWNED_CONTROL_GROUP
  test_unit_main_pid=$OWNED_ROOT_PID
  test_unit_active=active
  test_unit_loaded=yes
  test_boundaries[$root]=$OWNED_CONTROL_GROUP
}

FLEA_UI="$test_root/ui"
declare -a PROCESS_TARGETS=()
declare -a TARGET_PIDS=()
declare -a OWNED_PIDS=()
declare -a BOUNDARY_PIDS=()
declare -A OWNED_STARTS=()
OWNED_ROOT_PID=
OWNED_UNIT=
OWNED_BOUNDARY_VERIFIED=no
OWNED_UNIT_INVOCATION=
OWNED_CONTROL_GROUP=
OWNED_UNIT_SEQUENCE=0

CAP_ONLY=
declare -F select_process_targets >/dev/null && select_process_targets
unit_start_marker="$test_root/preexisting-unit-started"
(
  fail() { exit 1; }
  start_unit_boundary() { : > "$unit_start_marker"; }
  test_unit_preexisting=yes
  start_owned_entrant 'sleep 60'
)
check "pre-existing unit name refuses launch" 1 "$?"
check "pre-existing unit is never started over" no "$([ -e "$unit_start_marker" ] && printf yes || printf no)"

named_sleep flea post-launch
post_launch_pid=$NAMED_PID
post_launch_report="$test_root/post-launch-cleanup"
(
  fail() { exit 1; }
  start_unit_boundary() {
    test_unit_name=$1
    test_unit_loaded=yes
    test_unit_active=active
    test_unit_main_pid=$post_launch_pid
    test_unit_control_group="/test/$post_launch_pid"
    test_unit_invocation="invocation-${test_starts[$post_launch_pid]}"
    test_boundaries[$post_launch_pid]=$test_unit_control_group
  }
  read_unit_properties() {
    UNIT_LOAD_STATE=not-found
    UNIT_ACTIVE_STATE=inactive
    UNIT_SUB_STATE=dead
    UNIT_MAIN_PID=0
    UNIT_CONTROL_GROUP=
    UNIT_INVOCATION=
    UNIT_KILL_MODE=
    if [ "$test_unit_loaded" = yes ]; then
      UNIT_LOAD_STATE=loaded
      UNIT_ACTIVE_STATE=$test_unit_active
      UNIT_SUB_STATE=running
      UNIT_MAIN_PID=$test_unit_main_pid
      UNIT_CONTROL_GROUP=$test_unit_control_group
      UNIT_INVOCATION=$test_unit_invocation
      UNIT_KILL_MODE=process
    fi
  }
  stop_unit_boundary() {
    [ "$1" = "$test_unit_name" ] || return 1
    command kill "$post_launch_pid" 2>/dev/null || true
    test_unit_loaded=no
  }
  cap_cleanup() {
    local stop_status=0
    stop_owned_entrants || stop_status=$?
    printf '%s %s\n' "$stop_status" "$test_unit_loaded" > "$post_launch_report"
  }
  trap cap_cleanup EXIT
  start_owned_entrant 'sleep 60'
)
post_launch_status=$?
read -r post_launch_cleanup_status post_launch_unit_loaded < "$post_launch_report"
check "failed post-launch verification exits nonzero" 1 "$post_launch_status"
check "failed post-launch cleanup keeps refusal status" 1 "$post_launch_cleanup_status"
check "failed post-launch cleanup collects exact unit" no "$post_launch_unit_loaded"
sleep 0.1
named_running "$post_launch_pid"
check "failed post-launch cleanup stops exact process" 1 "$?"
stop_named "$post_launch_pid"
unlink "$test_root/post-launch/flea"
unlink "$post_launch_report"
rmdir "$test_root/post-launch"

named_sleep nemo preexisting
nemo_pid=$NAMED_PID
kill_entrants >/dev/null 2>&1
preexisting_status=$?
check "full run refuses a pre-existing nemo" 1 "$preexisting_status"
sleep 0.1
named_running "$nemo_pid"
preexisting_alive=$?
check "full run leaves the pre-existing nemo alive" 0 "$preexisting_alive"
stop_named "$nemo_pid"
unlink "$test_root/preexisting/nemo"
rmdir "$test_root/preexisting"

CAP_ONLY=flea
PROCESS_TARGETS=()
OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
OWNED_ROOT_PID=
OWNED_UNIT=
declare -F select_process_targets >/dev/null && select_process_targets
owned_flea_tree
if declare -F remember_owned_pid >/dev/null; then
  begin_owned_boundary "$OWNED_TEST_ROOT"
  test_boundaries[$OWNED_TEST_BACKEND]=$OWNED_CONTROL_GROUP
  capture_owned_boundary
else
  OWNED_PIDS=("$OWNED_TEST_ROOT" "$OWNED_TEST_BACKEND")
fi
owned_matches "$OWNED_TEST_BACKEND"
check "scoped run records its backend descendant" 0 "$?"
named_sleep flea late
flea_pid=$NAMED_PID
kill_entrants >/dev/null 2>&1
late_status=$?
check "scoped run refuses late unowned flea attribution" 1 "$late_status"
sleep 0.1
named_running "$OWNED_TEST_BACKEND"
owned_alive=$?
check "scoped run stops its exact owned flea" 1 "$owned_alive"
named_running "$flea_pid"
late_alive=$?
check "scoped run leaves the late unowned flea alive" 0 "$late_alive"
stop_named "$flea_pid"
stop_named "$OWNED_TEST_ROOT"
stop_named "$OWNED_TEST_BACKEND"
unlink "$test_root/owned/flea"
unlink "$test_root/late/flea"
rmdir "$test_root/owned" "$test_root/late"

PROCESS_TARGETS=("flea|")
OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
named_sleep flea reuse
reuse_pid=$NAMED_PID
begin_owned_boundary "$reuse_pid"
capture_owned_boundary || exit 1
test_starts[$reuse_pid]=reused-incarnation
kill_entrants >/dev/null 2>&1
check "reused session PID refuses production cleanup" 1 "$?"
sleep 0.1
named_running "$reuse_pid"
check "reused session PID is never signaled" 0 "$?"
stop_named "$reuse_pid"
unlink "$test_root/reuse/flea"
rmdir "$test_root/reuse"

OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
named_sleep flea absent-session
absent_member_pid=$NAMED_PID
absent_session=$(( absent_member_pid + 1000000 ))
OWNED_UNIT="test-$absent_session.service"
OWNED_BOUNDARY_VERIFIED=yes
OWNED_UNIT_INVOCATION=old-absent-unit
OWNED_CONTROL_GROUP="/test/$absent_session"
OWNED_ROOT_PID=$absent_session
OWNED_PIDS=("$absent_session")
OWNED_STARTS[$absent_session]=old-absent-leader
test_unit_name=$OWNED_UNIT
test_unit_invocation=$OWNED_UNIT_INVOCATION
test_unit_control_group=$OWNED_CONTROL_GROUP
test_unit_main_pid=$OWNED_ROOT_PID
test_unit_active=active
test_unit_loaded=yes
test_boundaries[$absent_member_pid]=$OWNED_CONTROL_GROUP
kill_entrants >/dev/null 2>&1
check "absent reused session leader refuses production cleanup" 1 "$?"
sleep 0.1
named_running "$absent_member_pid"
check "absent reused session member is never signaled" 0 "$?"
stop_named "$absent_member_pid"
unlink "$test_root/absent-session/flea"
rmdir "$test_root/absent-session"

OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
named_sleep flea session-root
session_root=$NAMED_PID
begin_owned_boundary "$session_root"
named_sleep detached reparented
reparented_pid=$NAMED_PID
test_boundaries[$reparented_pid]=$OWNED_CONTROL_GROUP
capture_owned_boundary
owned_matches "$reparented_pid"
check "reparented session member is captured" 0 "$?"
stop_owned_entrants || exit 1
sleep 0.1
named_running "$reparented_pid"
check "reparented session member is stopped" 1 "$?"
stop_named "$session_root"
stop_named "$reparented_pid"
unlink "$test_root/session-root/flea"
unlink "$test_root/reparented/detached"
rmdir "$test_root/session-root" "$test_root/reparented"

OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
named_sleep guardian late-guardian
late_guardian_pid=$NAMED_PID
begin_owned_boundary "$late_guardian_pid"
named_sleep parent late-parent
late_parent_pid=$NAMED_PID
named_sleep child late-child
late_child_pid=$NAMED_PID
test_boundaries[$late_parent_pid]=$OWNED_CONTROL_GROUP
capture_owned_boundary || exit 1
test_boundaries[$late_child_pid]=$OWNED_CONTROL_GROUP
late_capture_file="$test_root/late-captures"
printf '0\n' > "$late_capture_file"
late_fork_mode=yes
declare -a late_signal_log=()
kill() {
  late_signal_log+=("${!#}")
  command kill "$@"
}
stop_owned_entrants
late_stop_status=$?
late_fork_mode=no
late_signal_order="${late_signal_log[*]}"
unset -f kill
check "late-fork cleanup succeeds" 0 "$late_stop_status"
sleep 0.1
named_running "$late_child_pid"
check "late-fork child is stopped" 1 "$?"
check "late-fork child is signaled before guardian" "$late_child_pid $late_guardian_pid" "$late_signal_order"
stop_named "$late_guardian_pid"
stop_named "$late_parent_pid"
stop_named "$late_child_pid"
unlink "$test_root/late-guardian/guardian"
unlink "$test_root/late-parent/parent"
unlink "$test_root/late-child/child"
unlink "$late_capture_file"
rmdir "$test_root/late-guardian" "$test_root/late-parent" "$test_root/late-child"

OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
named_sleep guardian rolling-guardian
rolling_guardian_pid=$NAMED_PID
begin_owned_boundary "$rolling_guardian_pid"
named_sleep parent rolling-parent
rolling_parent_pid=$NAMED_PID
named_sleep child1 rolling-child1
rolling_child1_pid=$NAMED_PID
named_sleep child2 rolling-child2
rolling_child2_pid=$NAMED_PID
test_boundaries[$rolling_parent_pid]=$OWNED_CONTROL_GROUP
capture_owned_boundary || exit 1
test_boundaries[$rolling_child1_pid]=$OWNED_CONTROL_GROUP
test_boundaries[$rolling_child2_pid]=$OWNED_CONTROL_GROUP
rolling_capture_file="$test_root/rolling-captures"
printf '0\n' > "$rolling_capture_file"
rolling_fork_mode=yes
declare -a rolling_signal_log=()
kill() {
  rolling_signal_log+=("${!#}")
  command kill "$@"
}
stop_owned_entrants
rolling_stop_status=$?
rolling_fork_mode=no
rolling_signal_order="${rolling_signal_log[*]}"
unset -f kill
check "rolling-fork cleanup succeeds" 0 "$rolling_stop_status"
sleep 0.1
named_running "$rolling_child2_pid"
check "rolling-fork final child is stopped" 1 "$?"
check "rolling-fork boundary includes final child" \
  "$rolling_child2_pid $rolling_guardian_pid" "$rolling_signal_order"
stop_named "$rolling_guardian_pid"
stop_named "$rolling_parent_pid"
stop_named "$rolling_child1_pid"
stop_named "$rolling_child2_pid"
unlink "$test_root/rolling-guardian/guardian"
unlink "$test_root/rolling-parent/parent"
unlink "$test_root/rolling-child1/child1"
unlink "$test_root/rolling-child2/child2"
unlink "$rolling_capture_file"
rmdir "$test_root/rolling-guardian" "$test_root/rolling-parent" \
  "$test_root/rolling-child1" "$test_root/rolling-child2"

OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
named_sleep guardian escape-guardian
escape_guardian_pid=$NAMED_PID
begin_owned_boundary "$escape_guardian_pid"
named_sleep flea escape-member
escape_member_pid=$NAMED_PID
test_boundaries[$escape_member_pid]=$OWNED_CONTROL_GROUP
capture_owned_boundary || exit 1
test_boundaries[$escape_member_pid]=/test/escaped
test_stop_calls=0
stop_owned_entrants >/dev/null 2>&1
check "escaped owned member refuses boundary cleanup" 1 "$?"
check "escaped owned member prevents unit stop" 0 "$test_stop_calls"
sleep 0.1
named_running "$escape_member_pid"
check "escaped owned member is never signaled" 0 "$?"
stop_named "$escape_guardian_pid"
stop_named "$escape_member_pid"
unlink "$test_root/escape-guardian/guardian"
unlink "$test_root/escape-member/flea"
rmdir "$test_root/escape-guardian" "$test_root/escape-member"

declare -A WATCH_STARTS=()
named_sleep flea stale-watch
stale_watch_pid=$NAMED_PID
remember_watch "$stale_watch_pid" || exit 1
test_starts[$stale_watch_pid]=reused-watch-incarnation
stop_watches
sleep 0.1
named_running "$stale_watch_pid"
check "stale watcher PID is not signaled" 0 "$?"
stop_named "$stale_watch_pid"
unlink "$test_root/stale-watch/flea"
rmdir "$test_root/stale-watch"

named_sleep flea interrupted
interrupted_pid=$NAMED_PID
OWNED_PIDS=()
BOUNDARY_PIDS=()
OWNED_STARTS=()
begin_owned_boundary "$interrupted_pid"
capture_owned_boundary || exit 1
term_state="$test_root/term-state"
term_report="$test_root/term-report"
(
  cap_cleanup() {
    stop_owned_entrants
    printf 'restored\n' >> "$term_state"
  }
  trap 'cap_cleanup' EXIT
  trap 'cap_terminate 143' TERM
  kill -TERM "$BASHPID"
  printf 'closed\n' > "$term_report"
)
term_status=$?
check "TERM interruption exits nonzero" 143 "$term_status"
check "TERM interruption cleans exactly once" 1 "$(grep -c . "$term_state")"
check "TERM interruption never closes the report" no "$([ -e "$term_report" ] && printf yes || printf no)"
sleep 0.1
named_running "$interrupted_pid"
check "TERM interruption stops owned process" 1 "$?"
stop_named "$interrupted_pid"
unlink "$test_root/interrupted/flea"
unlink "$term_state"
rmdir "$test_root/interrupted"

qs() { return 7; }
require_flea_enumeration >/dev/null 2>&1
check "failed qs enumeration is refused" 1 "$?"

printf '%s checks, %s failed\n' "$checks" "$failures"
exit "$failures"
