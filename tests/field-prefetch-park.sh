#!/bin/bash
# Gates tools/flea-field-bench's prefetch park and restore on a scratch HOME: the operator's list leaves for the run and comes back unchanged.
set -u
cd "$(dirname "$0")/.." || exit 1
bench="$PWD/tools/flea-field-bench"
export TEST_READY_POLLS=200
export TEST_READY_POLL_S=0.01
export TEST_CHILD_SLEEP_S=300

# The fixture root is a mktemp -d of its own, and its one delete is checked absolute and two deep before the trap exists.
scratch=$(mktemp -d /tmp/flea-prefetch-park.XXXXXX) || exit 1
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep"; exit 1 ;;
esac
: > "$scratch/.flea-test-sandbox"
trap 'case $scratch in /tmp/?*) [ -f "$scratch/.flea-test-sandbox" ] && rm -rf -- "$scratch" ;; esac' EXIT

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

block=$(sed -n '/^# prefetch park begin$/,/^# prefetch park end$/p' "$bench")
[ -n "$block" ] || { echo "FAIL: no prefetch park block in $bench"; exit 1; }
# The globals the block reads, as the bench names them, evaluated under a HOME of the test's own.
globals=$(grep -E '^(PREFETCH|PREFETCH_PARKED_PREFIX|PREFETCH_PARKED|PREFETCH_PARKED_OK)=' "$bench")
check "the bench names the four prefetch globals" 4 "$(printf '%s\n' "$globals" | grep -c .)"

# One case per subshell, so a fail() inside the block ends that case and not the suite.
run_case() {
  ( HOME="$scratch/home-$1"; mkdir -p "$HOME/.cache/flea"
    fail() { printf 'refused: %s\n' "$*"; exit 3; }
    eval "$globals"; eval "$block"; shift; "$@" )
}

# The operator had a list: parked during the run, the warm launch's list replaces it, and the restore brings back the original bytes.
op_case() {
  printf 'operator list\n/usr/bin/flea 0 4096\n' > "$PREFETCH"
  park_prefetch
  printf 'parked=%s present=%s ' "$(head -1 "$PREFETCH_PARKED")" "$([ -e "$PREFETCH" ] && echo yes || echo no)"
  printf 'arm list\n' > "$PREFETCH"
  restore_prefetch
  printf 'restored=%s leftover=%s' "$(head -1 "$PREFETCH")" "$(ls "$HOME/.cache/flea" | grep -c parked)"
}
check "an operator list is parked, then restored unchanged" "parked=operator list present=no restored=operator list leftover=0" "$(run_case a op_case)"

# The operator had none: the run's list is removed at the end, so no tested Flea's list outlives the run.
none_case() {
  park_prefetch
  printf 'arm list\n' > "$PREFETCH"
  restore_prefetch
  printf 'present=%s' "$([ -e "$PREFETCH" ] && echo yes || echo no)"
}
check "with no operator list the run leaves none behind" "present=no" "$(run_case b none_case)"

# A list parked by a killed run is never parked over.
orphan_case() {
  printf 'older operator list\n' > "${PREFETCH_PARKED_PREFIX}1"
  printf 'newer\n' > "$PREFETCH"
  park_prefetch
  echo "parked anyway"
}
out=$(run_case c orphan_case); rc=$?
check "an orphaned park refuses the run" 3 "$rc"
check "the refusal names the orphan" 1 "$(printf '%s\n' "$out" | grep -c 'prefetch-parked-1 was parked by an invocation that was killed')"
check "the orphan and the live list are untouched" "older operator list|newer" "$(cat "$scratch/home-c/.cache/flea/prefetch-parked-1")|$(cat "$scratch/home-c/.cache/flea/prefetch")"

# A restore with nothing parked, as the EXIT trap runs it after an early failure, touches nothing.
early_case() {
  printf 'operator list\n' > "$PREFETCH"
  restore_prefetch
  printf 'kept=%s' "$(cat "$PREFETCH")"
}
check "a restore before any park keeps the operator's list" "kept=operator list" "$(run_case d early_case)"

# A failed restore is refused once, not again from the EXIT trap; a directory in the list's place fails the mv even for root.
retry_case() {
  printf 'operator list\n' > "$PREFETCH"
  park_prefetch
  mkdir -p "$PREFETCH/in-the-way"
  ( fail() { echo refused; exit 3; }; trap 'restore_prefetch 2>/dev/null' EXIT; restore_prefetch 2>/dev/null )
  printf 'kept=%s\n' "$(cat "$PREFETCH_PARKED")"
}
out=$(run_case e retry_case)
check "a failed restore is refused once" 1 "$(printf '%s\n' "$out" | grep -c '^refused$')"
check "a failed restore leaves the operator's list parked" 1 "$(printf '%s\n' "$out" | grep -c '^kept=operator list$')"

# The cache home guard, as the bench names it: unset or ~/.cache runs, anywhere else refuses before anything is parked.
guard=$(grep -E '^\[ "\$\{XDG_CACHE_HOME:-' "$bench")
check "the bench has one cache home guard" 1 "$(printf '%s\n' "$guard" | grep -c .)"
cache_home() { ( HOME="$scratch/home-f"; fail() { exit 3; }; if [ -n "$1" ]; then XDG_CACHE_HOME="$1"; else unset XDG_CACHE_HOME; fi; eval "$guard" ); echo $?; }
check "an unset cache home runs" 0 "$(cache_home '')"
check "a cache home of ~/.cache runs" 0 "$(cache_home "$scratch/home-f/.cache")"
check "a cache home elsewhere refuses" 3 "$(cache_home "$scratch/elsewhere")"

# The wiring: parked beside the thumbnails and before the warm launch, restored on both exits, and the warm launch must have recorded a list.
park_line=$(grep -n '^park_prefetch$' "$bench" | cut -d: -f1)
warm_line=$(grep -n '^# warm jobs begin$' "$bench" | cut -d: -f1)
check "park_prefetch runs once at setup" 1 "$(printf '%s\n' "$park_line" | grep -c .)"
check "each Flea build is warmed once, the flea and the anchor" 2 "$(grep -c 'wait "$WARM_PID" || exit 1$' "$bench")"
check "the list is parked before the first warm launch records one" yes "$([ "${park_line:-0}" -gt 0 ] && [ "${park_line:-0}" -lt "${warm_line:-0}" ] && echo yes || echo no)"
# run_guarded's calls end in ;; and run at launch time, so only the setup calls order against the park.
first_prefetch_line=$(grep -n 'prefetch_for "\$' "$bench" | grep -v ';;' | head -1 | cut -d: -f1)
check "the list is parked before the first prefetch_for" yes "$([ "${park_line:-0}" -gt 0 ] && [ "${park_line:-0}" -lt "${first_prefetch_line:-0}" ] && echo yes || echo no)"
check "restore_prefetch runs at the end" 1 "$(grep -c '^restore_prefetch$' "$bench")"
check "the EXIT trap restores in a subshell, so a failure there skips no later step" 1 "$(grep -c '^  ( restore_prefetch ) || cleanup_rc=1$' "$bench")"
check "the warm launch refuses to leave no list" 1 "$(grep -c 'the warm launch recorded no prefetch list' "$bench")"

# Signals must stop the warm shell and its group both before Flea starts and while it polls a mapped Flea.
export WARM_TEXT=$(sed -n '/^# warm launch begin$/,/^# warm launch end$/p' "$bench") SWEEP_TEXT=$(sed -n '/^# entrant sweep begin$/,/^# entrant sweep end$/p' "$bench")
export CLEAN_TEXT=$(sed -n '/^clean_bench() {$/,/^}$/p' "$bench") CLEANUP_TEXT=$(sed -n '/^bench_cleanup() {$/,/^}$/p' "$bench")
export WARM_START_TEXT=$(sed -n '/^# warm jobs begin$/,/^# warm jobs end$/p' "$bench")
# Sample input: ps -o stat= -p 4242 prints "S" for a live process or "Z" for a process that has ended but is not yet reaped.
running() {
  local state
  [[ "$1" =~ ^[0-9]+$ ]] && [ "$1" -gt 1 ] || return 1
  state=$(ps -o stat= -p "$1" 2>/dev/null)
  [[ -n "$state" && "$state" != *Z* ]]
}
group_running() {
  local pid
  for pid in $(pgrep -g "$1"); do running "$pid" && return 0; done
  return 1
}
signal_case() {
  local mode=$1 signal=$2 expected=$3 warm="$scratch/warm-$1-$2" warm_pid fake copy_pid rc elapsed alive group_alive
  mkdir -p "$warm/cg" "$warm/qml" "$warm/state/flea"
  printf '#!/bin/bash\necho $$ > "$FAKE_PIDFILE"\n: > "$QML_DISK_CACHE_PATH/unit"\necho list > "$FAKE_PREFETCH"\nexec sleep "$TEST_CHILD_SLEEP_S"\n' > "$warm/flea"; chmod +x "$warm/flea"
  printf '#!/bin/bash\ntrap "" TERM\necho $$ > "$W/copy-pid"\nwhile :; do sleep 1; done\n' > "$warm/copy.sh"
  cat > "$warm/run.sh" <<'EOS'
set -uo pipefail
fail() { printf 'FAIL: %s\n' "$*" >&2; exit 1; }
FIELD_FRONTEND=qml; FLEA_FRONTEND=qml
now_ms() { NOW_MS=$(( $(date +%s%N) / 1000000 )); }
wait_for_window() { return 0; }
kill_entrant() { :; }
close_entrant_dialogs() { :; }
watch_kill() { :; }
release_run_cg() { :; }
restore_thumbs() { :; }
restore_prefetch() { printf 'operator list\n' > "$PREFETCH"; }
prefetch_keep() { :; }
prefetch_for() { :; }
only_has() { if [ "$MODE" = anchor-fork ]; then [ "$1" = flea-anchor ]; else [ "$1" = flea ]; fi; }
KILL_TARGETS=(); SWEEP_TERM_POLLS=20; SWEEP_KILL_POLLS=20; TUI_CONF=""
BENCH_TOKEN="warm-test-$$-$RANDOM"; BENCH_CG=$W/cg; FLEA_QMLCACHE=$W/qml; FLEA_STATE=$W/state
FLEA_BIN=$W/flea; FLEA_UI=$W/ui; FIXTURE=$W; PREFETCH=$W/prefetch; ENTRANT_LAUNCHED=""; ROWS_PID=""; WARM_PID=""; WARM_STARTING=""; WARM_PREVIOUS_PID=""
FLEA_PREFETCH_SLOT=""; WITH_ANCHOR=no
if [ "$MODE" = anchor-fork ]; then
  WITH_ANCHOR=yes; ANCHOR_BIN=$FLEA_BIN; ANCHOR_UI=$FLEA_UI; ANCHOR_QMLCACHE=$FLEA_QMLCACHE; ANCHOR_STATE=$FLEA_STATE; ANCHOR_PREFETCH_SLOT=""
fi
export FAKE_PIDFILE=$W/pid FAKE_PREFETCH=$W/prefetch
eval "$WARM_TEXT"; eval "$SWEEP_TEXT"; eval "$CLEAN_TEXT"; eval "$CLEANUP_TEXT"
# The fake cgroup is ordinary files; the real token sweep still ends the independently sessioned Flea.
warm_stop() { :; }
eval "$(declare -f warm_qml_cache | sed '1s/warm_qml_cache/warm_qml_cache_body/')"
warm_qml_cache() { printf '%s\n' "$BASHPID" > "$W/warm-pid"; warm_qml_cache_body; }
eval "$(declare -f stop_warm_job | sed '1s/stop_warm_job/stop_warm_job_body/')"
stop_warm_job() { printf '%s\n' "${WARM_PID:-empty}" > "$W/pid-at-cleanup"; stop_warm_job_body; }
if [ "$MODE" != mapped ]; then cp() { bash "$W/copy.sh"; }; fi
WARM_QUIET_POLLS=600
trap bench_cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
# DEBUG fires immediately before WARM_PID=$!, after the real background fork, without adding a production test hook.
fork_signal() {
  [ "$BASH_COMMAND" = 'WARM_PID=$!' ] || return 0
  local polls=0
  while [ ! -f "$W/copy-pid" ] && [ "$polls" -lt "$TEST_READY_POLLS" ]; do sleep "$TEST_READY_POLL_S"; polls=$(( polls + 1 )); done
  printf '%s\n' "${WARM_PID:-empty}" > "$W/pid-at-signal"
  kill -s "$SIGNAL" "$$"
}
case $MODE in fork|anchor-fork) trap fork_signal DEBUG ;; esac
eval "$WARM_START_TEXT"
EOS
  W=$warm MODE=$mode SIGNAL=$signal python3 - "$signal" <<'PYDRIVER'
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
root = Path(os.environ['W'])
signum = getattr(signal, 'SIG' + sys.argv[1])
with root.joinpath('run.log').open('w') as log:
    child = subprocess.Popen(['bash', str(root / 'run.sh')], stdout=log, stderr=log,
                             preexec_fn=lambda: signal.signal(signal.SIGINT, signal.SIG_DFL))
    root.joinpath('harness-pid').write_text(str(child.pid))
    ready = root / ('pid' if os.environ['MODE'] == 'mapped' else 'copy-pid')
    deadline = time.monotonic() + 10
    while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
        time.sleep(0.05)
    root.joinpath('ready').write_text('yes' if ready.exists() else 'no')
    time.sleep(0.2)
    started = time.monotonic()
    if child.poll() is None and os.environ['MODE'] not in ('fork', 'anchor-fork'):
        child.send_signal(signum)
    try:
        rc = child.wait(timeout=8)
    except subprocess.TimeoutExpired:
        child.kill()
        child.wait()
        rc = 124
    root.joinpath('status').write_text(f'{rc} {int((time.monotonic() - started) * 1000)}')
PYDRIVER
  # Sample input: the Python driver's status file is "143 412", the exit status followed by elapsed milliseconds.
  read -r rc elapsed < "$warm/status" || true
  case $mode in
    fork|anchor-fork)
      check "$signal during $mode is sent from the fork window before WARM_PID assignment" empty "$(cat "$warm/pid-at-signal" 2>/dev/null)"
      check "$signal during $mode reaches EXIT cleanup with WARM_PID empty" empty "$(cat "$warm/pid-at-cleanup" 2>/dev/null)"
      ;;
  esac
  check "$signal during $mode reached the warm job" yes "$(cat "$warm/ready")"
  check "$signal during $mode exits with its signal status" "$expected" "$rc"
  check "$signal during $mode is trapped promptly" yes "$([ "${elapsed:-8000}" -lt 8000 ] && echo yes || echo no)"
  warm_pid=$(cat "$warm/warm-pid" 2>/dev/null)
  if [[ "$warm_pid" =~ ^[0-9]+$ ]] && [ "$warm_pid" -gt 1 ]; then
    alive=no; running "$warm_pid" && alive=yes
    group_alive=no; group_running "$warm_pid" && group_alive=yes
    check "$signal during $mode ends the warm shell" no "$alive"
    check "$signal during $mode ends the warm process group" no "$group_alive"
  else
    check "$signal during $mode recorded a real warm pid" yes no
  fi
  fake=$(cat "$warm/pid" 2>/dev/null)
  if [ "$mode" = mapped ]; then
    alive=no; running "$fake" && alive=yes
    [ "$alive" = yes ] && kill -KILL "$fake" 2>/dev/null
    check "$signal during $mode ends the warm Flea through the EXIT trap" no "$alive"
  else
    copy_pid=$(cat "$warm/copy-pid" 2>/dev/null)
    alive=no; running "$copy_pid" && alive=yes
    check "$signal during $mode kills a setup child that ignores TERM" no "$alive"
    check "$signal during $mode never launches Flea after cleanup" "" "$fake"
  fi
  check "$signal during $mode restores the operator's list unchanged" 'operator list' "$(cat "$warm/prefetch" 2>/dev/null)"
  # Rescue runs after every assertion, so it cannot make an un-killed setup child appear stopped.
  [ "${group_alive:-no}" = yes ] && kill -KILL -- "-$warm_pid" 2>/dev/null
}
signal_case mapped TERM 143
signal_case setup TERM 143
signal_case setup INT 130
signal_case fork TERM 143
signal_case fork INT 130
signal_case anchor-fork TERM 143
signal_case anchor-fork INT 130

# A launch mark before the fork must leave a stale job alone, and a fallback must reject a different command.
cat > "$scratch/fallback-guard.sh" <<'EOS'
set -uo pipefail
eval "$WARM_TEXT"
WARM_PID=""; WARM_STARTING=yes; WARM_PREVIOUS_PID=""
set -m
if [ "$MODE" = previous ]; then ( set +m; sleep "$TEST_CHILD_SLEEP_S"; : ) & else sleep "$TEST_CHILD_SLEEP_S" & fi
pid=$!
set +m
if [ "$MODE" = previous ]; then WARM_PREVIOUS_PID=$pid; fi
polls=0
while [ "$MODE" = command ] && [ "$(cat "/proc/$pid/comm" 2>/dev/null)" != sleep ] && [ "$polls" -lt "$TEST_READY_POLLS" ]; do sleep "$TEST_READY_POLL_S"; polls=$(( polls + 1 )); done
stop_warm_job
kill -0 "$pid" 2>/dev/null && printf 'kept\n' || printf 'killed\n'
[[ "$pid" =~ ^[0-9]+$ ]] && [ "$pid" -gt 1 ] && kill -KILL -- "-$pid" 2>/dev/null
wait "$pid" 2>/dev/null || true
EOS
check "a stale background pid before the warm fork is kept" kept "$(MODE=previous bash "$scratch/fallback-guard.sh" 2>"$scratch/previous.log")"
check "a fallback pid with a different command is kept" kept "$(MODE=command bash "$scratch/fallback-guard.sh" 2>"$scratch/command.log")"

[ "$fail" = 0 ] && echo "field-prefetch-park: all passed" || echo "field-prefetch-park: failures above"
exit "$fail"
