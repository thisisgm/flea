#!/bin/bash
# Runs the pure JavaScript suites under qml6, with no Quickshell and no window.
set -u
cd "$(dirname "$0")/.." || exit 1

if [ "$#" -gt 1 ]; then
  echo "js.sh: expected at most one suite name"
  exit 2
fi
if [ "$#" -eq 1 ]; then
  case "$1" in
    "") echo "js.sh: suite name cannot be empty"; exit 2 ;;
    --) echo "js.sh: -- is not a suite name"; exit 2 ;;
  esac
fi

if ! command -v qml6 >/dev/null; then
  echo "js.sh: qml6 is not installed, cannot run the pure JavaScript suites"
  exit 1
fi

run() {
  local suite_args=()
  local out code
  [ -n "${2:-}" ] && suite_args=(-- "$2")
  out=$(TZ="$1" QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 timeout 120 qml6 tests/js/harness.qml "${suite_args[@]}" 2>&1)
  code=$?
  echo "$out"
  if [ "$code" = 124 ]; then
    echo "js.sh: the harness did not finish inside 120s, which a load error does"
    exit 1
  fi
  # qml6 exits 0 on a ReferenceError inside an imported library, so the tally proves execution.
  if ! printf '%s' "$out" | grep -q "checks, "; then
    echo "js.sh: the harness reported no tally at all, so nothing ran"
    exit 1
  fi
  printf '%s' "$out" | grep -qE "checks, 0 failed" || exit 1
  [ "$code" = 0 ] || exit "$code"
}

# Caller controls the standard battery's timezone; unattended runs default to positive-offset Tokyo.
normal_timezone=${TZ-Asia/Tokyo}
if [ "$#" -eq 1 ]; then
  run "$normal_timezone" "$1"
  exit 0
fi

run "$normal_timezone"
# Reporter cases run under Edmonton, while DST-only fixtures run under New York.
run America/Edmonton edmonton
run America/New_York dst
