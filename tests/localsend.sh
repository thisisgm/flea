#!/usr/bin/env bash
# Exercise the real QML probe and argv handoff without starting LocalSend or sending files.
set -euo pipefail
source "$(dirname "$0")/../tools/flea-sandbox-guard"
repo=$(cd "$(dirname "$0")/.." && pwd)
fixture="$FIXTURE_ROOT/localsend-$$"
sandbox_make "$fixture"
qs_bin=$(command -v qs)
pid=""
cleanup() {
    local status=$?
    if (( status != 0 )); then cat "$fixture/log" >&2; fi
    if [[ -n "$pid" ]]; then kill "$pid" 2>/dev/null || true; wait "$pid" 2>/dev/null || true; fi
    sandbox_remove "$fixture"
}
trap cleanup EXIT
mkdir "$fixture/bin"
ln -s /bin/sh "$fixture/bin/sh"
cp "$repo/ui/LocalSend.qml" "$fixture/LocalSend.qml"
cat > "$fixture/shell.qml" <<QML
import QtQuick
import Quickshell
Scope {
    LocalSend { id: sender }
    Timer {
        interval: 500; running: true
        onTriggered: {
            console.log("AVAILABLE=" + sender.available)
            sender.send(["/tmp/a file.txt", "/tmp/\$(touch nope);'folder"])
        }
    }
}
QML
for executable in absent localsend localsend_app; do
    if [[ "$executable" != absent ]]; then
        cat > "$fixture/bin/$executable" <<'STUB'
#!/bin/sh
printf '%s\0' "$@" > "$LOCALSEND_TEST_LOG"
STUB
        chmod +x "$fixture/bin/$executable"
    fi
    PATH="$fixture/bin" QT_QPA_PLATFORM=offscreen LOCALSEND_TEST_LOG="$fixture/args" \
        "$qs_bin" -p "$fixture/shell.qml" > "$fixture/log" 2>&1 &
    pid=$!
    for ((attempt=0; attempt<100; attempt++)); do
        if [[ "$executable" == absent ]]; then
            grep -q 'AVAILABLE=false' "$fixture/log" && break
        else
            [[ -s "$fixture/args" ]] && break
        fi
        sleep 0.05
    done
    if [[ "$executable" == absent ]]; then
        grep -q 'AVAILABLE=false' "$fixture/log"
        [[ ! -e "$fixture/args" ]]
    else
        grep -q 'AVAILABLE=true' "$fixture/log"
        printf '%s\0' '/tmp/a file.txt' "/tmp/\$(touch nope);'folder" > "$fixture/expected"
        cmp "$fixture/expected" "$fixture/args"
        rm "$fixture/args" "$fixture/bin/$executable"
    fi
    if grep -E 'ERROR|ReferenceError|TypeError' "$fixture/log"; then exit 1; fi
    kill "$pid"
    wait "$pid" 2>/dev/null || true
    pid=""
    echo "PASS: LocalSend $executable"
done
