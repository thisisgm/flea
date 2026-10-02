#!/bin/bash
# Drives ui/ClipboardBridge.qml under a real Quickshell against a stubbed wl-clipboard: the publish
# argv and its payload bytes, the equal-payload skip, the own-echo guard, an external copy arriving,
# a text copy elsewhere emptying the mirror, an unchanged tick applying nothing, and wl-paste
# missing altogether. tests/js/clipshare.js holds the format; this is the process half that suite
# cannot see, and the stub is what keeps the operator's real clipboard untouched by a test run.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

SANDBOX=$FIXTURE_ROOT/clipbridge-$$
QMLDIR=$SANDBOX/flea
CLIPDIR=$SANDBOX/clip
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
  echo "clipbridge.sh: qs is not installed, cannot drive the QML bridge"
  exit 1
fi

sandbox_make "$SANDBOX" || exit 1
mkdir -p "$QMLDIR/js" "$CLIPDIR" "$SANDBOX/bin" || exit 1
cp ui/ClipboardBridge.qml "$QMLDIR/ClipboardBridge.qml" || exit 1
# Every library the bridge imports, and theirs, read from the import lines so a new one cannot be
# missed; the same walk tests/uiwriter.sh makes for ViewState.
libs=$(sed -n 's|^import "js/\([A-Za-z]*\)\.js".*|\1|p' ui/ClipboardBridge.qml)
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
printf 'module flea\nClipboardBridge 1.0 ClipboardBridge.qml\n' > "$QMLDIR/qmldir" || exit 1

# The stubs. wl-paste's watch mode is a stream that never ends, tail -f on a file the driver
# appends one line per simulated selection change to; its read mode answers the control files, the
# payload with no trailing newline the way the real one answers under -n. wl-copy logs its argv and
# keeps every payload piped into it, --- between them, so the assertions read bytes and not timing.
: > "$CLIPDIR/ticks"
: > "$CLIPDIR/paste.log"
: > "$CLIPDIR/copy.log"
: > "$CLIPDIR/copied"
printf '0' > "$CLIPDIR/has"
: > "$CLIPDIR/payload"
cat > "$SANDBOX/bin/wl-paste" <<'EOS'
#!/bin/sh
echo "$*" >> "$CLIPDIR/paste.log"
case "$*" in
  "-w echo x") exec tail -f "$CLIPDIR/ticks" ;;
  "-n -t x-special/gnome-copied-files")
    [ "$(cat "$CLIPDIR/has")" = "1" ] || exit 1
    cat "$CLIPDIR/payload" ;;
  *) exit 64 ;;
esac
EOS
cat > "$SANDBOX/bin/wl-copy" <<'EOS'
#!/bin/sh
echo "$*" >> "$CLIPDIR/copy.log"
cat >> "$CLIPDIR/copied"
printf '\n---\n' >> "$CLIPDIR/copied"
EOS
chmod +x "$SANDBOX/bin/wl-paste" "$SANDBOX/bin/wl-copy"

cat > "$QMLDIR/probe.qml" <<'QML'
import QtQuick
import Quickshell

// One bridge, driven from both sides: publishes on a schedule while the suite's driver mutates the
// stub clipboard and ticks the watch between them. Every event lands in one log the timer prints.
ShellRoot {
    id: root
    property var events: []

    ClipboardBridge {
        id: bridge
        onArrived: function (c) { root.events.push(c.paths.length === 0 ? "cleared" : "arrived " + c.moving + " " + c.paths.join(",")) }
        onMessage: function (text, isError) { root.events.push("said " + text) }
    }

    // A copy, the same copy again (which must not write), then its cut, on this box's own pace:
    // one wl-copy run is single-digit milliseconds against these hundreds.
    property var copyOnce: Timer {
        interval: 400; running: true
        onTriggered: bridge.publish({ paths: ["/tmp/a b.txt"], moving: false })
    }
    property var copyAgain: Timer {
        interval: 700; running: true
        onTriggered: bridge.publish({ paths: ["/tmp/a b.txt"], moving: false })
    }
    property var cutIt: Timer {
        interval: 1000; running: true
        onTriggered: bridge.publish({ paths: ["/tmp/a b.txt"], moving: true })
    }

    property var settled: Timer {
        interval: 4200; running: true
        onTriggered: {
            console.log("PROBE events=[" + root.events.join("|") + "]")
            Qt.quit()
        }
    }
}
QML

out=$(env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 CLIPDIR="$CLIPDIR" \
      PATH="$SANDBOX/bin:$PATH" timeout 60 qs -p "$QMLDIR/probe.qml" 2>&1 &
      # The driver's half, on the same clock the probe's timers keep: the bridge's own cut payload
      # read back (the echo), an external copy, a text copy elsewhere, and a tick that changed nothing.
      sleep 1.6
      printf 'cut\nfile:///tmp/a%%20b.txt' > "$CLIPDIR/payload"; printf '1' > "$CLIPDIR/has"; echo x >> "$CLIPDIR/ticks"
      sleep 0.7
      printf 'copy\nfile:///ext%%20file' > "$CLIPDIR/payload"; echo x >> "$CLIPDIR/ticks"
      sleep 0.7
      printf '0' > "$CLIPDIR/has"; echo x >> "$CLIPDIR/ticks"
      sleep 0.7
      echo x >> "$CLIPDIR/ticks"
      wait)

events=$(printf '%s\n' "$out" | sed -n 's/.*PROBE events=\[\(.*\)\].*/\1/p')
check "the bridge saw the external copy once, the empty once, and its own echo never" \
      "arrived false /ext file|cleared" "$events"
check "a copy and its cut each reached wl-copy under the gnome type, the repeat never" \
      "-t x-special/gnome-copied-files
-t x-special/gnome-copied-files" "$(cat "$CLIPDIR/copy.log")"
check "the payload bytes are the verb and the escaped uri, byte for byte" \
      "copy
file:///tmp/a%20b.txt
---
cut
file:///tmp/a%20b.txt
---" "$(cat "$CLIPDIR/copied")"

# The read the bridge makes unasked at startup: the selection standing when the window opened is
# already somebody's copy. Counted rather than ordered, because the watcher's own start races it:
# four ticks earn four reads, and the fifth is the one nothing asked for.
check "the four ticks and the unasked startup read are five reads, no more and no fewer" \
      "5" "$(grep -c -- '-n -t x-special/gnome-copied-files' "$CLIPDIR/paste.log")"

# wl-paste missing altogether: the bridge says so once and stays inert rather than crashing or
# spinning, the ui/ViewState.qml writer rule read off running-with-no-exit. The PATH holds only the
# sandbox and the shell the writer needs, because a stub merely deleted falls through to the real
# wl-paste behind it and the case then reads the operator's own clipboard.
rm "$SANDBOX/bin/wl-paste"
ln -s "$(command -v sh)" "$SANDBOX/bin/sh"
QS=$(command -v qs)
TIMEOUT=$(command -v timeout)
out=$(env QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 CLIPDIR="$CLIPDIR" \
      PATH="$SANDBOX/bin" "$TIMEOUT" 60 "$QS" -p "$QMLDIR/probe.qml" 2>&1)
events=$(printf '%s\n' "$out" | sed -n 's/.*PROBE events=\[\(.*\)\].*/\1/p')
case "$events" in
  *"said Copy and paste with other windows is off"*) echo "ok   a missing wl-paste is said once and the window keeps its own clipboard" ;;
  *) echo "FAIL a missing wl-paste is said once and the window keeps its own clipboard"; echo "  events: $events"; fail=1 ;;
esac

sandbox_remove "$SANDBOX"
if [ "$fail" = 0 ]; then
  echo "clipbridge: all checks passed"
else
  echo "clipbridge: FAILURES"
fi
exit "$fail"
