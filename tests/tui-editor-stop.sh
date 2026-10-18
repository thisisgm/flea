#!/usr/bin/env bash
# A SIGTERM to the TUI's job while the bulk rename editor's return runs its stty ends the TUI with status 0 and the terminal restored.
set -u
# Hard rule 9's guard, which owns FIXTURE_ROOT and every create and delete below.
. "$(dirname "$0")/../tools/flea-sandbox-guard"
cd "$(dirname "$0")/.." || exit 1
bin=${FLEA_BIN:-$PWD/target/debug/flea}
[[ -x "$bin" ]] || { echo 'FAIL tui-editor-stop needs the candidate binary'; exit 1; }

scratch="$FIXTURE_ROOT/flea-tui-editor-stop-$$"
sandbox_make "$scratch" || exit 1
cleanup() { sandbox_remove "$scratch"; }
trap cleanup EXIT
mkdir -p "$scratch"/{home,state,config,cache,runtime,fixture,bin} && chmod 700 "$scratch/runtime" || exit 1
for name in stop-alpha.txt stop-beta.txt; do printf 'tui editor stop fixture\n' > "$scratch/fixture/$name"; done
mkfifo "$scratch/stty-gate" || exit 1
# An editor that keeps every name, so the rename would have nothing to do.
printf '#!/bin/sh\nexit 0\n' > "$scratch/bin/editor"
# The second raw call is the editor's return; it says so on the TUI's screen, then waits at a FIFO no one opens until SIGTERM ends it.
cat > "$scratch/bin/stty" <<SH
#!/bin/sh
if [ "\$1" = raw ]; then
    if [ -f "$scratch/prepared" ] && [ ! -f "$scratch/held" ]; then
        : > "$scratch/held"
        printf 'FLEA_EDITOR_RETURN_HELD\n'
        exec /bin/cat "$scratch/stty-gate"
    fi
    : > "$scratch/prepared"
fi
exec /usr/bin/stty "\$@"
SH
chmod +x "$scratch/bin/editor" "$scratch/bin/stty" || exit 1

fail=0
check() {
  local label="$1" expected="$2" actual="$3"
  if [ "$expected" != "$actual" ]; then
    echo "FAIL $label"; echo "  expected: $expected"; echo "  actual:   $actual"; fail=1
  else
    echo "ok   $label"
  fi
}

# Sample output: "held=1 exit=0 fault=0 restored=1"; held=0 is a run that never reached the editor's return.
result=$(env -i PATH="$scratch/bin:/usr/bin:/bin" HOME="$scratch/home" XDG_STATE_HOME="$scratch/state" XDG_CONFIG_HOME="$scratch/config" \
  XDG_CACHE_HOME="$scratch/cache" XDG_RUNTIME_DIR="$scratch/runtime" TERM=xterm-256color VISUAL="$scratch/bin/editor" \
  /usr/bin/python3 - "$bin" "$scratch/fixture" <<'PY'
import fcntl, os, select, signal, struct, sys, termios, time
binary, fixture = sys.argv[1], sys.argv[2]
HELD = b"FLEA_EDITOR_RETURN_HELD"
master, slave = os.openpty()
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 160, 1600, 800))
saved = termios.tcgetattr(slave)
pid = os.fork()
if pid == 0:
    # The PTY is the TUI's controlling terminal, as a shell's is, so the editor handoff can move its foreground.
    os.setsid()
    fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
    for fd in (0, 1, 2):
        os.dup2(slave, fd)
    os.close(master)
    os.close(slave)
    os.execv(binary, [binary, "--tui", fixture])
exited = os.pidfd_open(pid)
seen, answered = b"", False

def until(marker, seconds):
    """Reads until marker is in the output, answering the device attributes query once; the bound only ends a hung TUI."""
    global seen, answered
    end = time.monotonic() + seconds
    while marker not in seen:
        left = end - time.monotonic()
        if left <= 0:
            return False
        ready, _, _ = select.select([master, exited], [], [], left)
        if master in ready:
            try:
                seen += os.read(master, 65536)
            except OSError:
                return False
            if not answered and b"\x1b[c" in seen:
                answered = True
                os.write(master, b"\x1b[?62;4c")
        elif exited in ready:
            return False
    return True

held = until(b"stop-beta", 10)
if held:
    # Ctrl+A selects both rows and r hands them to the editor as one bulk rename.
    os.write(master, b"\x01r")
    held = until(HELD, 10)
if held:
    os.killpg(pid, signal.SIGTERM)
# The TUI's exit is the pidfd turning readable; the PTY is drained meanwhile so no write of the TUI's can block it.
end = time.monotonic() + 10
while time.monotonic() < end:
    ready, _, _ = select.select([master, exited], [], [], end - time.monotonic())
    if exited in ready:
        break
    if master in ready:
        try:
            seen += os.read(master, 65536)
        except OSError:
            break
else:
    os.kill(pid, signal.SIGKILL)
_, status = os.waitpid(pid, 0)
while select.select([master], [], [], 0)[0]:
    try:
        chunk = os.read(master, 65536)
    except OSError:
        break
    if not chunk:
        break
    seen += chunk
code = str(os.WEXITSTATUS(status)) if os.WIFEXITED(status) else "signal%d" % os.WTERMSIG(status)
fault = int(b"terminal interface" in seen or b"The terminal could not" in seen)
restored = int(termios.tcgetattr(slave) == saved)
print("held=%d exit=%s fault=%d restored=%d" % (int(held), code, fault, restored))
if fault:
    start = max(seen.rfind(b"flea:"), 0)
    print("output: %r" % seen[start:start + 200])
PY
)
echo "$result"
# Sample input: "held=1 exit=0 fault=0 restored=1"; field held prints 1.
field() { printf '%s\n' "$result" | sed -n "s/.*$1=\([a-z0-9]*\).*/\1/p" | head -1; }
check "the bulk rename reached the editor's return stty" 1 "$(field held)"
check "the TUI left on SIGTERM with status 0" 0 "$(field exit)"
check "the TUI printed no terminal fault" 0 "$(field fault)"
check "the TUI restored the terminal mode" 1 "$(field restored)"

[ "$fail" = 0 ] && echo "tui-editor-stop: all passed" || echo "tui-editor-stop: failures above"
exit "$fail"
