#!/bin/bash
# Runs the pure JavaScript suites under qml6, with no Quickshell and no window.
set -u
cd "$(dirname "$0")/.." || exit 1

if ! command -v qml6 >/dev/null; then
  echo "js.sh: qml6 is not installed, cannot run the pure JavaScript suites"
  exit 1
fi

# qml6 routes console.log to the systemd journal, not stderr, unless told otherwise.
# A broken .import leaves the harness's event loop running with no checks reported, so the suite
# hangs rather than failing. The timeout turns that into a failure, which is what a suite owes.
# TZ is pinned to a non-UTC zone with daylight-saving transitions. That makes the date suite catch
# both a regression to UTC rendering and incorrect day boundaries when the local offset changes.
out=$(TZ=America/New_York QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 timeout 120 qml6 tests/js/harness.qml 2>&1)
code=$?
echo "$out"
if [ "$code" = 124 ]; then
  echo "js.sh: the harness did not finish inside 120s, which a load error does"
  exit 1
fi
# qml6 exits 0 on a ReferenceError inside an imported library, so the tally is what says it ran.
if ! printf '%s' "$out" | grep -q "checks, "; then
  echo "js.sh: the harness reported no tally at all, so nothing ran"
  exit 1
fi
# Anchored on the digit run: "0 failed" is a substring of "10 failed" and of "100 failed", so the
# unanchored form passed exactly the runs it was added to catch.
printf '%s' "$out" | grep -qE "checks, 0 failed" || exit 1
exit $code
