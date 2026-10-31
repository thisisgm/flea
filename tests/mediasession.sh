#!/usr/bin/env bash
. "$(dirname "$0")/headless-commons.sh"
# The real PreviewMedia and Quick Look against a stub MPRIS helper, offscreen: the idle inhibitor is on only while a video plays, the helper starts after play and gets one state line per change and none per position tick, its commands reach the player, and closing the player ends the helper.
set -u
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

for tool in qs ffmpeg; do
    command -v "$tool" >/dev/null || { printf 'mediasession.sh: %s is not installed, cannot run\n' "$tool"; exit 1; }
done

# A marked sandbox of its own under the fixture root, so cleanup deletes only what this run owns.
test_root=$(mktemp -d "$FIXTURE_ROOT/flea-mediasession-XXXXXX") || exit 1
# GNU mktemp -d keeps a relative TMPDIR verbatim, so the path is checked absolute and two components deep.
case $test_root in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$test_root', which is not an absolute path two components deep"; exit 1 ;;
esac
sandbox_require "$test_root" || exit 1
: > "$test_root/$SANDBOX_MARKER" || exit 1
cleanup() { sandbox_remove "$test_root"; }
trap cleanup EXIT

mkdir -p "$test_root/config" "$test_root/home" "$test_root/state/flea" "$test_root/runtime" "$test_root/bin" "$test_root/stub" "$test_root/media" || exit 1
chmod 700 "$test_root/runtime" || exit 1
ln -s "$PWD/ui" "$test_root/config/flea" || exit 1
headless_commons "$test_root" || exit 1
ln -s "$(readlink -f ui/boot/Ui)" "$test_root/config/Ui" || exit 1
cp tests/mediasession.qml "$test_root/config/shell.qml" || exit 1
printf '{}\n' > "$test_root/state/flea/ui.json" || exit 1

# A 4 s clip with a picture and a sound, and a 4 s tone with neither a picture nor a cover.
ffmpeg -v error -f lavfi -i testsrc=size=64x64:rate=10:duration=4 -f lavfi -i sine=frequency=440:duration=4 \
    -c:v mpeg4 -c:a aac -shortest "$test_root/media/clip.mp4" || { echo "FAIL ffmpeg could not make the clip"; exit 1; }
cp "$test_root/media/clip.mp4" "$test_root/media/next.mp4" || exit 1
ffmpeg -v error -f lavfi -i sine=frequency=440:duration=4 -c:a libvorbis "$test_root/media/tone.ogg" \
    || { echo "FAIL ffmpeg could not make the tone"; exit 1; }

# Stands for tools/flea-mpris: records its pid and every stdin line, and prints each line appended to cmds.log.
cat > "$test_root/bin/stub-mpris" <<'STUB'
#!/bin/sh
printf '%s\n' "$$" >> "$STUB_DIR/starts.log"
tail --pid=$$ -n 0 -f "$STUB_DIR/cmds.log" &
while IFS= read -r line; do printf '%s\n' "$line" >> "$STUB_DIR/lines.log"; done
STUB
chmod +x "$test_root/bin/stub-mpris" || exit 1
: > "$test_root/stub/cmds.log" || exit 1

output=$(env -u DISPLAY -u WAYLAND_DISPLAY -u HYPRLAND_INSTANCE_SIGNATURE \
    FLEA_MPRIS="$test_root/bin/stub-mpris" STUB_DIR="$test_root/stub" MEDIASESSION_MEDIA="$test_root/media" \
    HOME="$test_root/home" XDG_STATE_HOME="$test_root/state" XDG_RUNTIME_DIR="$test_root/runtime" \
    QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME=generic QT_FORCE_STDERR_LOGGING=1 \
    timeout 90 qs -p "$test_root/config" 2>&1)
qs_status=$?

verdict=0
# Sample input: "  INFO qml: MEDIASESSION PASS 31 checks", then "MEDIASESSION DONE failures=0", then the probe's own SIGTERM (143).
pass_count=$(printf '%s\n' "$output" | grep -c 'MEDIASESSION PASS')
fail_count=$(printf '%s\n' "$output" | grep -c 'MEDIASESSION FAIL')
done_count=$(printf '%s\n' "$output" | grep -c 'MEDIASESSION DONE')
if [ "$qs_status" -ne 143 ]; then
    printf 'FAIL qs exited %s, want the owned self-kill 143 after DONE\n' "$qs_status"
    verdict=1
fi
if [ "$done_count" -ne 1 ]; then
    printf 'FAIL completion receipts %s, want exactly 1 DONE beside the PASS\n' "$done_count"
    verdict=1
fi
if [ "$pass_count" -ne 1 ] || [ "$fail_count" -ne 0 ]; then
    printf 'FAIL the media session missed its contract\n'
    printf '%s\n' "$output" | grep -aE 'MEDIASESSION|ERROR|error'
    verdict=1
fi
# The lane has no sound server and the offscreen platform cannot mask a FloatingWindow; these lines are the lane's, never the player's.
warnings=$(printf '%s\n' "$output" | grep -aE 'TypeError|ReferenceError|WARN|ERROR' \
    | grep -vF -e 'This plugin does not support setting window masks' -e 'No audio device detected' -e 'pa_context_connect() failed')
if [ -n "$warnings" ]; then
    printf 'FAIL the harness logged a warning\n'
    printf '%s\n' "$warnings"
    verdict=1
fi
if [ "$verdict" -ne 0 ]; then
    printf '%s\n' "$output"
    exit 1
fi
printf '%s\n' "$output" | grep -o 'MEDIASESSION PASS.*'
printf 'MEDIASESSION STATUS qs_exit=%s done=1\n' "$qs_status"
