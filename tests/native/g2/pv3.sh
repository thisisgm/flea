#!/bin/bash
# Native media run: the bench-seam flea-gui drives its media kit from a FIFO and records presentation, CPU and PSS.
# Usage: tests/native/g2/pv3.sh /absolute/flea-gui-bench-seam
set -euo pipefail
binary=${1:?absolute path to a bench-seam flea-gui}
case $binary in /*) ;; *) echo 'binary path must be absolute' >&2; exit 2 ;; esac
[ -x "$binary" ] || { echo 'binary is not executable' >&2; exit 2; }
for tool in ffmpeg mkfifo bwrap; do command -v "$tool" > /dev/null || { echo "missing $tool" >&2; exit 2; }; done
here=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../../.." && pwd -P)
mkdir -p "$here/.superpowers/g2-pv3"
root=$(mktemp -d "$here/.superpowers/g2-pv3/native.XXXXXX")
case $root in "$here"/.superpowers/g2-pv3/native.?*) ;; *) echo 'invalid scratch root' >&2; exit 2 ;; esac
printf 'pv3\n' > "$root/.flea-pv3-kit-root"

# The boards' Media fixture plus the performance clips; every file is made fresh inside the marked root.
clip() {
    local name=$1 size=$2 seconds=$3 codec=$4
    [ ! -e "$root/$name" ] || { echo "refused existing $name" >&2; exit 2; }
    local video=(-c:v libx264 -preset ultrafast -crf 28 -c:a aac)
    [ "$codec" = h264 ] || video=(-c:v libvpx -deadline realtime -cpu-used 8 -b:v 8M -c:a libvorbis)
    ffmpeg -nostdin -v error -n -f lavfi -i "testsrc2=size=$size:rate=30" -f lavfi -i "sine=frequency=440:sample_rate=48000" \
        -t "$seconds" -pix_fmt yuv420p "${video[@]}" "$root/$name"
}
clip clip.mp4 320x240 2 h264
clip h264-1080.mp4 1920x1080 12 h264
clip h264-2160.mp4 3840x2160 12 h264
clip vp8-1080.webm 1920x1080 12 vp8
clip vp8-2160.webm 3840x2160 12 vp8
clip long.mp4 1280x720 120 h264
ffmpeg -nostdin -v error -n -f lavfi -i 'sine=frequency=440:sample_rate=44100' -t 2 -c:a libvorbis "$root/tone.ogg"
printf 'not a clip\n' > "$root/unplayable.mp4"

run_kit() {
    local out="$root/mpv"
    mkdir "$out"
    printf 'pv3\n' > "$out/.flea-pv3-kit-root"
    mkfifo "$out/control"
    "$binary" --media-kit window --fifo "$out/control" --records "$out/records.jsonl" \
        > "$out/kit.log" 2>&1 &
    local kit=$!
    exec 7> "$out/control"
    say() { printf '%s\n' "$*" >&7; }
    settle() { sleep "${1:-2}"; }
    settle 3
    say phase startup
    say phase rest
    for file in h264-1080.mp4 h264-2160.mp4 vp8-1080.webm vp8-2160.webm; do
        for round in 1 2 3; do
            say open "$root/$file"
            settle 3
            say state
            [ "$round" != 1 ] || { say sustain 5; settle 6; }
            say close
            settle 1
        done
    done
    say open "$root/clip.mp4"
    settle 1
    say phase selected
    settle 2
    say phase open
    say toggle; settle 1; say state
    say toggle; settle 1; say state
    say seek 1000; settle 1; say state
    say mute; settle 1; say state
    say mute; say volume 40; settle 1; say state
    settle 3; say state
    say toggle; settle 1; say state
    say close; settle 2
    say phase closed
    say open "$root/tone.ogg" audio; settle 3; say state
    say open "$root/unplayable.mp4"; settle 3; say state
    say open "$root/long.mp4"; settle 30; say sustain 20; settle 21; say state
    say close; settle 2
    say quit
    exec 7>&-
    wait "$kit" || true
}

run_kit
printf 'evidence=%s\n' "$root"
