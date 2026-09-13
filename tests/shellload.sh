#!/bin/bash
# Loads the real shell the way a launch does, and fails on any QML error. Nothing else headless does
# this: an assignment to a property that no longer exists is a load error for the WHOLE shell, so
# Flea opens no window at all, and 0.1.4 reached a benchmark in that state with qmllint reporting 0
# regressions, tests/run-all.sh green across 22 suites, and cargo clean of warnings.
set -u
cd "$(dirname "$0")/.." || exit 1

pass=0
fail=0
ok()  { printf 'ok   %s\n' "$*"; pass=$((pass+1)); }
bad() { printf 'FAIL %s\n' "$*"; fail=$((fail+1)); }

if ! command -v qs >/dev/null; then
    echo "shellload.sh: qs is not installed, cannot load the shell"
    exit 1
fi

. "$PWD/tools/flea-sandbox-guard"
sandbox_forbidden /tmp && sandbox_refuse "shellload: /tmp is inside a forbidden test target"
shellload_root=$(mktemp -d /tmp/flea-shellload.XXXXXXXX) || exit 1
FIXTURE_ROOT=$shellload_root
sandbox_root_ok
shellload_root=$SANDBOX_ROOT
readonly shellload_root
printf 'Flea shell load sandbox\n' > "$shellload_root/$SANDBOX_MARKER" || exit 1
shellload_work="$shellload_root/work"
readonly shellload_work
cleanup() {
    local result=$?
    trap - EXIT
    # sandbox_remove verifies an absolute, non-empty path contained in this run's marked root before rm.
    sandbox_remove "$shellload_work"
    exit "$result"
}
trap cleanup EXIT
trap 'exit 130' INT
trap 'exit 143' TERM
sandbox_scratch "$shellload_work"
mkdir -p "$shellload_work"/{home,config,state,data,cache,runtime,tmp,fixture} || exit 1
chmod 700 "$shellload_work/runtime" || exit 1
# Copy only Omarchy's actual theme inputs; the real product imports and settings readers stay intact.
for relative in .local/state/omarchy/current/theme/colors.toml .local/state/omarchy/current/theme/shell.toml \
                .local/state/omarchy/current/theme.name .config/omarchy/shell.toml; do
    if [ -f "$HOME/$relative" ]; then
        mkdir -p "$shellload_work/home/$(dirname "$relative")" || exit 1
        cp -- "$HOME/$relative" "$shellload_work/home/$relative" || exit 1
    fi
done
shellload_bin=${FLEA_BIN:-$PWD/target/debug/flea}
[ -x "$shellload_bin" ] || { echo "shellload.sh: build the candidate backend first: $shellload_bin"; exit 1; }
log="$shellload_root/shell.log"
sandbox_require "$log"

# Offscreen and with no compositor, so this needs neither the display nor the display lock. A shell
# does not exit on its own, so the timeout expiring is the success path and 124 is not a failure.
# Seconds: generous enough for a cold QML compile on a loaded box, short enough for the battery.
load_seconds=25
env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE -u FLEA_SELECT \
    HOME="$shellload_work/home" XDG_CONFIG_HOME="$shellload_work/config" \
    XDG_STATE_HOME="$shellload_work/state" XDG_DATA_HOME="$shellload_work/data" \
    XDG_CACHE_HOME="$shellload_work/cache" XDG_RUNTIME_DIR="$shellload_work/runtime" TMPDIR="$shellload_work/tmp" \
    FLEA_PATH="$shellload_work/fixture" FLEA_BIN="$shellload_bin" \
    QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
    timeout "$load_seconds" qs -p "$PWD/ui" >"$log" 2>&1
status=$?

if [ "$status" -eq 124 ]; then
    ok "the loaded shell stayed alive until the existing timeout"
else
    bad "qs exited before the load window completed (exit $status)"
fi

# Sample input, one Quickshell log line: '  INFO: Configuration Loaded'
if grep -q 'Configuration Loaded' "$log"; then
    ok "the shell loads: qs reported Configuration Loaded"
else
    bad "qs never reported Configuration Loaded, so the shell did not load (timeout exit $status)"
fi

# Sample input, the 0.1.4 failure: 'ERROR:   caused by @ConvertDialog.qml[126:21]: Cannot assign to
# non-existent property "onHoverEntered"'. Colour codes sit before the level, so match it anywhere.
errors=$(grep -aciE 'ERROR|Failed to load configuration|unavailable|Cannot assign to non-existent' "$log")
if [ "$errors" -eq 0 ]; then
    ok "no QML error, no unavailable type, no assignment to a property that does not exist"
else
    bad "the shell logged $errors error line(s):"
    grep -aiE 'ERROR|unavailable|Cannot assign to non-existent' "$log" | head -5 | sed 's/^/     /'
fi

printf 'shellload: full log %s\n' "$log"
printf 'shellload: %s check(s), %s failed\n' "$((pass + fail))" "$fail"
[ "$fail" -eq 0 ]
