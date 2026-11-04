#!/usr/bin/env bash
# Leaving the TUI while mpv is still drawing writes none of mpv's bytes after the terminal is restored.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL tui-quit-playing needs the candidate binary'; exit 1; }

scratch="$FIXTURE_ROOT/flea-tui-quit-playing-$$"
sandbox_make "$scratch" || exit 1
cleanup() { sandbox_remove "$scratch"; }
trap cleanup EXIT
mkdir -p "$scratch"/{home,state,config,cache,runtime,fixture,bin} && chmod 700 "$scratch/runtime" || exit 1
printf 'not a movie\n' > "$scratch/fixture/clip.mp4"
# A stand-in mpv that draws a kitty frame as fast as its pipe takes them, so its pipe is full when the TUI leaves.
cat > "$scratch/bin/mpv" <<'SH'
#!/bin/sh
frame=$(printf '\033[3;81f\033_Ga=T,f=24,s=1,v=1,C=1,q=2,m=0;AAAA\033\\')
while :; do printf '%s' "$frame"; done
SH
chmod +x "$scratch/bin/mpv"

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

# Sample output: "mpv_seen=1 late=0 exit=0"; mpv_seen=0 is a run that never reached the player.
result=$(env -i PATH="$scratch/bin:/usr/bin:/bin" HOME="$scratch/home" XDG_STATE_HOME="$scratch/state" XDG_CONFIG_HOME="$scratch/config" \
  XDG_CACHE_HOME="$scratch/cache" XDG_RUNTIME_DIR="$scratch/runtime" TERM=xterm-256color \
  /usr/bin/python3 - "$bin" "$scratch/fixture" <<'PY'
import os, pty, re, select, signal, struct, sys, time, fcntl, termios
binary, fixture = sys.argv[1], sys.argv[2]
# The stand-in's kitty payload, which no part of the TUI draws, marks one relayed mpv frame.
FRAME, LEAVE = b"a=T,f=24,s=1,v=1,C=1,q=2,m=0;AAAA", b"\x1b[?1049l"
pid, fd = pty.fork()
if pid == 0:
    os.execv(binary, [binary, "--tui", fixture])
fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 1600, 800))
seen, replied, probe_id = b"", False, None

def pump(seconds):
    """Reads for up to seconds, answering the kitty probe once the TUI asks it; False once the TUI closed its end."""
    global seen, replied, probe_id
    end = time.monotonic() + seconds
    while time.monotonic() < end:
        ready, _, _ = select.select([fd], [], [], 0.02)
        if not ready:
            continue
        try:
            chunk = os.read(fd, 65536)
        except OSError:
            return False
        seen += chunk
        if not replied and b"\x1b[c" in seen:
            query = re.search(rb"\x1b_Gi=([0-9]+),s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\", seen)
            if query is not None:
                replied, probe_id = True, query.group(1)
                os.write(fd, b"\x1b_Gi=" + probe_id + b";OK\x1b\\\x1b[?62;4c")
    return True

deadline = time.monotonic() + 8
while time.monotonic() < deadline and seen.count(FRAME) < 3:
    pump(0.1)
mpv_seen = seen.count(FRAME)
# Unread, the pty fills, the relay blocks in its write and mpv's pipe fills behind it.
time.sleep(1.0)
os.kill(pid, signal.SIGTERM)
# A clean exit closes the pty well inside this bound; past it the TUI counts as hung and the kill below ends it.
drain_end = time.monotonic() + 5
while time.monotonic() < drain_end and pump(0.5):
    pass
code, deadline = "killed", time.monotonic() + 3
while time.monotonic() < deadline:
    done, status = os.waitpid(pid, os.WNOHANG)
    if done:
        code = str(os.WEXITSTATUS(status)) if os.WIFEXITED(status) else "signal%d" % os.WTERMSIG(status)
        break
    time.sleep(0.02)
else:
    os.kill(pid, signal.SIGKILL)
    os.waitpid(pid, 0)
left = seen.rfind(LEAVE)
late = seen[left + len(LEAVE):].count(FRAME) if left >= 0 else -1
print("probe_id=%s mpv_seen=%d late=%d exit=%s" % (probe_id, mpv_seen, late, code))
PY
)
echo "$result"
field() { printf '%s\n' "$result" | sed -n "s/.*$1=\([-a-z0-9]*\).*/\1/p"; }
check "the TUI reached its player and relayed mpv's frames" yes "$([ "$(field mpv_seen)" -ge 3 ] 2>/dev/null && echo yes || echo no)"
check "the TUI restored the terminal and left a screen mpv never drew on" 0 "$(field late)"
check "the TUI left on SIGTERM with status 0" 0 "$(field exit)"

[ "$fail" = 0 ] && echo "tui-quit-playing: all passed" || echo "tui-quit-playing: failures above"
exit "$fail"
