#!/usr/bin/env bash
# Real save window, real backend, exclusively marker-owned fixtures outside HOME.
set -euo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.."
[ -x target/debug/flea ] || cargo build --locked -q
root="$FIXTURE_ROOT/flea-picker-folder-$$"
sandbox_make "$root"
trap 'sandbox_remove "$root"' EXIT
mkdir -p "$root"/{config,home/Downloads/existing,state/flea,cache,data,runtime}
chmod 700 "$root/runtime"
printf '{"pickerView":"list","places":{"favourites":[]}}\n' > "$root/state/flea/ui.json"
ln -s "$PWD/ui" "$root/config/flea"
ln -s "$(readlink -f ui/boot/Commons)" "$root/config/Commons"
ln -s "$(readlink -f ui/boot/Ui)" "$root/config/Ui"
request=$(/usr/bin/python3 - "$root/home/Downloads" <<'PY'
import json,sys
print(json.dumps({'mode':'save','folder':sys.argv[1],'name':'download.zip','title':'Folder creation regression'}))
PY
)
for probe in picker-folder picker-folder-state; do
cp "tests/$probe.qml" "$root/config/shell.qml"
set +e
output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    HOME="$root/home" XDG_CONFIG_HOME="$root/config" XDG_STATE_HOME="$root/state" XDG_CACHE_HOME="$root/cache" \
    XDG_DATA_HOME="$root/data" XDG_RUNTIME_DIR="$root/runtime" FLEA_BIN="$PWD/target/debug/flea" \
    FLEA_PICKER="$request" FLEA_PICKER_REPLY="$root/reply.json" QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 timeout 25 qs -p "$root/config" 2>&1)
status=$?
set -e
printf '%s\n' "$output" | grep -E 'PICKERFOLDER|FOLDERSTATE' || true
warnings=$(printf '%s\n' "$output" | grep -aE 'WARN|ERROR|TypeError|ReferenceError|Cannot assign' \
    | grep -vF '$HYPRLAND_INSTANCE_SIGNATURE is unset. Cannot connect to hyprland.' \
    | grep -vF 'This plugin does not support setting window masks' || true)
if [[ "$status" != 0 && "$status" != 143 ]] || [[ -n "$warnings" ]] \
    || ! printf '%s\n' "$output" | grep -aqE '(PICKERFOLDER|FOLDERSTATE) DONE checks=[1-9][0-9]* failed=0'; then
    printf '%s\n' "$output"
    echo 'FAIL picker-folder: missing clean completion'
    exit 1
fi
done
[[ -d "$root/home/Downloads/Reports 雪" ]] || { echo 'FAIL directory not created'; exit 1; }
[[ ! -e "$root/home/Downloads/Reports 雪/download.zip" ]] || { echo 'FAIL save contents written'; exit 1; }
[[ ! -e "$root/home/Downloads/Reports 雪/Never created" ]] || { echo 'FAIL cancellation created a directory'; exit 1; }
/usr/bin/python3 - "$root/reply.json" <<'PY'
import json,sys
assert json.load(open(sys.argv[1]))['response'] == 1, 'Picker must remain cancellable after folder creation'
PY
printf 'picker-folder: passed\n'
