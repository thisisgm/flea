#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# Drive production previews offscreen with the real 120ms timer, then repeat in seeded manual mode.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
# qs exits 128 + SIGTERM when the probe kills itself after its receipt.
self_kill_exit=143

if ! command -v qs >/dev/null; then
    echo "preview-settle-live.sh: qs is not installed, cannot drive the preview"
    exit 1
fi

if ! command -v ffmpeg >/dev/null; then
    printf 'FAIL preview settle: ffmpeg is required for the marked JPEG fixture\n'
    exit 1
fi

run_phase() {
    local seed="$1" label="$2" attempt="${3:-1}" output code pass_count fail_count done_count clean_count expected warnings index
    sandbox_root_ok
    test_root=$(mktemp -d "$SANDBOX_ROOT/flea-preview-settle.XXXXXX") || exit 1
    : > "$test_root/$SANDBOX_MARKER" || exit 1
    mkdir -p "$test_root"/{config,home,state/flea,runtime,tmp,images} || exit 1
    chmod 700 "$test_root/runtime" || exit 1
    : > "$test_root/images/$SANDBOX_MARKER" || exit 1
    if ! ffmpeg -nostdin -hide_banner -loglevel error -f lavfi -i color=c=white:s=16x16 \
        -frames:v 1 -threads 1 "$test_root/images/img0.jpg" > "$test_root/image.log" 2>&1; then
        cat "$test_root/image.log"
        exit 1
    fi
    for index in {1..7}; do
        cp -- "$test_root/images/img0.jpg" "$test_root/images/img$index.jpg" || exit 1
    done
    ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
    headless_commons "$test_root" || exit 1
    ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
    cp tests/preview-settle-live.qml "$test_root/config/shell.qml" || exit 1
    if [ -n "$seed" ]; then
        printf '%s' "$seed" > "$test_root/state/flea/ui.json" || exit 1
    fi
    if [ "$label" = manual ]; then
        output=$(FLEA_PREVIEWSETTLE_MANUAL=1 FLEA_PREVIEWSETTLE_DIR="$test_root/images" FLEA_PREVIEWSETTLE_ATTEMPT="$attempt" \
            env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
            HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
            XDG_CONFIG_HOME="$test_root/home/.config" XDG_CACHE_HOME="$test_root/home/.cache" TMPDIR="$test_root/tmp" \
            QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
            timeout 60 qs -p "$test_root/config" 2>&1)
    else
        output=$(FLEA_PREVIEWSETTLE_DIR="$test_root/images" FLEA_PREVIEWSETTLE_ATTEMPT="$attempt" \
            env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
            HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
            XDG_CONFIG_HOME="$test_root/home/.config" XDG_CACHE_HOME="$test_root/home/.cache" TMPDIR="$test_root/tmp" \
            QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
            timeout 60 qs -p "$test_root/config" 2>&1)
    fi
    code=$?
    sandbox_remove "$test_root"
    warnings=$(printf '%s\n' "$output" | grep -aiE 'WARN|ERROR|TypeError|ReferenceError|not ready|not a type|is not defined|file not found' || true)
    # A burst the box stalled past the settle window voids an attempt that printed no failure and no warning and ended by its own kill, so the phase runs again, three attempts at most.
    if [ "$attempt" -lt 3 ] && [ "$code" -eq "$self_kill_exit" ] && [ -z "$warnings" ] \
        && printf '%s\n' "$output" | grep -aq 'PREVIEWSETTLE VOID' && ! printf '%s\n' "$output" | grep -aq 'PREVIEWSETTLE FAIL'; then
        printf '%s\n' "$output" | grep -a 'PREVIEWSETTLE VOID'
        return 2
    fi
    # Only the probe's self-termination after one exact clean receipt proves completion.
    pass_count=$(printf '%s\n' "$output" | grep -ac 'PREVIEWSETTLE PASS')
    fail_count=$(printf '%s\n' "$output" | grep -ac 'PREVIEWSETTLE FAIL')
    done_count=$(printf '%s\n' "$output" | grep -ac 'PREVIEWSETTLE DONE')
    clean_count=$(printf '%s\n' "$output" | grep -acE 'PREVIEWSETTLE DONE failures=0$')
    expected=47
    [ "$label" = manual ] && expected=3
    if [ "$code" -ne "$self_kill_exit" ] || [ "$done_count" -ne 1 ] || [ "$clean_count" -ne 1 ] \
        || [ "$pass_count" -ne "$expected" ] || [ "$fail_count" -ne 0 ] || [ -n "$warnings" ]; then
        printf 'FAIL preview settle %s: qs_exit=%s done=%s clean=%s pass=%s/%s fail=%s\n' \
            "$label" "$code" "$done_count" "$clean_count" "$pass_count" "$expected" "$fail_count"
        printf '%s\n' "$output"
        exit 1
    fi
    printf '%s\n' "$output" | grep -a 'PREVIEWSETTLE DONE'
    printf 'PREVIEWSETTLE STATUS phase=%s qs_exit=%s done=1 pass=%s fail=0\n' "$label" "$code" "$pass_count"
}

test_root=""
cleanup() { [ -z "$test_root" ] || sandbox_remove "$test_root"; }
trap cleanup EXIT
# A voided attempt returns 2 and the phase runs again from this loop, so run_phase never calls itself.
settle_phase() {
    local attempt status
    for attempt in 1 2 3; do
        status=0
        run_phase "$1" "$2" "$attempt" || status=$?
        [ "$status" -eq 2 ] || return "$status"
    done
    printf 'FAIL preview settle %s: every attempt voided\n' "$2"
    exit 1
}
settle_phase "" auto
settle_phase '{"preview":{"loadOn":"manual"}}' manual
