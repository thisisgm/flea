#!/usr/bin/env bash
# Exercise the shipped chooser and real backend through native Qt events in both views.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tests/headless-commons.sh"
. "$PWD/tools/flea-sandbox-guard"
for tool in qs python3 timeout; do
    command -v "$tool" >/dev/null || { printf 'FAIL picker-search needs %s\n' "$tool" >&2; exit 1; }
done
backend="$PWD/target/debug/flea"
[ -x "$backend" ] || { printf 'FAIL picker-search needs target/debug/flea; run cargo build first\n' >&2; exit 1; }
# The same probe can judge a baseline UI without replacing this checkout's implementation.
ui_source=${FLEA_PICKER_TEST_UI:-"$PWD/ui"}
[ -d "$ui_source" ] || { printf 'FAIL picker-search UI directory not found: %s\n' "$ui_source" >&2; exit 1; }
sandbox_root_ok
test_root=$(mktemp -d "$SANDBOX_ROOT/flea-picker-search.XXXXXXXX") || exit 1
: > "$test_root/$SANDBOX_MARKER"
trap 'sandbox_remove "$test_root"' EXIT
mkdir -p "$test_root/config" "$test_root/fixture/z-folder" "$test_root/wide/z-folder"
cp -a "$ui_source" "$test_root/config/flea" || exit 1
# Keep the real saved-reply/worker shutdown; only replace its final self-signal with a clean probe exit.
python3 - "$test_root/config/flea/PickerWindow.qml" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
source = path.read_text()
stopped = 'onStopped: Quickshell.execDetached(["kill", String(Quickshell.processId)])'
if source.count(stopped) != 1:
    sys.exit('FAIL picker-search could not identify owned picker teardown')
path.write_text(source.replace(stopped, 'onStopped: Qt.exit(0)'))
PY
[ "$?" -eq 0 ] || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f "$ui_source/boot/Ui")" "$test_root/config/Ui"
cp tests/picker-search.qml "$test_root/config/shell.qml"
python3 - "$test_root/fixture" "$test_root/wide" <<'PY'
from pathlib import Path
import sys
from xml.etree.ElementTree import Element, SubElement, ElementTree
fixture, wide = map(Path, sys.argv[1:])
for name in ['a-pick.txt', 'b-hidden.txt', 'c-pick.txt', 'd-hidden.png', 'e-pick.png']:
    (fixture / name).write_text(name + '\n')
(fixture / 'z-folder/nested-pick.txt').write_text('not an immediate child\n')
for index in range(400):
    (wide / f'a-loaded-{index:03}.txt').write_text('loaded-window scope\n')
(wide / 'zz-unloaded-only.txt').write_text('outside the initial held window\n')
(wide / 'z-folder/nested-unloaded.txt').write_text('never recursively searched\n')
history = Element('xbel', version='1.0')
for path in sorted(wide.iterdir()):
    if path.is_file():
        SubElement(history, 'bookmark', href=path.as_uri(), visited='2026-10-16T12:00:00Z')
ElementTree(history).write(wide.parent / 'recently-used.xbel', encoding='utf-8', xml_declaration=True)
PY
[ "$?" -eq 0 ] || exit 1
failures=0
phases=0
for view in list grid; do
    for scenario in activate escape navigate chips all range scope recent; do
        phase="$test_root/$view-$scenario"
        mkdir -p "$phase"/{home,state/flea,cache,data,runtime,tmp}
        chmod 700 "$phase/runtime"
        printf '{"pickerView":"%s","keys":"default"}' "$view" > "$phase/state/flea/ui.json"
        fixture="$test_root/fixture"
        case "$scenario" in scope|recent) fixture="$test_root/wide" ;; esac
        if [ "$scenario" = recent ]; then cp "$test_root/recently-used.xbel" "$phase/data/recently-used.xbel"; fi
        request=$(python3 - "$fixture" "$scenario" <<'PY'
import json
import sys
request = dict(mode='open', folder=sys.argv[1], multiple=sys.argv[2] in ['all', 'range'], title='Picker filename filter')
if sys.argv[2] == 'chips':
    request['filters'] = [dict(label='Text', globs=['*.txt'], mimes=[]), dict(label='Pictures', globs=['*.png'], mimes=[])]
print(json.dumps(request))
PY
        )
        output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
            HOME="$phase/home" XDG_STATE_HOME="$phase/state" XDG_CONFIG_HOME="$phase/home/.config" \
            XDG_CACHE_HOME="$phase/cache" XDG_DATA_HOME="$phase/data" XDG_RUNTIME_DIR="$phase/runtime" TMPDIR="$phase/tmp" \
            FLEA_BIN="$backend" FLEA_PICKER="$request" FLEA_PICKER_REPLY="$phase/reply.json" \
            FLEA_PICKER_SEARCH_CASE="$scenario" FLEA_PICKER_SEARCH_VIEW="$view" \
            QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software \
            QT_FORCE_STDERR_LOGGING=1 timeout 15 qs -p "$test_root/config" 2>&1)
        code=$?
        phases=$((phases + 1))
        printf 'PICKER_SEARCH CASE %s %s exit=%s\n' "$view" "$scenario" "$code"
        printf '%s\n' "$output"
        if [ "$code" -ne 0 ] || [[ "$output" == *'PICKER_SEARCH FAIL'* ]] || [[ ! "$output" =~ PICKER_SEARCH\ DONE\ checks=[0-9]+\ failed=0 ]]; then
            printf 'FAIL picker-search %s %s did not reach a clean verdict\n' "$view" "$scenario"
            failures=$((failures + 1))
        fi
        if [[ "$output" =~ TypeError|ReferenceError|Unable\ to\ assign|Cannot\ anchor\ to\ an\ item|Binding\ loop\ detected|Failed\ to\ load\ configuration ]]; then
            printf 'FAIL picker-search %s %s emitted a QML warning\n' "$view" "$scenario"
            failures=$((failures + 1))
        fi
        case "$scenario" in
            activate|all|range|escape|recent)
                if ! python3 - "$phase/reply.json" "$fixture" "$scenario" <<'PY'
import json
from pathlib import Path
import sys
try:
    reply = json.loads(Path(sys.argv[1]).read_text())
except (OSError, ValueError):
    sys.exit('FAIL picker-search did not save a portal reply')
if sys.argv[3] == 'escape':
    expected = dict(response=1)
    if reply != expected:
        sys.exit('FAIL picker-search final Escape did not cancel cleanly')
else:
    names = ['c-pick.txt'] if sys.argv[3] == 'activate' else ['a-pick.txt', 'c-pick.txt', 'e-pick.png']
    if sys.argv[3] == 'recent':
        names = ['a-loaded-001.txt']
    expected = [(Path(sys.argv[2]) / name).as_uri() for name in names]
    if reply.get('response') != 0 or sorted(reply.get('uris', [])) != sorted(expected):
        sys.exit('FAIL picker-search accepted the wrong filtered file identities')
PY
                then failures=$((failures + 1)); fi
                ;;
            *)
                if [ -e "$phase/reply.json" ]; then
                    printf 'FAIL picker-search %s %s answered before explicit acceptance\n' "$view" "$scenario"
                    failures=$((failures + 1))
                fi
                ;;
        esac
    done
done
printf 'picker-search: %s phases, %s failed\n' "$phases" "$failures"
[ "$failures" -eq 0 ]
