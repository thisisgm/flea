#!/usr/bin/env bash
# Drives the real backend's media probe. Two things it must do and once did not: answer a blocked
# input inside its own deadline, and disbelieve a probe that exited non-zero however plausible the
# output it printed looks.
set -u
set -o pipefail
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1

BIN=./target/debug/flea
FIXTURE="${FLEA_MEDIA_DIR:-$FIXTURE_ROOT/flea-media-btrfs}"
D="$FIXTURE_ROOT/flea-media-test-$$"
# src/backend/mediaprobe.rs PROBE_LIMIT is 5 s; this is that plus the slack a spawn and a debug build cost.
PROBE_DEADLINE=8
# Well past the deadline, so a probe with no wall bound at all is reported as no answer and not as a slow one.
ANSWER_BOUND=20
# Enough of a real container to keep ffprobe reading, and under the 64 KiB a pipe buffers, so the writer never blocks.
FED_BYTES=60000
# Longer than the whole suite: the writer's job is to hold the pipe open, never to close it.
WRITER_SECONDS=120
BACKEND_PID=""
WRITER_PID=""
STUB_PATH=""
fail=0

cleanup() {
  exec 3>&- 2>/dev/null
  exec 4>&- 2>/dev/null
  [ -n "$BACKEND_PID" ] && kill "$BACKEND_PID" 2>/dev/null
  [ -n "$WRITER_PID" ] && kill "$WRITER_PID" 2>/dev/null
  # A probe with no deadline outlives the backend that started it, and only this run knows its path.
  [ -n "$D" ] && pkill -f -- "$D" 2>/dev/null
  sandbox_remove "$D"
}
trap cleanup EXIT

check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

[ -d "$FIXTURE" ] || { echo "media.sh: the media fixture is missing at $FIXTURE"; exit 1; }
[ -f "$FIXTURE/clip_6.mp4" ] || { echo "media.sh: $FIXTURE/clip_6.mp4 is missing"; exit 1; }
[ -x "$BIN" ] || { echo "media.sh: $BIN is missing, run cargo build"; exit 1; }

sandbox_make "$D"
mkdir -p "$D/clip" "$D/blocked" "$D/stub"
cp "$FIXTURE/clip_6.mp4" "$D/clip/clip.mp4"
mkfifo "$D/blocked/pipe.mp4"

send() { printf '%s\n' "$1" >&3; }

start_backend() {
  rm -f "$D/out" "$D/in"; : > "$D/out"; mkfifo "$D/in"
  PATH="${STUB_PATH:+$STUB_PATH:}$PATH" $BIN --backend < "$D/in" > "$D/out" 2>/dev/null &
  BACKEND_PID=$!
  exec 3> "$D/in"
}

stop_backend() {
  send '{"c":"quit"}'
  exec 3>&-
  wait "$BACKEND_PID" 2>/dev/null
  BACKEND_PID=""
}

# Lists the directory, asks row 0 for its media metadata, and leaves the backend running so the
# caller can look for a probe that outlived the answer. Sets ANSWER and ELAPSED.
ask_meta() {
  local dir="$1" started i found
  start_backend
  send "{\"c\":\"list\",\"path\":\"$dir\",\"first\":10}"
  started=$SECONDS
  send '{"c":"meta","row":0,"media":true}'
  ANSWER=NO-ANSWER
  # One grep both decides and answers: a loop that matches the prefix and then greps again for the
  # number reports an empty ANSWER if the line ever reaches the file in two writes.
  for i in $(seq 1 $((ANSWER_BOUND * 10))); do
    if found=$(grep -m1 -o '"w":[0-9]*,"h":[0-9]*,"ms":[0-9]*,"rate":[0-9]*' "$D/out"); then
      ANSWER=$found
      break
    fi
    sleep 0.1
  done
  ELAPSED=$((SECONDS - started))
}

# Plausible metadata on stdout and then the status this case is about. prlimit heads the product's
# own sandbox argv, so shimming it is how a chosen probe result reaches the real code path.
make_stub() {
  cat > "$D/stub/prlimit" <<SH
#!/bin/sh
printf 'codec_type=video\nwidth=1920\nheight=1080\nsample_rate=48000\nduration=10.000000\n'
exit $1
SH
  chmod +x "$D/stub/prlimit"
  STUB_PATH="$D/stub"
}

echo "--- the control: a real clip still measures ---"
ask_meta "$D/clip"
check "a real ffprobe answers the clip's own numbers" '"w":1920,"h":1080,"ms":10000,"rate":0' "$ANSWER"
stop_backend

# A stalled producer, not an empty pipe: the header read that runs before the probe consumes the
# first 8 KiB and returns, and ffprobe then blocks waiting for a moov atom that never arrives. A
# blocked process burns no CPU, so prlimit's own --cpu limit can never end it.
echo "--- a blocked input, which burns no CPU and so has no bound but a wall clock ---"
( exec 4> "$D/blocked/pipe.mp4"; head -c "$FED_BYTES" "$D/clip/clip.mp4" >&4; sleep "$WRITER_SECONDS" ) &
WRITER_PID=$!
ask_meta "$D/blocked"
check "a blocked input is answered at all" "yes" "$([ "$ANSWER" != NO-ANSWER ] && echo yes || echo no)"
check "inside the probe deadline" "yes" "$([ "$ELAPSED" -le "$PROBE_DEADLINE" ] && echo yes || echo no)"
check "and it measured nothing" '"w":0,"h":0,"ms":0,"rate":0' "$ANSWER"
check "no probe outlived the answer" "0" "$(pgrep -c -f -- "$D/blocked/pipe.mp4" 2>/dev/null || true)"
stop_backend
kill "$WRITER_PID" 2>/dev/null; WRITER_PID=""

echo "--- a probe that printed plausible numbers and then exited non-zero ---"
make_stub 1
ask_meta "$D/clip"
check "a non-zero probe is not believed" '"w":0,"h":0,"ms":0,"rate":0' "$ANSWER"
stop_backend

echo "--- the same output from a probe that exited zero, so the case above is about the status ---"
make_stub 0
ask_meta "$D/clip"
check "a zero-exit probe is still parsed" '"w":1920,"h":1080,"ms":10000,"rate":48000' "$ANSWER"
stop_backend
STUB_PATH=""

echo
if [ "$fail" = 0 ]; then echo "media.sh: all checks passed"; else echo "media.sh: FAILURES above"; fi
exit "$fail"
