#!/bin/bash
. "$(dirname "${BASH_SOURCE[0]}")/../tools/flea-ipc"
flea_ipc_init qs
. "$(dirname "$0")/headless-commons.sh"
# Two ViewState singletons over one temp ui.json: live apply of one window's change in the other, per-window state, own-write and half-written guards; tests/js/uistate.js pins the merge, this pins the QML wiring.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
# The probes start the GVfs trash daemon, so the run gets its own bus; on the inherited one a later suite's first trash goes unlisted.
if [ "${FLEA_XWSETTINGS_BUS:-}" != 1 ]; then
  command -v dbus-run-session >/dev/null || { echo 'xwsettings.sh: dbus-run-session is required' >&2; exit 1; }
  exec dbus-run-session -- env FLEA_XWSETTINGS_BUS=1 bash "tests/$(basename "$0")" "$@"
fi

BIN=$PWD/target/debug/flea
SANDBOX=$FIXTURE_ROOT/xwsettings-$$
QMLDIR=$SANDBOX/flea
fail=0
probe_poll_s=0.05
probe_wait_tries=600
probe_timeout_s=60
startup_checks=5
# WindowBody.backendDrained terminates qs with SIGTERM, which timeout reports as 128 + 15.
qs_drained_exit=143

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
  echo "xwsettings.sh: qs is not installed, cannot drive the QML probes"
  exit 1
fi
if [ ! -x "$BIN" ]; then
  printf 'xwsettings.sh: no binary at %s\n' "$BIN" >&2
  printf 'xwsettings.sh: build it (cargo build); refusing to report on nothing\n' >&2
  exit 1
fi
python3 tests/ui-process-ownership.py || fail=1

sandbox_make "$SANDBOX" || exit 1
# The drain probe's run dir lives in the sandbox, so the guard's removal at the end takes it too.
sandbox_scratch "$SANDBOX/drain" || exit 1
TMPDIR=$SANDBOX/drain python3 tests/xwsettings-drain.py "$BIN"
check "quitReady observers finish before shutdown" 0 "$?"

# The singleton and the libraries it imports, copied the way tests/uiwriter.sh copies them:
# importing ui/ whole makes Quickshell scan every file in it and warn about the two OEM symlinks
# a headless run has no session for.
mkdir -p "$QMLDIR/js" || exit 1
cp ui/ViewState.qml "$QMLDIR/ViewState.qml" || exit 1
# Sample import: import "js/Settings.js" as Settings; transitive libraries use .import "Places.js" as Places.
libs=$(sed -n 's|^import "js/\([A-Za-z]*\)\.js".*|\1|p' ui/ViewState.qml)
copied=" "
while [ -n "${libs// /}" ]; do
  next=""
  for lib in $libs; do
    case "$copied" in *" $lib "*) continue ;; esac
    cp "ui/js/$lib.js" "$QMLDIR/js/$lib.js" || exit 1
    copied="$copied$lib "
    next="$next $(sed -n 's|^\.import "\([A-Za-z]*\)\.js".*|\1|p' "ui/js/$lib.js" | tr '\n' ' ')"
  done
  libs=$next
done
headless_commons "$SANDBOX" "$QMLDIR/Commons" || exit 1
printf 'module flea\nsingleton ViewState 1.0 ViewState.qml\n' > "$QMLDIR/qmldir" || exit 1

# Window A: saves one preference the way pane.toggleHidden does, then stays alive one beat past
# its drained writer so a re-applied own write would have to show as a queued patch or a failure.
cat > "$QMLDIR/writer.qml" <<'QML'
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
        ViewState.changeKey("hidden", true)
        root.started = true
    }

    property var watcher: Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            if (root.started && ViewState.writeBook.inflight.length === 0) {
                console.log("PROBE failures=" + root.failures)
                console.log("PROBE patch=" + ViewState.patch())
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

# Window B: holds another window's read and watches it. It quits the moment the preference lands,
# reporting what else moved with it, so a late or partial application reads as that and not a pass.
cat > "$QMLDIR/watcher.qml" <<'QML'
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
        console.log("PROBE watching hidden=" + ViewState.state.hidden + " view=" + ViewState.view)
        root.started = true
    }

    property var watcher: Timer {
        interval: 50
        repeat: true
        running: true
        onTriggered: {
            if (!root.started) return
            if (ViewState.state.hidden === true) {
                console.log("PROBE applied hidden=" + ViewState.state.hidden + " view=" + ViewState.view
                            + " lastPath=" + ViewState.state.lastPath + " density=" + ViewState.state.density)
                console.log("PROBE failures=" + root.failures)
                console.log("PROBE patch=" + ViewState.patch())
                Qt.quit()
            }
        }
    }

    property var backstop: Timer {
        interval: 15000
        running: true
        onTriggered: {
            console.log("PROBE stalled hidden=" + ViewState.state.hidden)
            Qt.quit()
        }
    }
}
QML

sandbox_scratch "$SANDBOX/state" || exit 1
env XDG_STATE_HOME="$SANDBOX/state" "$BIN" --ui-state \
  '{"hidden":false,"view":"grid","lastPath":"/seed","density":"compact"}' >/dev/null 2>&1 \
  || { echo "FAIL xwsettings: the seed write failed"; exit 1; }

env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/state" \
    FLEA_BIN="$BIN" timeout 60 qs -p "$QMLDIR/watcher.qml" > "$SANDBOX/watching.log" 2>&1 &
watching_pid=$!
waited=0
until grep -q 'PROBE watching' "$SANDBOX/watching.log" 2>/dev/null; do
  waited=$((waited + 1))
  if [ "$waited" -gt 600 ]; then
    echo "FAIL xwsettings: the watching window never reported its read"
    fail=1
    kill "$watching_pid" 2>/dev/null
    wait "$watching_pid" 2>/dev/null
    break
  fi
  sleep 0.05
done
if [ "$fail" -eq 0 ]; then
  env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/state" \
      FLEA_BIN="$BIN" timeout 60 qs -p "$QMLDIR/writer.qml" > "$SANDBOX/writing.log" 2>&1
  wait "$watching_pid"
  applied=$(grep 'PROBE applied' "$SANDBOX/watching.log" | head -1)
  check "the watching window printed what it applied" "1" "$([ -n "$applied" ] && echo 1 || echo 0)"
  check "the other window's hidden toggle applied there" "1" "$(echo "$applied" | grep -c 'hidden=true')"
  check "and its view stayed its own" "1" "$(echo "$applied" | grep -c 'view=grid')"
  check "and where it was stayed too" "1" "$(echo "$applied" | grep -c 'lastPath=/seed')"
  check "the watching window was never asked to save" "1" "$(grep -c 'PROBE patch={}' "$SANDBOX/watching.log")"
  check "and reported no failure" "1" "$(grep -c 'PROBE failures=0' "$SANDBOX/watching.log")"
  check "the writing window drained with nothing owed" "1" "$(grep -c 'PROBE patch={}' "$SANDBOX/writing.log")"
  check "and nothing reported" "1" "$(grep -c 'PROBE failures=0' "$SANDBOX/writing.log")"
  flat=$(tr -d ' \n' < "$SANDBOX/state/flea/ui.json" 2>/dev/null)
  check "the file holds the toggle" "1" "$(echo "$flat" | grep -c '"hidden":true')"
  check "and the view nobody touched" "1" "$(echo "$flat" | grep -c '"view":"grid"')"
fi

# A half-written file is ignored until the next valid write: the watching window keeps drawing
# what it had, and the write after the garbage still applies.
if [ "$fail" -eq 0 ]; then
  sandbox_scratch "$SANDBOX/garbage" || exit 1
  mkdir -p "$SANDBOX/garbage/state/flea" || exit 1
  env XDG_STATE_HOME="$SANDBOX/garbage/state" "$BIN" --ui-state \
    '{"hidden":false,"view":"grid","density":"compact"}' >/dev/null 2>&1 \
    || { echo "FAIL xwsettings: the garbage-phase seed write failed"; exit 1; }
  env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/garbage/state" \
      FLEA_BIN="$BIN" timeout 60 qs -p "$QMLDIR/watcher.qml" > "$SANDBOX/garbage.log" 2>&1 &
  garbage_pid=$!
  waited=0
  until grep -q 'PROBE watching' "$SANDBOX/garbage.log" 2>/dev/null; do
    waited=$((waited + 1))
    if [ "$waited" -gt 600 ]; then
      echo "FAIL xwsettings: the garbage-phase watcher never reported its read"
      fail=1
      kill "$garbage_pid" 2>/dev/null
      wait "$garbage_pid" 2>/dev/null
      break
    fi
    sleep 0.05
  done
  if [ "$fail" -eq 0 ]; then
    # A live second replace in the same process: the stayer stays alive across the garbage and the
    # recovery write, so the recovery applying in its own log proves no fresh process was needed.
    cat > "$QMLDIR/stayer.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root
    property string lastDensity: ""
    Component.onCompleted: {
        console.log("PROBE watching hidden=" + ViewState.state.hidden)
        root.lastDensity = String(ViewState.state.density || "compact")
    }
    property var watcher: Timer {
        interval: 50
        repeat: true
        running: true
        onTriggered: {
            var density = String(ViewState.state.density || "compact")
            if (density !== root.lastDensity) {
                root.lastDensity = density
                console.log("PROBE live density=" + density)
            }
            if (ViewState.state.hidden === true) {
                console.log("PROBE applied hidden=" + ViewState.state.hidden)
                console.log("PROBE patch=" + ViewState.patch())
                Qt.quit()
            }
        }
    }
    property var backstop: Timer {
        interval: 30000
        running: true
        onTriggered: {
            console.log("PROBE stalled hidden=" + ViewState.state.hidden)
            Qt.quit()
        }
    }
}
QML
    kill "$garbage_pid" 2>/dev/null || true
    wait "$garbage_pid" 2>/dev/null || true
    env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/garbage/state" \
        FLEA_BIN="$BIN" timeout 60 qs -p "$QMLDIR/stayer.qml" > "$SANDBOX/stayer.log" 2>&1 &
    stayer_pid=$!
    waited=0
    until grep -q 'PROBE watching' "$SANDBOX/stayer.log" 2>/dev/null; do
      waited=$((waited + 1))
      if [ "$waited" -gt 600 ]; then
        echo "FAIL xwsettings: the stayer never reported its read"
        fail=1
        kill "$stayer_pid" 2>/dev/null
        wait "$stayer_pid" 2>/dev/null
        break
      fi
      sleep 0.05
    done
    if [ "$fail" -eq 0 ]; then
      # Truncated mid-object, the way an editor's own write looks between its truncate and its close.
      printf '%s' '{"hidden":true,"view":"grid","density":"compact"' \
        > "$SANDBOX/garbage/state/flea/ui.json" || exit 1
      # A marker valid write behind the garbage: the watcher answering it proves it handled every event after the garbage too.
      env XDG_STATE_HOME="$SANDBOX/garbage/state" "$BIN" --ui-state \
        '{"hidden":false,"view":"grid","density":"normal"}' >/dev/null 2>&1 \
        || { echo "FAIL xwsettings: the marker write failed"; fail=1; }
      # Only the probe's end (its stall backstop or timeout) stops the wait early.
      until grep -q 'PROBE live density=normal' "$SANDBOX/stayer.log" 2>/dev/null; do
        if ! kill -0 "$stayer_pid" 2>/dev/null; then
          echo "FAIL xwsettings: the watcher never answered the marker behind the garbage"
          fail=1
          break
        fi
        sleep "$probe_poll_s"
      done
      check "the watcher answered a later event behind the garbage" "1" "$(grep -c 'PROBE live density=normal' "$SANDBOX/stayer.log")"
      check "and the half-written file applied nothing live" "0" "$(grep -c 'PROBE applied' "$SANDBOX/stayer.log")"
      # The live second replace in the same process: the recovery write lands in the stayer's own log.
      env XDG_STATE_HOME="$SANDBOX/garbage/state" "$BIN" --ui-state \
        '{"hidden":true,"view":"grid","density":"compact"}' >/dev/null 2>&1 \
        || { echo "FAIL xwsettings: the recovery write failed"; fail=1; }
      wait "$stayer_pid"
      check "the next valid write applies again live" "1" "$(grep -c 'PROBE applied hidden=true' "$SANDBOX/stayer.log")"
    fi
  fi
fi

# Settled values, not raw bytes: a bogus column heals, a null places group keeps favourites,
# and a removed key reads as its default. Each hand edit below fails on raw_bytes and passes on settled.
if [ "$fail" -eq 0 ]; then
  sandbox_scratch "$SANDBOX/settled" || exit 1
  mkdir -p "$SANDBOX/settled/state/flea" || exit 1
  env XDG_STATE_HOME="$SANDBOX/settled/state" "$BIN" --ui-state \
    '{"hidden":true,"density":"compact","columns":["name","size","date"],"places":{"showUnmounted":true,"favourites":[{"label":"Old","path":"/old"}]}}' >/dev/null 2>&1 \
    || { echo "FAIL xwsettings: the settled-phase seed write failed"; exit 1; }
  cat > "$QMLDIR/settled.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root
    Component.onCompleted: {
        console.log("PROBE watching columns=" + JSON.stringify(ViewState.state.columns)
                    + " placesType=" + (ViewState.state.places === null ? "null" : typeof ViewState.state.places)
                    + " hidden=" + ViewState.state.hidden)
    }
    property var watcher: Timer {
        interval: 100
        repeat: true
        running: true
        onTriggered: {
            console.log("PROBE live columns=" + JSON.stringify(ViewState.state.columns)
                        + " placesType=" + (ViewState.state.places === null ? "null" : typeof ViewState.state.places)
                        + " hidden=" + ViewState.state.hidden
                        + " favourites=" + JSON.stringify((ViewState.state.places || {}).favourites)
                        + " density=" + ViewState.density)
        }
    }
}
QML
  env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/settled/state" \
      FLEA_BIN="$BIN" timeout "$probe_timeout_s" qs -p "$QMLDIR/settled.qml" > "$SANDBOX/settled.log" 2>&1 &
  settled_pid=$!
  # Waits for a live line carrying the marker after the given line count; only the probe's end (its timeout) stops the wait early.
  settled_wait() {
    until [ "$(grep 'PROBE live' "$SANDBOX/settled.log" | tail -n +"$(($1 + 1))" | grep -c "$2")" -ge 1 ]; do
      kill -0 "$settled_pid" 2>/dev/null || return 1
      sleep "$probe_poll_s"
    done
  }
  waited=0
  until grep -q 'PROBE watching' "$SANDBOX/settled.log" 2>/dev/null; do
    waited=$((waited + 1))
    if [ "$waited" -gt 600 ]; then
      echo "FAIL xwsettings: the settled watcher never reported its read"
      fail=1
      kill "$settled_pid" 2>/dev/null
      wait "$settled_pid" 2>/dev/null
      break
    fi
    sleep 0.05
  done
  if [ "$fail" -eq 0 ]; then
    live_before_bogus=$(grep -c 'PROBE live' "$SANDBOX/settled.log")
    python3 - "$SANDBOX/settled/state/flea/ui.json" <<'PY'
import json, sys
p = sys.argv[1]
doc = json.load(open(p))
doc["columns"] = ["name", "size", "bogus"]
doc["density"] = "normal"
json.dump(doc, open(p, "w"))
PY
    # The density marker rides the same write as the bogus edit, so only a line carrying it proves the watcher handled that write.
    if ! settled_wait "$live_before_bogus" 'density=normal'; then
      echo "FAIL xwsettings: the watcher never answered the bogus column edit; the log's last lines:"
      tail -n 5 "$SANDBOX/settled.log" | cut -c1-200
      fail=1
    fi
    handled=$(grep 'PROBE live' "$SANDBOX/settled.log" | tail -n +"$((live_before_bogus + 1))" | grep 'density=normal')
    check "a bogus column is never taken" "0" "$(printf '%s' "$handled" | grep -c 'bogus')"
    # The healed default appears live; on raw_bytes the bogus string stays in the log.
    check "and the settled default lands instead" "1" "$([ "$(printf '%s' "$handled" | grep -c '"name","size","date"')" -ge 1 ] && echo 1 || echo 0)"
    live_before=$(grep -c 'PROBE live' "$SANDBOX/settled.log")
    python3 - "$SANDBOX/settled/state/flea/ui.json" <<'PY'
import json, sys
p = sys.argv[1]
doc = json.load(open(p))
doc["places"] = None
doc["density"] = "compact"
json.dump(doc, open(p, "w"))
PY
    # The compact marker rides the null write, so a periodic tick without it proves nothing about the edit.
    if ! settled_wait "$live_before" 'density=compact'; then
      echo "FAIL xwsettings: the watcher never answered the null places edit"
      fail=1
    fi
    handled_null=$(grep 'PROBE live' "$SANDBOX/settled.log" | tail -n +"$((live_before + 1))" | grep 'density=compact')
    check "a null places group never empties favourites" "0" "$(printf '%s' "$handled_null" | grep -c 'placesType=null')"
    check "and the kept entries survive it" "1" "$([ "$(printf '%s' "$handled_null" | grep -c '/old')" -ge 1 ] && echo 1 || echo 0)"
    live_before=$(grep -c 'PROBE live' "$SANDBOX/settled.log")
    printf '%s' '{"density":"normal"}' > "$SANDBOX/settled/state/flea/ui.json" || exit 1
    settled_wait "$live_before" 'hidden=false' || true
    check "a removed key reads as its default" "1" "$([ "$(grep 'PROBE live' "$SANDBOX/settled.log" | tail -n +"$((live_before + 1))" | grep -c 'hidden=false')" -ge 1 ] && echo 1 || echo 0)"
    echo "settled tail:"
    grep 'PROBE live' "$SANDBOX/settled.log" | tail -3 || true
  fi
  kill "$settled_pid" 2>/dev/null || true
  wait "$settled_pid" 2>/dev/null || true
fi

# A refused patch never blocks later saves: bogus columns are refused, then a valid density lands
# in the same process. On raw_bytes the second write is refused for the window's life.
if [ "$fail" -eq 0 ]; then
  sandbox_scratch "$SANDBOX/refuse" || exit 1
  mkdir -p "$SANDBOX/refuse/state/flea" || exit 1
  env XDG_STATE_HOME="$SANDBOX/refuse/state" "$BIN" --ui-state \
    '{"hidden":false,"density":"compact"}' >/dev/null 2>&1 \
    || { echo "FAIL xwsettings: the refuse-phase seed write failed"; exit 1; }
  cat > "$QMLDIR/refuse.qml" <<'QML'
import QtQuick
import Quickshell

ShellRoot {
    id: root
    property int failures: 0
    property bool stepped: false
    property var reporter: Connections {
        target: ViewState
        function onSaveFailed() { root.failures = root.failures + 1 }
    }
    Component.onCompleted: {
        ViewState.changeKey("columns", ["name", "size", "bogus"])
    }
    property var step: Timer {
        interval: 1500
        running: true
        repeat: false
        onTriggered: {
            root.stepped = true
            console.log("PROBE afterRefuse failures=" + root.failures + " patch=" + ViewState.patch())
            ViewState.changeKey("density", "normal")
        }
    }
    property var done: Timer {
        interval: 300
        repeat: true
        running: true
        onTriggered: {
            if (root.stepped && root.failures > 0 && ViewState.writeBook.inflight.length === 0) {
                console.log("PROBE final failures=" + root.failures + " patch=" + ViewState.patch()
                            + " density=" + ViewState.state.density)
                Qt.quit()
            }
        }
    }
    property var backstop: Timer {
        interval: 30000
        running: true
        onTriggered: {
            console.log("PROBE stalled failures=" + root.failures + " patch=" + ViewState.patch())
            Qt.quit()
        }
    }
}
QML
  out=$(env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/refuse/state" \
      FLEA_BIN="$BIN" timeout 60 qs -p "$QMLDIR/refuse.qml" 2>&1)
  check "the refused patch is reported once" "1" "$(echo "$out" | grep -c 'PROBE afterRefuse failures=1')"
  check "and a later valid write applies again in the same process" "1" "$(echo "$out" | grep -c 'PROBE final failures=1 patch={} density=normal')"
  flat=$(tr -d ' \n' < "$SANDBOX/refuse/state/flea/ui.json" 2>/dev/null)
  check "and the file holds the valid write" "1" "$(echo "$flat" | grep -c '"density":"normal"')"
  echo "$out" | grep -a 'PROBE ' | tail -5 || true
fi

# Hold a real apply child, queue a refused write behind it, and release the child only after checking the queue.
if [ "$fail" -eq 0 ]; then
  sandbox_scratch "$SANDBOX/prune" || exit 1
  env XDG_STATE_HOME="$SANDBOX/prune/state" "$BIN" --ui-state \
    '{"columns":["name","size","date"],"density":"compact"}' >/dev/null 2>&1 || exit 1
  mkfifo "$SANDBOX/prune/gate" || exit 1
  cat > "$SANDBOX/prune/held-bin" <<'SH'
#!/bin/bash
set -u
if [ "$#" -eq 1 ] && [ "$1" = --ui-state ]; then
  if [ ! -e "$PROBE_ENTERED" ]; then
    : > "$PROBE_ENTERED"
    read -r permit < "$PROBE_GATE"
  fi
fi
exec "$PROBE_REAL_BIN" "$@"
SH
  chmod +x "$SANDBOX/prune/held-bin" || exit 1
  cp tests/xwsettings-prune.qml "$QMLDIR/prune.qml" || exit 1
  out=$(env QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 XDG_STATE_HOME="$SANDBOX/prune/state" \
      FLEA_BIN="$SANDBOX/prune/held-bin" PROBE_REAL_BIN="$BIN" PROBE_ENTERED="$SANDBOX/prune/entered" \
      PROBE_GATE="$SANDBOX/prune/gate" PROBE_POLL_S="$probe_poll_s" \
      timeout "$probe_timeout_s" qs -p "$QMLDIR/prune.qml" 2>&1)
  # Sample probe: "PROBE drained runs=1 inflight= queued= patch={}" records a real child start and its completed prune.
  check "the held-prune probe finished" 0 "$?"
  check "a refused prune stays queued behind a real busy settle" 1 \
    "$(echo "$out" | grep -c 'PROBE busy-kept={"columns":\["name","size","bogus"\],"density":"normal"}')"
  check "the queued prune never moves inflight while busy" 1 "$(echo "$out" | grep -c 'PROBE busy-inflight=$')"
  check "the settle child really was running" 1 "$(echo "$out" | grep -c 'PROBE busy-running=true')"
  check "exactly one prune starts and drains after the apply lands" 1 \
    "$(echo "$out" | grep -c 'PROBE drained runs=1 inflight= queued= patch={}')"
  flat=$(tr -d ' \n' < "$SANDBOX/prune/state/flea/ui.json")
  check "the valid key beside the refused key retries" 1 "$(echo "$flat" | grep -c '"density":"normal"')"
  echo "$out" | grep -a 'PROBE ' || true
fi

# Both real WindowBody instances share state; B's launch and startup writes must leave A's pane alone.
if [ "$fail" -eq 0 ]; then
  sandbox_scratch "$SANDBOX/start" || exit 1
  mkdir -p "$SANDBOX/start/files-a" "$SANDBOX/start/files-b" || exit 1
  for role in a b; do
    for name in a.txt b.txt c.txt; do
      : > "$SANDBOX/start/files-$role/$name" || exit 1
    done
    config="$SANDBOX/start/ui-$role"
    mkdir -p "$config/boot" || exit 1
    headless_commons "$SANDBOX" "$config/boot/Commons" || exit 1
    ln -s "$(readlink -f ui/boot/Ui)" "$config/boot/Ui" || exit 1
    cp tests/xwsettings-start.qml "$config/boot/shell.qml" || exit 1
  done
  env XDG_STATE_HOME="$SANDBOX/start/state" "$BIN" --ui-state \
    '{"hidden":false,"view":"list","density":"compact","updates":{"autoCheck":false}}' >/dev/null 2>&1 \
    || exit 1
  env DISPLAY=flea-offscreen QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 QSG_RHI_BACKEND=opengl \
      XDG_STATE_HOME="$SANDBOX/start/state" FLEA_UI="$SANDBOX/start/ui-a" FLEA_BIN="$BIN" FLEA_PATH="$SANDBOX/start/files-a" \
      PROBE_BODY="$PWD/ui/WindowBody.qml" PROBE_STATE_HELPER="$PWD/tests/xwsettings-start-state.qml" \
      PROBE_ROLE=A PROBE_READY="$SANDBOX/start/ready" PROBE_DONE="$SANDBOX/start/done" PROBE_OTHER_PATH="$SANDBOX/start/files-b" \
      PROBE_POLL_S="$probe_poll_s" timeout "$probe_timeout_s" qs -p "$SANDBOX/start/ui-a/boot/shell.qml" \
      > "$SANDBOX/staying.log" 2>&1 &
  staying_pid=$!
  waited=0
  until [ -e "$SANDBOX/start/ready" ]; do
    waited=$((waited + 1))
    if [ "$waited" -ge "$probe_wait_tries" ]; then
      echo "FAIL xwsettings: A never captured its focus and cursor"
      cat "$SANDBOX/staying.log"
      fail=1
      break
    fi
    sleep "$probe_poll_s"
  done
  if [ "$fail" -eq 0 ]; then
    env DISPLAY=flea-offscreen QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 QSG_RHI_BACKEND=opengl \
        XDG_STATE_HOME="$SANDBOX/start/state" FLEA_UI="$SANDBOX/start/ui-b" FLEA_BIN="$BIN" FLEA_PATH="$SANDBOX/start/files-b" \
        PROBE_BODY="$PWD/ui/WindowBody.qml" PROBE_STATE_HELPER="$PWD/tests/xwsettings-start-state.qml" PROBE_ROLE=B PROBE_READY="$SANDBOX/start/ready" PROBE_DONE="$SANDBOX/start/done" \
        PROBE_POLL_S="$probe_poll_s" timeout "$probe_timeout_s" qs -p "$SANDBOX/start/ui-b/boot/shell.qml" \
        > "$SANDBOX/starting.log" 2>&1
    check "B's real startup drained" "$qs_drained_exit" "$?"
    check "B's backend answered its drain" 1 "$(grep -c 'PROBE B backend drained' "$SANDBOX/starting.log")"
    check "B listed its own folder" 1 "$(grep -c 'PROBE B drained path=' "$SANDBOX/starting.log")"
    : > "$SANDBOX/start/done" || exit 1
  fi
  wait "$staying_pid"
  # Sample probe: "PROBE PASS second startup keeps A's cursor" follows B's startup and A's completed settle.
  check "A's real window drained" "$qs_drained_exit" "$?"
  check "A's backend answered its drain" 1 "$(grep -c 'PROBE A backend drained' "$SANDBOX/staying.log")"
  check "B's start preserved A's pane, focus, cursor and listing" "$startup_checks" "$(grep -c 'PROBE PASS second startup' "$SANDBOX/staying.log")"
  check "the startup probe reported no failure" 0 "$(grep -c 'PROBE FAIL' "$SANDBOX/staying.log")"
  check "A handled the state change before checking" 1 "$(grep -c 'PROBE A done failures=0 settles=' "$SANDBOX/staying.log")"
  if [ "$fail" -ne 0 ]; then
    cat "$SANDBOX/staying.log" "$SANDBOX/starting.log" 2>/dev/null
  fi
fi

# Drive tab/window keys through WindowBody; record detached launches and forward other calls to the real candidate.
sandbox_scratch "$SANDBOX/tabs" || exit 1
mkdir -p "$SANDBOX/tabs/config" "$SANDBOX/tabs/a/sub" "$SANDBOX/tabs/b/sub" || exit 1
printf 'a\n' > "$SANDBOX/tabs/a/a.txt" || exit 1
printf 'b\n' > "$SANDBOX/tabs/b/b.txt" || exit 1
ln -s "$PWD/ui" "$SANDBOX/tabs/config/flea" || exit 1
headless_commons "$SANDBOX" "$SANDBOX/tabs/config/Commons" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$SANDBOX/tabs/config/Ui" || exit 1
cp tests/xwsettings-tabs.qml "$SANDBOX/tabs/config/shell.qml" || exit 1
cat > "$SANDBOX/tabs/launcher" <<'SH'
#!/bin/bash
if [ "$#" -eq 1 ] && [[ "$1" = /* ]]; then
    printf '%s\n' "$1" >> "$PROBE_LAUNCHES"
    exit 0
fi
exec "$PROBE_REAL_BIN" "$@"
SH
chmod +x "$SANDBOX/tabs/launcher" || exit 1
# An entry "mode@ms" runs the mode with the phase timer at ms, so a tick lands inside the phase's own pointer waits.
for entry in tabs window trash tabview restore openers watch quickdrag quickdrag@1 middrag dragpreview; do
  mode=${entry%@*}
  tick=""
  [ "$entry" = "$mode" ] || tick=${entry#*@}
  label=${entry/@/-tick}
  mkdir -p "$SANDBOX/tabs/$label" || exit 1
  # The three drag shapes share one probe; every other mode shares the tabs probe.
  case "$mode" in
    quickdrag|middrag|dragpreview) cp tests/xwsettings-drag.qml "$SANDBOX/tabs/config/shell.qml" || exit 1 ;;
    *) cp tests/xwsettings-tabs.qml "$SANDBOX/tabs/config/shell.qml" || exit 1 ;;
  esac
  : > "$SANDBOX/tabs/$label/launches" || exit 1
  env XDG_STATE_HOME="$SANDBOX/tabs/$label/state" "$BIN" --ui-state \
    '{"keys":"default","view":"list","sort":{"key":"name","reverse":false},"updates":{"autoCheck":false}}' >/dev/null 2>&1 || exit 1
  start_path="$SANDBOX/tabs/a"
  if [ "$mode" = restore ] || [ "$mode" = dragpreview ]; then
    restored=$(python3 - "$SANDBOX/tabs/a" "$SANDBOX/tabs/b" "$mode" <<'PY'
import json, sys
a, b, mode = sys.argv[1:]
print(json.dumps({"startIn": "last", "lastTabs": {"paths": [a, b, a + "/sub"], "index": 0 if mode == "dragpreview" else 1}}))
PY
    ) || exit 1
    env XDG_STATE_HOME="$SANDBOX/tabs/$label/state" "$BIN" --ui-state "$restored" >/dev/null 2>&1 || exit 1
    start_path=""
  fi
  out=$(env DISPLAY=flea-offscreen QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 QSG_RHI_BACKEND=opengl \
      XDG_STATE_HOME="$SANDBOX/tabs/$label/state" FLEA_BIN="$SANDBOX/tabs/launcher" \
      FLEA_PATH="$start_path" PROBE_BASE="$SANDBOX/tabs/a" PROBE_OTHER_PATH="$SANDBOX/tabs/b" \
      PROBE_MODE="$mode" PROBE_TICK_MS="$tick" PROBE_REAL_BIN="$BIN" PROBE_LAUNCHES="$SANDBOX/tabs/$label/launches" PROBE_SHOT="$SANDBOX/tabs/$label/held.png" \
      PROBE_BODY="$PWD/ui/WindowBody.qml" timeout 30 qs -p "$SANDBOX/tabs/config" 2>&1)
  result=$?
  printf '%s\n' "$out" | grep -E 'TAB_HUNT (FAIL|DONE)|TypeError|ReferenceError|ERROR' || true
  printf '%s\n' "$out" | grep -E 'WARN' | sort -u | head -5 || true
  check "the $label key/pointer probe drains" "$qs_drained_exit" "$result"
  check "the $label key/pointer probe finishes without a failure" 1 \
    "$(printf '%s\n' "$out" | grep -cE "TAB_HUNT DONE $mode [0-9]+ checks, 0 failed")"
  check "the $label key/pointer probe has no script errors" 0 \
    "$(printf '%s\n' "$out" | grep -cE 'TypeError|ReferenceError|ERROR')"
  if ! printf '%s\n' "$out" | grep -q "TAB_HUNT DONE $mode "; then printf '%s\n' "$out" | tail -15; fi
  if [ "$mode" = dragpreview ] && [ -n "${FLEA_CI_SUITE_LOGS:-}" ]; then
    cp "$SANDBOX/tabs/$label/held.png" "$FLEA_CI_SUITE_LOGS/cap-tabs-drag-held-offscreen.png" || fail=1
  fi
done

python3 tests/xwsettings-backends.py "$BIN" "$SANDBOX/two-backends" || fail=1

sandbox_remove "$SANDBOX" || exit 1

[ "$fail" -eq 0 ] && echo "xwsettings: all checks passed"
exit "$fail"
