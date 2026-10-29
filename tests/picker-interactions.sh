#!/usr/bin/env bash
# Chooser mouse parity in both modes and views; all files and settings are disposable.
set -euo pipefail
. "$(dirname "$0")/../tools/flea-sandbox-guard"
. "$(dirname "$0")/headless-commons.sh"
cd "$(dirname "$0")/.."
[ -x target/debug/flea ] || cargo build --locked -q
root="$FIXTURE_ROOT/flea-pickinput-$$"
sandbox_make "$root"
trap 'sandbox_remove "$root"' EXIT
mkdir -p "$root"/{config,fixture/sub}
ln -s "$PWD/ui" "$root/config/flea"
headless_commons "$root"
ln -s "$(readlink -f ui/boot/Ui)" "$root/config/Ui"
cp tests/picker-interactions.qml "$root/config/shell.qml"
printf 'hidden\n' > "$root/fixture/.secret"
printf 'hidden\n' > "$root/fixture/sub/.secret"
for n in $(seq -w 0 99); do printf 'file\n' > "$root/fixture/sub/file$n.txt"; done
for mode in save open; do
    for view in list grid; do
        phase="$root/$mode-$view"
        mkdir -p "$phase"/{home,state/flea,cache,data,runtime,tmp}
        chmod 700 "$phase/runtime"
        printf '{"pickerView":"%s","hidden":false,"keys":"default","places":{"favourites":[]},"preview":{"thumbSize":"medium","ctrlZoom":true}}\n' "$view" > "$phase/state/flea/ui.json"
        request=$(python3 - "$mode" "$root/fixture" <<'PY'
import json, sys
print(json.dumps(dict(mode=sys.argv[1], folder=sys.argv[2], name='report.pdf', title='Chooser interactions')))
PY
        )
        set +e
        output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
            HOME="$phase/home" XDG_STATE_HOME="$phase/state" XDG_CONFIG_HOME="$phase/home/.config" \
            XDG_CACHE_HOME="$phase/cache" XDG_DATA_HOME="$phase/data" XDG_RUNTIME_DIR="$phase/runtime" TMPDIR="$phase/tmp" \
            FLEA_BIN="$PWD/target/debug/flea" FLEA_PICKER="$request" FLEA_PICKER_REPLY="$phase/reply.json" \
            QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
            timeout 30 qs -p "$root/config" 2>&1)
        status=$?
        set -e
        printf 'picker-interactions: %s %s exit=%s\n' "$mode" "$view" "$status"
        printf '%s\n' "$output" | grep 'PICKERINPUT' || true
        warnings=$(printf '%s\n' "$output" | grep -aE 'WARN|ERROR|TypeError|ReferenceError|Unable to assign' \
            | grep -vF '$HYPRLAND_INSTANCE_SIGNATURE is unset. Cannot connect to hyprland.' \
            | grep -vF 'This plugin does not support setting window masks' || true)
        if [[ "$status" != 143 ]] || [[ -n "$warnings" ]] \
            || ! printf '%s\n' "$output" | grep -aqE 'PICKERINPUT DONE checks=[1-9][0-9]* failed=0'; then
            printf '%s\n' "$output"
            exit 1
        fi
        python3 - "$phase/reply.json" <<'PY'
import json, sys
assert json.load(open(sys.argv[1])) == {'response': 1}, 'Chooser must remain cancellable'
PY
    done
done
[[ ! -e "$root/fixture/sub/report.pdf" ]] || { echo 'FAIL chooser wrote save contents'; exit 1; }
printf 'picker-interactions: passed\n'
