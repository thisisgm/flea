#!/usr/bin/env bash
# A real Quickshell Process blocks on a private FIFO just as a sequential backend can block on
# FUSE. New navigation must reap it, discard its incomplete stdout line and page the replacement.
# In parallel the identity-check backend blocks on its own read: cancel must reap both, even if
# neither reads another stdin byte. Immediate cancellation covers the before-onStarted race.
set -eu
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
command -v qs >/dev/null
fixture="$FIXTURE_ROOT/picker-stall-$$"
sandbox_make "$fixture"
cleanup() {
    # On a failed assertion the owned helper may still be blocked; never signal a recycled PID.
    python3 - "$fixture" <<'PYEND'
import os
from pathlib import Path
import signal
import sys
root = Path(sys.argv[1])
assert (root / '.flea-test-sandbox').is_file()
for text in (root / 'pids').read_text().splitlines() if (root / 'pids').exists() else []:
    process = Path('/proc') / text
    try:
        if os.stat(process).st_uid == os.getuid() and os.fsencode(root / 'helper') in (process / 'cmdline').read_bytes().split(b'\0'):
            os.kill(int(text), signal.SIGKILL)
    except FileNotFoundError:
        pass
PYEND
    sandbox_remove "$fixture"
}
trap cleanup EXIT
mkdir -p "$fixture/flea/js"
cp ui/PickerListing.qml ui/PickerLifecycle.qml ui/Backend.qml "$fixture/flea/"
cp ui/js/Messages.js "$fixture/flea/js/"
# Drive the real runningChanged handler in the rare cancel-before-FailedToStart order. Natural
# missing-executable timing here fails before cancel; this test-only method fixes the event order,
# not the handler, so restoring the old guard makes missing-order time out.
python3 - "$fixture/flea/Backend.qml" <<'PYEND'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text().replace('    function send(object) {', '''
    function testFailedStartDuringQuit() {
        if (child.running) throw new Error("Failed-start fixture unexpectedly has a child")
        root.queueing = true
        root.quitting = true
        child.runningChanged()
    }
    function send(object) {
''', 1)
path.write_text(text)
PYEND
printf 'module flea\nsingleton ViewState 1.0 ViewState.qml\n' > "$fixture/flea/qmldir"
printf 'pragma Singleton\nimport QtQuick\nQtObject { property var state: ({}) }\n' > "$fixture/flea/ViewState.qml"
cp tests/picker-stall.qml "$fixture/shell.qml"
cp tests/picker-stall-helper.py "$fixture/helper"
chmod +x "$fixture/helper"
mkfifo "$fixture/stall.fifo"
for scenario in navigate cancel early missing early-missing missing-order; do
    : > "$fixture/pids"
    : > "$fixture/requests"
    helper="$fixture/helper"
    [[ $scenario != *missing* ]] || helper="$fixture/missing"
    if ! QT_QPA_PLATFORM=offscreen FLEA_BIN="$helper" FLEA_PICKER_CASE="$scenario" FLEA_PICKER_FIXTURE="$fixture" \
        timeout 6 qs -p "$fixture/shell.qml" > "$fixture/output" 2>&1; then
        cat "$fixture/output"; exit 1
    fi
    grep -F "picker-stall $scenario PASS" "$fixture/output" || { cat "$fixture/output"; exit 1; }
    if [[ $scenario == navigate ]] && [[ $(wc -l < "$fixture/pids") != 3 ]]; then
        echo "FAIL: superseded navigation launched an extra worker"; exit 1
    fi
    while read -r pid; do
        if kill -0 "$pid" 2>/dev/null; then echo "FAIL: owned helper $pid survived"; exit 1; fi
    done < "$fixture/pids"
    if grep -q '"c":"transfer"' "$fixture/requests"; then echo 'FAIL: picker sent a write'; exit 1; fi
done
printf 'picker-stall: 6 process lifecycle scenarios passed\n'
