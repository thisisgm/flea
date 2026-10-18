#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# A press held across a watched re-read of an unchanged listing, or past the press-and-hold interval, must still tap on release, in every view.
set -u
cd "$(dirname "$0")/.." || exit 1
. "$PWD/tools/flea-sandbox-guard"
command -v qs >/dev/null || { echo 'FAIL presskeep needs qs'; exit 1; }
command -v dbus-run-session >/dev/null || { echo 'FAIL presskeep needs private D-Bus'; exit 1; }
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL presskeep needs the candidate backend'; exit 1; }
sandbox_root_ok
probe=$(mktemp -d "$SANDBOX_ROOT/flea-presskeep.XXXXXXXX") || exit 1
: > "$probe/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$probe"; }
trap cleanup EXIT
mkdir -p "$probe"/{home,config,state,data,cache,runtime} "$probe/home/fixture" || exit 1
chmod 700 "$probe/runtime" || exit 1
for name in a.txt b.txt c.txt d.txt; do printf 'press keep fixture\n' > "$probe/home/fixture/$name"; done
env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state \
    '{"view":"list","keys":"default","menu":{"hidden":[]},"preview":{"column":false,"thumbnails":"off"},"updates":{"autoCheck":false},"display":{"textSize":{"mode":14}}}' >/dev/null || exit 1
ln -s "$PWD/ui" "$probe/config/flea" || exit 1
headless_commons "$probe" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui" || exit 1
cp tests/presskeep.qml "$probe/config/shell.qml" || exit 1
ln -s "$PWD/ui/boot/fleatab.qml" "$probe/config/fleatab.qml" || exit 1
log="$probe/qs.log"
readonly runLimitSeconds=60 expectedChecks=51 expectedQsStatus=143
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
    HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" \
    XDG_DATA_HOME="$probe/data" XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
    FLEA_PATH="$probe/home/fixture" FLEA_BIN="$bin" GIO_USE_VOLUME_MONITOR=unix QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic \
    QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
    timeout "$runLimitSeconds" dbus-run-session -- qs -p "$probe/config" > "$log" 2>&1
qs_status=$?
cat "$log"
verdict=0
[[ "$qs_status" == "$expectedQsStatus" ]] || { printf 'FAIL qs exit %s, expected %s after backend drain\n' "$qs_status" "$expectedQsStatus"; verdict=1; }
[[ "$(rg -c 'PRESSKEEP DONE' "$log")" == 1 ]] || { echo 'FAIL expected exactly one completion receipt'; verdict=1; }
rg -q "PRESSKEEP DONE checks=$expectedChecks failed=0" "$log" || { echo 'FAIL presskeep tally'; verdict=1; }
[[ "$(rg -c 'PRESSKEEP ok ' "$log")" == "$expectedChecks" ]] || { echo 'FAIL presskeep passed tally'; verdict=1; }
if rg 'PRESSKEEP FAIL|TypeError|ReferenceError|ERROR|Cannot assign|WARN' "$log" | rg -vF 'This plugin does not support setting window masks'; then
    echo 'FAIL presskeep logged a failed check or engine warning'
    verdict=1
fi
printf 'PRESSKEEP STATUS qs_exit=%s\n' "$qs_status"
exit "$verdict"
