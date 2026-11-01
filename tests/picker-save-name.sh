#!/usr/bin/env bash
# Pointer selection in the real picker, with private homes, buses, runtime and backend workers.
set -euo pipefail
cd "$(dirname "$0")/.."
. "$PWD/tools/flea-sandbox-guard"
. "$PWD/tests/headless-commons.sh"
sandbox_root_ok
scratch=$(mktemp -d "$SANDBOX_ROOT/flea-save-name.XXXXXXXX")
: > "$scratch/$SANDBOX_MARKER"
trap 'sandbox_remove "$scratch"' EXIT
mkdir -p "$scratch/config" "$scratch/fixture/z-folder"
cp -a ui "$scratch/config/flea"
# Only the owned test process exits normally after the shipped reply-write/worker-drain sequence.
python3 - "$scratch/config/flea/PickerWindow.qml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
text = path.read_text()
stop = 'onStopped: Quickshell.execDetached(["kill", String(Quickshell.processId)])'
assert text.count(stop) == 1, 'picker teardown changed'
path.write_text(text.replace(stop, 'onStopped: Qt.exit(0)'))
PY
headless_commons "$scratch"
ln -s "$(readlink -f ui/boot/Ui)" "$scratch/config/Ui"
cp tests/picker-save-name.qml "$scratch/config/shell.qml"
for name in a-first.txt archive.tar.gz plain '.env' '日本語 résumé.txt'; do
    printf 'unchanged %s\n' "$name" > "$scratch/fixture/$name"
done
[ -x "$PWD/target/debug/flea" ] || cargo build --locked
failures=0
phases=0
for view in list grid; do
    for scenario in save collision stale open folder multiple savefiles; do
        phase="$scratch/$view-$scenario"
        mkdir -p "$phase"/{home,state/flea,cache,data,runtime,tmp}
        chmod 700 "$phase/runtime"
        printf '{"pickerView":"%s","foldersFirst":false}' "$view" > "$phase/state/flea/ui.json"
        request=$(python3 - "$scratch/fixture" "$scenario" <<'PY'
import json, sys
scenario = sys.argv[2]
print(json.dumps(dict(folder=sys.argv[1], mode='savefiles' if scenario == 'savefiles' else
    'open' if scenario in ('open', 'folder', 'multiple') else 'save',
    multiple=scenario == 'multiple', directory=scenario == 'folder', name='caller.txt')))
PY
        )
        status=0
        output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
            HOME="$phase/home" XDG_CONFIG_HOME="$phase/home/.config" XDG_STATE_HOME="$phase/state" \
            XDG_CACHE_HOME="$phase/cache" XDG_DATA_HOME="$phase/data" XDG_RUNTIME_DIR="$phase/runtime" TMPDIR="$phase/tmp" \
            DBUS_SYSTEM_BUS_ADDRESS="unix:path=$phase/runtime/no-system-bus" \
            FLEA_BIN="$PWD/target/debug/flea" FLEA_PICKER="$request" FLEA_PICKER_REPLY="$phase/reply.json" \
            FLEA_SAVE_NAME_CASE="$scenario" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
            QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
            dbus-run-session -- timeout --kill-after=2 15 qs -p "$scratch/config" 2>&1) || status=$?
        printf 'SAVE_NAME CASE %s %s exit=%s\n%s\n' "$view" "$scenario" "$status" "$output"
        phases=$((phases + 1))
        if [ "$status" -ne 0 ] || [ "$(grep -c 'SAVE_NAME DONE .* checks, 0 failed' <<< "$output")" -ne 1 ] \
            || grep -E 'SAVE_NAME FAIL|WARN|ERROR|TypeError|ReferenceError' <<< "$output" \
                | grep -vF 'This plugin does not support setting window masks' | grep -q .; then
            failures=$((failures + 1))
        fi
        if [ "$scenario" = collision ]; then
            if ! python3 - "$phase/reply.json" "$scratch/fixture/archive.tar.gz" <<'PY'
import json, sys
from pathlib import Path
assert json.loads(Path(sys.argv[1]).read_text()) == dict(response=0, uris=[Path(sys.argv[2]).as_uri()], filter=-1)
PY
            then failures=$((failures + 1)); fi
        elif [ -e "$phase/reply.json" ]; then
            echo 'FAIL selection unexpectedly wrote a portal reply'
            failures=$((failures + 1))
        fi
    done
done
for name in a-first.txt archive.tar.gz plain '.env' '日本語 résumé.txt'; do
    if [ "$(< "$scratch/fixture/$name")" != "unchanged $name" ]; then
        echo 'FAIL picker changed fixture bytes'
        failures=$((failures + 1))
    fi
done
printf 'picker-save-name: %s phases, %s failed\n' "$phases" "$failures"
[ "$failures" -eq 0 ]
