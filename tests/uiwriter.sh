#!/bin/bash
# Drives ui/ViewState.qml's writer under a real Quickshell: the queue drain, the failure path, and
# two windows saving different settings over one state file. tests/js/uistate.js can reach none of
# them, because the book alone never sees a Process, Quickshell does not emit exited for a program it
# could not start at all, and a lost update needs two processes holding two reads of one file.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=$PWD/target/debug/flea
SANDBOX=$FIXTURE_ROOT/uiwriter-$$
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

# The singleton and the four libraries it imports are copied rather than imported: importing ui/ as
# a directory makes Quickshell scan every file in it and warn about the two OEM symlinks this test
# has no session for. Commons is one of those two, and ViewState reads Omarchy's own base size from
# it, so this root gets that link alone rather than the whole directory.
sandbox_make "$SANDBOX" || exit 1
mkdir -p "$QMLDIR/js" || exit 1
cp ui/ViewState.qml "$QMLDIR/ViewState.qml" || exit 1
# The transitive set, not just what ViewState.qml names: Settings.js imports Places.js, which
# imports Mounts.js, which imports Protocols.js. Copying only the four ViewState names left the
# singleton unloadable, so every probe below printed nothing and every check read it as a failure.
for lib in UiState Settings TextSize Keymap Places Mounts Protocols; do
  cp "ui/js/$lib.js" "$QMLDIR/js/$lib.js" || exit 1
done
ln -sfn /usr/share/omarchy/shell/Commons "$QMLDIR/Commons" || exit 1
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

# Two settings changed in one turn: the second has to queue behind the first carrying BOTH, because
# a patch naming only the newest would drop the first if the writer under it were refused.
cat > "$QMLDIR/union.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root

    Component.onCompleted: {
        ViewState.toggleColumn("kind")
        ViewState.setKeysPreset("windows")
        console.log("PROBE queued=" + ViewState.writeBook.pending)
    }

    property var settled: Timer {
        interval: 1500
        running: true
        onTriggered: {
            console.log("PROBE inflight=[" + ViewState.writeBook.inflight + "]")
            console.log("PROBE pending=[" + ViewState.writeBook.pending + "]")
            Qt.quit()
        }
    }
}
QML

# A refused write leaves its setting owed, so the next change carries it again rather than dropping
# the one the pane has already reported; and a landed write clears its own, so the change after it
# names only itself. One probe for both halves: which one runs is decided by the binary it is given.
cat > "$QMLDIR/owed.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root

    property int failures: 0

    property var reporter: Connections {
        target: ViewState
        function onSaveFailed() { root.failures = root.failures + 1 }
    }

    Component.onCompleted: ViewState.setKeysPreset("windows")

    // Long enough for the first writer to have exited or failed to start on this box, where one
    // --ui-state costs single-digit milliseconds; the suite reads the printed patch and no timing.
    property var second: Timer {
        interval: 700
        running: true
        onTriggered: {
            console.log("PROBE failures=" + root.failures)
            ViewState.setTextSize({ mode: 16 })
            console.log("PROBE second=" + ViewState.writeBook.inflight)
            done.running = true
        }
    }

    property var done: Timer {
        interval: 700
        onTriggered: Qt.quit()
    }
}
QML

# One window of the two-window case. It loads ui.json at startup and either changes its own setting
# at once or waits on a trigger file first, which is how the suite makes one window's read older than
# the other window's write without either of them ever re-reading the file.
cat > "$QMLDIR/two.qml" <<'QML'
import QtQuick
import Quickshell
import Quickshell.Io

ShellRoot {
    id: root

    readonly property string mine: Quickshell.env("PROBE_CHANGE")
    readonly property string trigger: Quickshell.env("PROBE_TRIGGER") || ""

    property int failures: 0

    property var reporter: Connections {
        target: ViewState
        function onSaveFailed() { root.failures = root.failures + 1 }
    }

    function write() {
        if (root.mine === "display")
            ViewState.setTextSize({ mode: 16 })
        else
            ViewState.setKeysPreset("windows")
        console.log("PROBE sent=" + ViewState.writeBook.inflight)
        settled.running = true
    }

    Component.onCompleted: {
        console.log("PROBE loaded keys=" + ViewState.keysPreset)
        if (root.trigger.length === 0)
            root.write()
        else
            waiter.running = true
    }

    // ui/ViewState.qml's FileView is watchChanges: false by design, so this window keeps the read it
    // took at startup; the suite releases the trigger only once the other window's change is in the
    // file, which is the lost update's own ordering and not a sleep hoping for it.
    property var waiter: Process {
        command: ["sh", "-c", "while [ ! -e \"$PROBE_TRIGGER\" ]; do sleep 0.05; done"]
        onExited: root.write()
    }

    property var settled: Timer {
        interval: 1200
        onTriggered: {
            console.log("PROBE failures=" + root.failures)
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
      timeout 60 qs -p "$QMLDIR/${2:-probe.qml}" 2>&1
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

# Two settings in one turn, and the second queues behind the first carrying both. The queued patch
# is the union of what changed and not the newest key alone, so a refusal under it loses neither.
out=$(drive "$BIN" union.qml)
# Quickshell prefixes every console.log with a coloured " DEBUG qml: ", so nothing here is anchored,
# and each captured line is checked for existence before it is read: a grep over a line the probe
# never printed counts zero and passes any check looking for zero.
queued=$(echo "$out" | grep 'PROBE queued=' | head -1)
check "the probe printed what it queued" "1" "$([ -n "$queued" ] && echo 1 || echo 0)"
check "the second setting queues behind the running writer" "1" "$(echo "$queued" | grep -c '"keys":"windows"')"
check "and the queued patch still carries the first" "1" "$(echo "$queued" | grep -c '"columns"')"
check "the queue drains" "1" "$(echo "$out" | grep -c 'PROBE inflight=\[\]')"
check "and nothing is left waiting" "1" "$(echo "$out" | grep -c 'PROBE pending=\[\]')"
state_flat=$(tr -d ' \n' < "$SANDBOX/state/flea/ui.json" 2>/dev/null)
check "the column change reached the file" "1" "$(echo "$state_flat" | grep -c '"columns":\["name","size","date","kind"\]')"
check "and the preset beside it did too" "1" "$(echo "$state_flat" | grep -c '"keys":"windows"')"

# A writer that never started leaves its setting owed: the change after it has to carry both, or the
# refused one is lost with nothing but a status-bar sentence to say it ever existed.
out=$(drive /nonexistent/flea-uiwriter-test owed.qml)
refused_second=$(echo "$out" | grep 'PROBE second=' | head -1)
check "the probe printed the patch after the refusal" "1" "$([ -n "$refused_second" ] && echo 1 || echo 0)"
check "the refusal is reported before the next change" "1" "$(echo "$out" | grep -c 'PROBE failures=1')"
check "the patch after a refusal still carries the refused setting" "1" "$(echo "$refused_second" | grep -c '"keys":"windows"')"
check "and the setting the change itself made" "1" "$(echo "$refused_second" | grep -c '"display"')"

# The other half: a write that LANDS clears what it carried, so the next change names only itself.
# Without that, every later patch keeps this window's own read of every key it has ever written, and
# that read is exactly what another window's change would be overwritten by.
out=$(drive "$BIN" owed.qml)
landed_second=$(echo "$out" | grep 'PROBE second=' | head -1)
check "the probe printed the patch after the landed write" "1" "$([ -n "$landed_second" ] && echo 1 || echo 0)"
check "a landed write is not reported" "1" "$(echo "$out" | grep -c 'PROBE failures=0')"
check "the change after a landed write names its own setting" "1" "$(echo "$landed_second" | grep -c '"display"')"
check "and nothing else" "0" "$(echo "$landed_second" | grep -c '"keys"')"
state_flat=$(tr -d ' \n' < "$SANDBOX/state/flea/ui.json" 2>/dev/null)
check "both settings are in the file" "1" "$(echo "$state_flat" | grep -c '"keys":"windows"')"
check "including the one written second" "1" "$(echo "$state_flat" | grep -c '"textSize":{"mode":16}')"

# A ui.json a newer Flea wrote carries sub-keys this one has no rule for, and both the settle and the
# merge keep them, so the window reads them. A patch has to carry the leaf its writer changed and
# never the group that leaf was merged into: src/uistate.rs refuses display.aKeyThisBuildHasNeverHeardOf
# and refuses the whole patch with it, so the change would be lost and the pane would say so.
sandbox_scratch "$SANDBOX/newer" || exit 1
mkdir -p "$SANDBOX/newer/state/flea" || exit 1
printf '%s\n' '{"keys":"mac","display":{"textSize":{"mode":"system"},"aKeyThisBuildHasNeverHeardOf":true}}' \
  > "$SANDBOX/newer/state/flea/ui.json" || exit 1
out=$(env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/newer/state" \
      FLEA_BIN="$BIN" PROBE_CHANGE=display PROBE_TRIGGER="" timeout 60 qs -p "$QMLDIR/two.qml" 2>&1)
newer_sent=$(echo "$out" | grep 'PROBE sent=' | head -1)
check "the probe printed the patch it sent beside a newer Flea's sub-key" "1" "$([ -n "$newer_sent" ] && echo 1 || echo 0)"
check "and that patch never names the sub-key" "0" "$(echo "$newer_sent" | grep -c 'NeverHeardOf')"
check "so nothing was refused" "1" "$(echo "$out" | grep -c 'PROBE failures=0')"
newer_flat=$(tr -d ' \n' < "$SANDBOX/newer/state/flea/ui.json" 2>/dev/null)
check "the text size reached the file" "1" "$(echo "$newer_flat" | grep -c '"textSize":{"mode":16}')"
check "and the newer Flea's sub-key survived beside it" "1" "$(echo "$newer_flat" | grep -c 'aKeyThisBuildHasNeverHeardOf')"

# Two windows over one state file, which is the lost update itself. The waiting window reads first,
# the other window changes a different setting and that change lands, and only then does the waiting
# window save. What it must not do is write the value it read at startup back over that change.
# ui/ViewState.qml's FileView never re-reads, so the merge in src/uistate.rs is the only thing
# between the two, and a merge cannot protect a key the caller overwrites by name.
SEED='{"columns":["name","size","date"],"keys":"mac","display":{"textSize":{"mode":"system"}},"menu":{"hidden":["delete"]}}'

two_windows() {
  local waits="$1" acts="$2" state=$SANDBOX/two/state trigger=$SANDBOX/two/trigger waited=0
  # Cleared first, so an early return leaves the checks below reading nothing rather than the
  # previous pair's file and the previous pair's patch.
  two_flat=""
  two_sent=""
  sandbox_scratch "$SANDBOX/two" || exit 1
  mkdir -p "$state" || exit 1
  env XDG_STATE_HOME="$state" "$BIN" --ui-state "$SEED" >/dev/null 2>&1 \
    || { echo "FAIL two windows: the seed write failed"; fail=1; return 1; }
  env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$state" \
      FLEA_BIN="$BIN" PROBE_CHANGE="$waits" PROBE_TRIGGER="$trigger" \
      timeout 60 qs -p "$QMLDIR/two.qml" > "$SANDBOX/two/waiting.log" 2>&1 &
  # The only process this block kills is the one it started, and it is waited for rather than killed:
  # the probe quits itself and timeout bounds it, so no pattern over anyone else's processes is used.
  waiting_pid=$!
  until grep -q 'PROBE loaded' "$SANDBOX/two/waiting.log" 2>/dev/null; do
    waited=$((waited + 1))
    if [ "$waited" -gt 600 ]; then
      echo "FAIL two windows: the waiting window never reported a read"
      fail=1
      kill "$waiting_pid" 2>/dev/null
      wait "$waiting_pid" 2>/dev/null
      return 1
    fi
    sleep 0.05
  done
  env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$state" \
      FLEA_BIN="$BIN" PROBE_CHANGE="$acts" PROBE_TRIGGER="" \
      timeout 60 qs -p "$QMLDIR/two.qml" > "$SANDBOX/two/acting.log" 2>&1
  # The ordering is proven and not assumed: the trigger is only released once the acting window's
  # change is in the file, so the waiting window's save is the later of the two and its read the older.
  if ! grep -q '"mode": 16\|"keys": "windows"' "$state/flea/ui.json" 2>/dev/null; then
    echo "FAIL two windows: the acting window's change never reached the file"
    fail=1
  fi
  : > "$trigger" || exit 1
  wait "$waiting_pid"
  two_flat=$(tr -d ' \n' < "$state/flea/ui.json" 2>/dev/null)
  two_sent=$(grep 'PROBE sent=' "$SANDBOX/two/waiting.log" | head -1)
  # Printed, so what is said about this pair is read off the run rather than off the check labels.
  echo "     the waiting window changed $waits and sent ${two_sent##*PROBE sent=}"
  echo "     and the file then held $two_flat"
}

# The waiting window saves a text size; the other window's preset change has to survive it.
two_windows display keys
check "the waiting window printed the patch it sent" "1" "$([ -n "$two_sent" ] && echo 1 || echo 0)"
check "the acting window's preset is in the file" "1" "$(echo "$two_flat" | grep -c '"keys":"windows"')"
check "and the waiting window's text size is too" "1" "$(echo "$two_flat" | grep -c '"textSize":{"mode":16}')"
check "the waiting window sent its own setting" "1" "$(echo "$two_sent" | grep -c '"display"')"
check "and never named the one it only read" "0" "$(echo "$two_sent" | grep -c 'keys')"
check "the hidden menu action nobody touched survives" "1" "$(echo "$two_flat" | grep -c '"hidden":\["delete"\]')"
check "and the column set nobody touched survives" "1" "$(echo "$two_flat" | grep -c '"columns":\["name","size","date"\]')"

# And the other way round, because a fix that only holds in one direction is not the invariant.
two_windows keys display
check "the waiting window printed the patch it sent" "1" "$([ -n "$two_sent" ] && echo 1 || echo 0)"
check "the acting window's text size is in the file" "1" "$(echo "$two_flat" | grep -c '"textSize":{"mode":16}')"
check "and the waiting window's preset is too" "1" "$(echo "$two_flat" | grep -c '"keys":"windows"')"
check "the waiting window sent its own setting" "1" "$(echo "$two_sent" | grep -c '"keys":"windows"')"
check "and never named the one it only read" "0" "$(echo "$two_sent" | grep -c 'display')"

# The temporal half of the lost update, which a narrower patch cannot close: the window records what
# a writer stored only when the whole queue drains, so a setting that has ALREADY landed stays owed
# and rides along inside every patch queued behind it. The wrapper below is what makes the ordering a
# fact rather than a hope: it stands in for flea, records the argv of every writer, and holds the one
# the suite names at the door until the suite has changed the file under it.
cat > "$SANDBOX/wrapflea" <<'WRAP'
#!/bin/sh
set -u
# Sample input: --ui-state {"keys":"windows"}
n=$(cat "$PROBE_WRAP_DIR/count" 2>/dev/null || echo 0)
n=$((n + 1))
printf '%s\n' "$n" > "$PROBE_WRAP_DIR/count"
printf '%s' "$2" > "$PROBE_WRAP_DIR/sent.$n"
if [ "$n" = "${PROBE_WRAP_FAIL:-}" ]; then
  printf 'flea: refused by the uiwriter wrapper\n' >&2
  exit 2
fi
# Written after the argv, so the file the suite waits on proves the argv beside it is already there.
if [ "$n" = "${PROBE_WRAP_HOLD:-}" ]; then
  : > "$PROBE_WRAP_DIR/held"
  while [ ! -e "$PROBE_WRAP_DIR/release" ]; do sleep 0.05; done
fi
exec "$PROBE_WRAP_REAL" "$@"
WRAP
chmod +x "$SANDBOX/wrapflea" || exit 1

# Two changes in ONE turn, so the second is queued behind the first writer rather than racing it.
# The probe quits on the book emptying and not on a timing, so a held writer cannot be cut off.
cat > "$QMLDIR/queued.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root

    property int failures: 0
    property bool started: false

    property var reporter: Connections {
        target: ViewState
        function onSaveFailed() { root.failures = root.failures + 1 }
    }

    Component.onCompleted: {
        ViewState.setKeysPreset("windows")
        ViewState.setTextSize({ mode: 16 })
        console.log("PROBE queued=" + ViewState.writeBook.pending)
        root.started = true
    }

    property var watcher: Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (root.started && ViewState.writeBook.inflight.length === 0) {
                console.log("PROBE failures=" + root.failures)
                Qt.quit()
            }
        }
    }

    // A backstop with its own line, so a run that never drained is read as that and not as a pass.
    property var backstop: Timer {
        interval: 30000
        running: true
        onTriggered: {
            console.log("PROBE stalled failures=" + root.failures)
            Qt.quit()
        }
    }
}
QML

# The failure arm of the same shape: the first writer is refused, and the patch behind it has to keep
# the refused setting AND carry the newer value the window took for it while that writer was running.
cat > "$QMLDIR/refused.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root

    property int failures: 0
    property bool started: false

    property var reporter: Connections {
        target: ViewState
        function onSaveFailed() { root.failures = root.failures + 1 }
    }

    Component.onCompleted: {
        ViewState.toggleColumn("kind")
        ViewState.setTextSize({ mode: 16 })
        ViewState.toggleColumn("mode")
        console.log("PROBE queued=" + ViewState.writeBook.pending)
        root.started = true
    }

    property var watcher: Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (root.started && ViewState.writeBook.inflight.length === 0) {
                console.log("PROBE failures=" + root.failures)
                Qt.quit()
            }
        }
    }

    property var backstop: Timer {
        interval: 30000
        running: true
        onTriggered: {
            console.log("PROBE stalled failures=" + root.failures)
            Qt.quit()
        }
    }
}
QML

WRAPSEED='{"columns":["name","size","date"],"keys":"mac","display":{"textSize":{"mode":"system"}}}'

# One wrap run: its own state home, its own wrapper bookkeeping, and the wrapper as FLEA_BIN. Started
# in the background so the suite can act between two of its writers; the caller waits for wrap_pid.
wrap_start() {
  local probe="$1" hold="$2" refuse="$3"
  sandbox_scratch "$SANDBOX/wrap" || exit 1
  mkdir -p "$SANDBOX/wrap/state" "$SANDBOX/wrap/bin" || exit 1
  env XDG_STATE_HOME="$SANDBOX/wrap/state" "$BIN" --ui-state "$WRAPSEED" >/dev/null 2>&1 \
    || { echo "FAIL wrap: the seed write failed"; fail=1; return 1; }
  env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
      XDG_STATE_HOME="$SANDBOX/wrap/state" FLEA_BIN="$SANDBOX/wrapflea" \
      PROBE_WRAP_DIR="$SANDBOX/wrap/bin" PROBE_WRAP_REAL="$BIN" \
      PROBE_WRAP_HOLD="$hold" PROBE_WRAP_FAIL="$refuse" \
      timeout 60 qs -p "$QMLDIR/$probe" > "$SANDBOX/wrap/probe.log" 2>&1 &
  wrap_pid=$!
  return 0
}

# The combined control. The window saves the keys preset and changes the text size in one turn; the
# preset LANDS; the CLI then changes the preset, the way another window or a script would; and only
# then is the queued writer let go. Its patch must name the text size alone, because the preset it
# was holding is already in the file and is no longer this window's to write.
wrap_ui=$SANDBOX/wrap/state/flea/ui.json
if wrap_start queued.qml 2 ""; then
  waited=0
  until [ -e "$SANDBOX/wrap/bin/held" ]; do
    waited=$((waited + 1))
    if [ "$waited" -gt 600 ]; then
      echo "FAIL queued acknowledgement: no writer ever reached the door"
      fail=1
      break
    fi
    sleep 0.05
  done
  if [ -e "$SANDBOX/wrap/bin/held" ]; then
    # Read off the file and not assumed: the queued writer is at the door, so the writer before it
    # has exited, and this is the proof its patch reached the file before the CLI write below.
    check "the first writer's preset is in the file before anything else touches it" "1" \
          "$(tr -d ' \n' < "$wrap_ui" 2>/dev/null | grep -c '"keys":"windows"')"
    env XDG_STATE_HOME="$SANDBOX/wrap/state" "$BIN" --ui-state '{"keys":"mac"}' >/dev/null 2>&1 \
      || { echo "FAIL queued acknowledgement: the CLI write failed"; fail=1; }
    check "the CLI's preset is what the file holds when the queued writer is released" "1" \
          "$(tr -d ' \n' < "$wrap_ui" 2>/dev/null | grep -c '"keys":"mac"')"
    : > "$SANDBOX/wrap/bin/release" || exit 1
  fi
  wait "$wrap_pid"
  queued_sent=$(cat "$SANDBOX/wrap/bin/sent.2" 2>/dev/null)
  wrap_flat=$(tr -d ' \n' < "$wrap_ui" 2>/dev/null)
  # Printed, so what is said about this control is read off the run and not off the check labels.
  echo "     the first writer sent $(cat "$SANDBOX/wrap/bin/sent.1" 2>/dev/null)"
  echo "     the queued writer sent $queued_sent"
  echo "     and the file then held $wrap_flat"
  check "the queued writer sent a patch at all" "1" "$([ -n "$queued_sent" ] && echo 1 || echo 0)"
  check "the queued writer carries the setting it is queued for" \
        '{"display":{"textSize":{"mode":16}}}' "$queued_sent"
  check "and never the preset the writer before it already stored" "0" \
        "$(printf '%s' "$queued_sent" | grep -c 'keys')"
  check "so the CLI's preset survives the queued write" "1" "$(echo "$wrap_flat" | grep -c '"keys":"mac"')"
  check "and the text size the queued writer was for landed" "1" \
        "$(echo "$wrap_flat" | grep -c '"textSize":{"mode":16}')"
  check "nothing was reported to the pane" "1" "$(grep -c 'PROBE failures=0' "$SANDBOX/wrap/probe.log")"
fi

# The failure arm. The first writer is REFUSED, so nothing is acknowledged and its setting stays
# owed; and the window changed that same setting again while it ran, so what goes out behind it is
# the newer value and never the refused one.
if wrap_start refused.qml "" 1; then
  wait "$wrap_pid"
  refused_sent=$(cat "$SANDBOX/wrap/bin/sent.2" 2>/dev/null)
  wrap_flat=$(tr -d ' \n' < "$wrap_ui" 2>/dev/null)
  echo "     the refused writer sent $(cat "$SANDBOX/wrap/bin/sent.1" 2>/dev/null)"
  echo "     the writer after it sent $refused_sent"
  echo "     and the file then held $wrap_flat"
  check "the refusal is reported to the pane" "1" "$(grep -c 'PROBE failures=1' "$SANDBOX/wrap/probe.log")"
  check "the patch after the refusal keeps the refused setting and takes its newer value" \
        '{"columns":["name","size","date","kind","mode"],"display":{"textSize":{"mode":16}}}' "$refused_sent"
  check "and never sends the value the refused writer was carrying" "0" \
        "$(printf '%s' "$refused_sent" | grep -c '"kind"\]')"
  check "the newer column set is what reached the file" "1" \
        "$(echo "$wrap_flat" | grep -c '"columns":\["name","size","date","kind","mode"\]')"
  check "and the setting queued beside it landed too" "1" \
        "$(echo "$wrap_flat" | grep -c '"textSize":{"mode":16}')"
fi

sandbox_remove "$SANDBOX" || exit 1

[ "$fail" -eq 0 ] && echo "uiwriter: all checks passed"
exit $fail
