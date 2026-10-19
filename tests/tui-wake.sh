#!/usr/bin/env bash
# With the TUI's input wait set to 5 s, the rows still arrive at once and a lone Escape is still held for the whole wait.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL tui-wake needs the candidate binary'; exit 1; }

# The input wait the Escape timing below is judged against; one name, so the wait and its floor cannot drift.
input_wait_ms=5000
scratch="$FIXTURE_ROOT/flea-tui-wake-$$"
sandbox_make "$scratch" || exit 1
cleanup() { sandbox_remove "$scratch"; }
trap cleanup EXIT
mkdir -p "$scratch"/{home,state,config,cache,runtime,fixture} && chmod 700 "$scratch/runtime" || exit 1
for name in wake-alpha.txt wake-beta.txt wake-gamma.txt; do printf 'tui wake fixture\n' > "$scratch/fixture/$name"; done

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

# Sample output: "rows_ms=4 sheet_ms=3 escape_ms=5012 closed=yes exit=0"; -1 is a step that never drew, "killed" a TUI that ignored SIGTERM.
result=$(env -i PATH=/usr/bin:/bin HOME="$scratch/home" XDG_STATE_HOME="$scratch/state" XDG_CONFIG_HOME="$scratch/config" \
  XDG_CACHE_HOME="$scratch/cache" XDG_RUNTIME_DIR="$scratch/runtime" TERM=xterm-256color FLEA_TUI_INPUT_TIMEOUT_MS=$input_wait_ms \
  /usr/bin/python3 - "$bin" "$scratch/fixture" "$PWD/i18n/en/flea.json" <<'PY'
import json, os, pty, select, signal, struct, sys, time, fcntl, termios
binary, fixture = sys.argv[1], sys.argv[2]
# The keys sheet's card title, bold, from the English catalog; the TUI writes whole frames and only when one changes, so a frame without it has no sheet.
with open(sys.argv[3], encoding="utf-8") as catalog:
    SHEET = ("\x1b[1m" + json.load(catalog)["keys.title"]).encode()
start = time.monotonic()
pid, fd = pty.fork()
if pid == 0:
    os.execv(binary, [binary, "--tui", fixture])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))

def read_for(seconds, until=None):
    """Reads output for up to seconds; returns (bytes, ms of the first chunk or of the first chunk holding until)."""
    end, seen, hit = time.monotonic() + seconds, b"", -1
    while time.monotonic() < end:
        ready, _, _ = select.select([fd], [], [], 0.02)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            break
        seen += chunk
        if hit < 0 and (until is None or until in seen):
            hit = int((time.monotonic() - mark) * 1000)
            if until is not None:
                break
    return seen, hit

def quiet(seconds):
    """Waits until the TUI has written nothing for seconds, so the next output belongs to the next step."""
    while True:
        ready, _, _ = select.select([fd], [], [], seconds)
        if not ready:
            return
        try:
            os.read(fd, 65536)
        except OSError:
            return  # EIO: the TUI has closed its end of the terminal

mark = start
_, rows_ms = read_for(4.0, b"wake-alpha")
quiet(1.0)
mark = time.monotonic(); os.write(fd, b"?")
_, sheet_ms = read_for(2.0, SHEET)
quiet(0.5)
# The lone Escape: the sheet closes only once the input wait has passed with no byte behind it.
mark = time.monotonic(); os.write(fd, b"\x1b")
after, escape_ms = read_for(8.0)
closed = "yes" if SHEET not in after and b"wake-alpha" in after else "no"
os.kill(pid, signal.SIGTERM)
code, deadline = "killed", time.monotonic() + 3
while time.monotonic() < deadline:
    done, status = os.waitpid(pid, os.WNOHANG)
    if done:
        code = str(os.WEXITSTATUS(status)) if os.WIFEXITED(status) else "signal%d" % os.WTERMSIG(status)
        break
    quiet(0.02)
else:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
print("rows_ms=%d sheet_ms=%d escape_ms=%d closed=%s exit=%s" % (rows_ms, sheet_ms, escape_ms, closed, code))
PY
)
echo "$result"
field() { printf '%s\n' "$result" | sed -n "s/.*$1=\([-a-z0-9]*\).*/\1/p"; }
rows=$(field rows_ms); sheet=$(field sheet_ms); escape=$(field escape_ms)
# Half the input wait, so only a wake from the backend explains rows or a sheet this early; the margin absorbs a loaded box.
early_ms=$((input_wait_ms / 2))
# The Escape floor is the input wait less 500 ms of scheduling slack; the ceiling covers a loaded box.
escape_floor_ms=$((input_wait_ms - 500)) escape_ceiling_ms=8000
check "the first listing reached the screen without waiting for the input timeout" yes "$([ "${rows:--1}" -ge 0 ] && [ "$rows" -lt "$early_ms" ] && echo yes || echo no)"
check "the keys sheet is drawn at once" yes "$([ "${sheet:--1}" -ge 0 ] && [ "$sheet" -lt "$early_ms" ] && echo yes || echo no)"
# Before the fix the terminal's own 100 ms timer decided, so the sheet closed at once whatever the wait was set to.
check "a lone Escape is held for the input wait, then acted on" yes "$([ "${escape:--1}" -ge "$escape_floor_ms" ] && [ "$escape" -lt "$escape_ceiling_ms" ] && echo yes || echo no)"
check "the frames after the Escape list the fixture and draw no sheet" yes "$(field closed)"
check "the TUI left on SIGTERM with status 0" 0 "$(field exit)"

# The wiring the timing rests on: raw mode never blocks a read, and every backend line rings the loop's wake socket.
check "raw mode reads without a timer" 1 "$(grep -c '"raw", "-echo", "min", "0", "time", "0"' src/tui/terminal.rs)"
check "the reader thread rings once per event" 2 "$(grep -c 'let _ = (&ring).write(&\[1\]);' src/tui/wire.rs)"

[ "$fail" = 0 ] && echo "tui-wake: all passed" || echo "tui-wake: failures above"
exit "$fail"
