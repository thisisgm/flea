#!/usr/bin/env bash
# Exercise the real rail drop receiver and a device-blind negative mutant offscreen.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1
repo=$PWD
. "$repo/tools/flea-sandbox-guard"
. "$repo/tests/headless-commons.sh"
sandbox_root_ok
mkdir -p "$SANDBOX_ROOT" || exit 1
probe=$(mktemp -d "$SANDBOX_ROOT/probe.XXXXXXXX") || exit 1
: > "$probe/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$probe"; }
trap cleanup EXIT
mkdir -p "$probe"/{config,home,state,cache,runtime,undo,fixture/source,fixture/favorite,fixture/usb} || exit 1
chmod 700 "$probe/runtime" "$probe/undo"
for name in a b c; do printf '%s incoming\n' "$name" > "$probe/fixture/source/$name.txt"; done
printf 'occupied\n' > "$probe/fixture/favorite/c.txt"
ln -s "$repo/ui" "$probe/config/flea"
headless_commons "$probe" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$probe/config/Ui"
ln -s "$repo/ui/boot/fleatab.qml" "$probe/config/fleatab.qml"
cp tests/raildrop.qml "$probe/config/shell.qml"
# The caller may name the build under test, as tests/ui-captures-drops.sh does.
bin=${FLEA_BIN:-$repo/target/debug/flea}
run_probe() {
    env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
        HOME="$probe/home" XDG_STATE_HOME="$probe/state" XDG_CONFIG_HOME="$probe/config" XDG_DATA_HOME="$probe/home/data" \
        XDG_CACHE_HOME="$probe/cache" XDG_RUNTIME_DIR="$probe/runtime" \
        FLEA_UNDO_DIR="$probe/undo" FLEA_BIN="$bin" RAILDROP_FIXTURE="$probe/fixture" \
        QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_QUICK_BACKEND=software QT_FORCE_STDERR_LOGGING=1 \
        timeout 45 dbus-run-session -- qs -p "$probe/config" > "$1" 2>&1
}
run_probe "$probe/healthy.log"
status=$?
warnings=$(grep -E 'TypeError|ReferenceError|Binding loop|ERROR|WARN' "$probe/healthy.log" | grep -vF 'This plugin does not support setting window masks' || true)
# Sample input: "  INFO qml: RAILDROP DONE checks=63 failed=0" proves the full fixture flow ran.
if { [ "$status" -ne 0 ] && [ "$status" -ne 143 ]; } || ! grep -q 'RAILDROP DONE checks=[1-9][0-9]* failed=0$' "$probe/healthy.log" \
    || grep -q 'RAILDROP FAIL' "$probe/healthy.log" || [ -n "$warnings" ]; then
    cat "$probe/healthy.log"
    printf 'FAIL raildrop healthy probe exit=%s\n' "$status"
    exit 1
fi
grep 'RAILDROP ' "$probe/healthy.log"
cmp -s "$probe/fixture/favorite/c.txt" <(printf 'occupied\n') || { echo 'FAIL raildrop collision changed the old file'; exit 1; }
for name in a b c; do
    cmp -s "$probe/fixture/source/$name.txt" <(printf '%s incoming\n' "$name") \
        || { printf 'FAIL raildrop source changed: %s\n' "$probe/fixture/source/$name.txt"; exit 1; }
done
# The existing probe calls backend undo directly; this one delivers z and Ctrl+Z to a real window.
seed=$(printf '{"places":{"rail":"shown","autoHide":false,"favourites":[{"label":"Favorite","path":"%s/favorite"}]},"preview":{"column":false,"thumbnails":"off"},"updates":{"autoCheck":false}}' "$probe/fixture")
env HOME="$probe/home" XDG_STATE_HOME="$probe/state" "$bin" --ui-state "$seed" >/dev/null || exit 1
cp tests/raildrop-undo.qml "$probe/config/shell.qml"
FLEA_PATH="$probe/fixture/source" run_probe "$probe/undo.log"
status=$?
grep 'RAILUNDO ' "$probe/undo.log"
if { [ "$status" -ne 0 ] && [ "$status" -ne 143 ]; } || ! grep -q 'RAILUNDO DONE checks=[1-9][0-9]* failed=0$' "$probe/undo.log" \
    || grep -qE 'RAILUNDO FAIL|TypeError|ReferenceError|Binding loop|ERROR' "$probe/undo.log"; then
    cat "$probe/undo.log"
    printf 'FAIL raildrop keyboard undo probe exit=%s\n' "$status"
    exit 1
fi
cmp -s "$probe/fixture/source/a.txt" <(printf 'a incoming\n') && [ ! -e "$probe/fixture/favorite/a.txt" ] \
    || { echo 'FAIL raildrop keyboard undo changed the source or kept the destination'; exit 1; }
cp tests/raildrop.qml "$probe/config/shell.qml"
# The mutant ignores the matched destination device, so the same-device favorite wrongly copies.
cp -a ui "$probe/mutant-ui"
mutant="$probe/mutant-ui/js/RailDrop.js"
body=$(cat -- "$mutant") || exit 1
needle='held && held.path === path ? held.dev : 0'
rest=${body#*"$needle"}
[[ "$body" == *"$needle"* && "$rest" != *"$needle"* ]] \
    || { echo 'FAIL raildrop mutant seam must occur exactly once'; exit 1; }
printf '%s\n' "${body/"$needle"/0}" > "$mutant" || exit 1
[[ -n "$probe" && "$probe" == /* && -f "$probe/$SANDBOX_MARKER" ]] || exit 1
rm -- "$probe/config/flea"
ln -s "$probe/mutant-ui" "$probe/config/flea"
run_probe "$probe/mutant.log"
status=$?
if { [ "$status" -eq 0 ] || [ "$status" -eq 143 ]; } && grep -q 'RAILDROP FAIL source presence follows the chosen verb' "$probe/mutant.log" \
    && grep -q 'RAILDROP DONE checks=[1-9][0-9]* failed=[1-9][0-9]*$' "$probe/mutant.log"; then
    printf 'raildrop: healthy file drops, collision card and undo passed; device-blind mutant rejected\n'
else
    cat "$probe/mutant.log"
    printf 'FAIL raildrop negative mutant did not reach the wrong-source assertion, exit=%s\n' "$status"
    exit 1
fi
