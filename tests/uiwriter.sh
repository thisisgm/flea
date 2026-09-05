#!/bin/bash
# Drives ui/ViewState.qml's writer under a real Quickshell: the queue drain and the failure path.
# tests/js/uistate.js can reach neither, because the book alone never sees a Process, and Quickshell
# does not emit exited for a program it could not start at all.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=$PWD/target/debug/flea
SANDBOX=$FIXTURE_ROOT/uiwriter
QMLDIR=$SANDBOX/flea
fail=0

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"
    echo "  expected: $expected"
    echo "  actual:   $actual"
    fail=1
  else
    echo "ok   $label"
  fi
}

if ! command -v qs >/dev/null; then
  echo "uiwriter.sh: qs is not installed, cannot drive the QML writer"
  exit 1
fi
if [ ! -x "$BIN" ]; then
  printf 'uiwriter.sh: no binary at %s\n' "$BIN" >&2
  printf 'uiwriter.sh: build it (cargo build); refusing to report on nothing\n' >&2
  exit 1
fi

# The singleton and its book are copied rather than imported: importing ui/ as a directory makes
# Quickshell scan every file in it and warn about the two OEM symlinks this test has no session for.
sandbox_make "$SANDBOX" || exit 1
mkdir -p "$QMLDIR/js" || exit 1
cp ui/ViewState.qml "$QMLDIR/ViewState.qml" || exit 1
cp ui/js/UiState.js "$QMLDIR/js/UiState.js" || exit 1
printf 'module flea\nsingleton ViewState 1.0 ViewState.qml\n' > "$QMLDIR/qmldir" || exit 1

cat > "$QMLDIR/probe.qml" <<'QML'
import QtQuick
import Quickshell

// Two column toggles in one turn: the first starts a writer and the second queues behind it, so one
// run exercises the start, the drain through onExited and whatever a failed start does instead.
ShellRoot {
    id: root
    property int failures: 0

    property var reporter: Connections {
        target: ViewState
        function onSaveFailed() { root.failures = root.failures + 1 }
    }

    Component.onCompleted: {
        ViewState.toggleColumn("kind")
        ViewState.toggleColumn("mode")
    }

    // Long enough for two writers to run and exit on this box, where one --ui-state costs single
    // digit milliseconds; the suite reads the printed book and never a timing.
    property var settled: Timer {
        interval: 1500
        running: true
        onTriggered: {
            console.log("PROBE failures=" + root.failures)
            console.log("PROBE inflight=[" + ViewState.writeBook.inflight + "]")
            console.log("PROBE pending=[" + ViewState.writeBook.pending + "]")
            Qt.quit()
        }
    }
}
QML

# Each run gets its own empty state home, so the file under test is only ever this run's.
drive() {
  sandbox_scratch "$SANDBOX/state" || exit 1
  env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
      XDG_STATE_HOME="$SANDBOX/state" FLEA_BIN="$1" \
      timeout 60 qs -p "$QMLDIR/probe.qml" 2>&1
}

# A writer that cannot start. Quickshell emits no exited for it, so the book has to learn from the
# only signal there is, and both patches have to end refused rather than one of them stranded.
out=$(drive /nonexistent/flea-uiwriter-test)
check "a writer that never starts is reported to the pane" "1" "$(echo "$out" | grep -c 'PROBE failures=2')"
check "and leaves nothing in flight" "1" "$(echo "$out" | grep -c 'PROBE inflight=\[\]')"
check "and nothing queued behind it" "1" "$(echo "$out" | grep -c 'PROBE pending=\[\]')"
check "and writes no state file at all" "0" "$([ -e "$SANDBOX/state/flea/ui.json" ] && echo 1 || echo 0)"

# The same two toggles against the real binary: one writer at a time, and the queued one drains.
out=$(drive "$BIN")
check "a writer that runs reports nothing to the pane" "1" "$(echo "$out" | grep -c 'PROBE failures=0')"
check "and the queued patch drains through the first writer's exit" "1" "$(echo "$out" | grep -c 'PROBE inflight=\[\]')"
check "and both column changes reached the file" "1" "$(tr -d ' \n' < "$SANDBOX/state/flea/ui.json" 2>/dev/null | grep -c '"columns":\["name","size","date","kind","mode"\]')"

sandbox_remove "$SANDBOX" || exit 1

[ "$fail" -eq 0 ] && echo "uiwriter: all checks passed"
exit $fail
