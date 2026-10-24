#!/bin/bash
# Checks that a field run ends an indexer an entrant activated on every exit path, and that a refusal names it.
set -eu
cd "$(dirname "$0")/.."
# The fixture root is a mktemp -d of its own, and its one delete is checked absolute and two deep before the trap exists.
scratch=$(mktemp -d)
case $scratch in
  /*/*) ;;
  *) echo "FAIL: mktemp -d gave '$scratch', which is not an absolute path two components deep" >&2; exit 1 ;;
esac
: > "$scratch/.flea-test-sandbox"
trap 'pkill -9 -f -- "$scratch/bin/" 2>/dev/null || true; [ -f "$scratch/.flea-test-sandbox" ] && rm -rf "$scratch"' EXIT
if pgrep -x 'localsearch-3|localsearch-ext' > /dev/null; then
  echo "FAIL: a real localsearch process runs on this host; stop it before this suite, which never ends one it did not start" >&2
  exit 1
fi
python3 -B - "$scratch" "${FIELD_LOCALSEARCH_TOOL:-tools/flea-field-search}" <<'PY'
import os
import signal
import subprocess
import sys
import time
from pathlib import Path

root, tool = Path(sys.argv[1]), str(Path(sys.argv[2]).resolve())
bindir = root / 'bin'
bindir.mkdir()
for name in ('localsearch-3', 'localsearch-ext'):
    (bindir / name).write_bytes(Path('/usr/bin/sleep').read_bytes())
    (bindir / name).chmod(0o755)

# The driver loads the real tool, passes require_idle, lets a pretend entrant activate both indexers
# outside its own process tree (dbus activation does that), then fails or waits as asked.
driver = root / 'driver.py'
driver.write_text('''
import os, subprocess, sys, time
from importlib.machinery import SourceFileLoader
S = SourceFileLoader('search_under_test', sys.argv[1]).load_module()
S.LEDGER = sys.argv[2]
S.KILL_TARGETS = [("localsearch-3", ""), ("localsearch-ext", "")]
mode, bindir, pidfile = sys.argv[3:6]
S.require_idle()
if mode == "refuse-only":
    sys.exit(0)
pids = []
for name in ("localsearch-3", "localsearch-ext"):
    pids.append(subprocess.Popen([os.path.join(bindir, name), "300"], start_new_session=True,
                                 stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).pid)
open(pidfile, "w").write(" ".join(map(str, pids)))
if mode == "fail":
    raise RuntimeError("identity command failed: ['/usr/bin/pcmanfm', '--version']: ")
if mode == "wait":
    time.sleep(120)
if mode == "kill":
    os.kill(os.getpid(), 9)
''')

def alive(pid):
    try:
        os.kill(pid, 0)
        return Path('/proc/%d/stat' % pid).read_text().rsplit(')', 1)[1].split()[0] != 'Z'
    except OSError:
        return False

def run(mode, ledger, wait_signal=None):
    pidfile = root / (mode + '.pids')
    pidfile.unlink(missing_ok=True)
    proc = subprocess.Popen(['python3', '-B', str(driver), tool, str(ledger), mode, str(bindir), str(pidfile)],
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    if wait_signal:
        for _ in range(100):
            if pidfile.exists() and pidfile.read_text().count(' '):
                break
            time.sleep(0.1)
        proc.send_signal(wait_signal)
    out, err = proc.communicate(timeout=60)
    pids = [int(p) for p in pidfile.read_text().split()] if pidfile.exists() else []
    return proc.returncode, err, pids

def settle(pids, seconds=15):
    end = time.time() + seconds
    while time.time() < end and any(alive(p) for p in pids):
        time.sleep(0.1)
    return [p for p in pids if alive(p)]

def reap(pids):
    for p in pids:
        try:
            os.kill(p, 9)
        except OSError:
            pass

# 1. A step fails after an entrant activated the indexers: both must be ended.
rc, err, pids = run('fail', root / 'ledger-fail.jsonl')
left = settle(pids)
reap(left)
assert rc != 0 and len(pids) == 2, (rc, err)
assert not left, 'a failing step left the activated indexers running: %r' % left
ledger = (root / 'ledger-fail.jsonl').read_text()
assert '"start"' in ledger and '"end"' in ledger, ledger

# 2. The run is terminated mid-measure (a systemd stop): same guarantee.
rc, err, pids = run('wait', root / 'ledger-term.jsonl', signal.SIGTERM)
left = settle(pids)
reap(left)
assert not left, 'SIGTERM left the activated indexers running: %r' % left

# 3. A run that is killed outright leaves them, and the next refusal says this harness started them.
ledger = root / 'ledger-kill.jsonl'
rc, err, pids = run('kill', ledger)
assert rc == -9 and all(alive(p) for p in pids), (rc, pids)
rc, err, _ = run('refuse-only', ledger)
reap(pids)
settle(pids)
assert rc != 0 and 'already running' in err and 'refusing to start' in err, err
assert 'localsearch-3 pid %d started 20' % pids[0] in err, err
assert 'this harness started it' in err and 'never ended' in err, err

# 4. An indexer no recorded run covers is named with its start time and is not claimed.
stray = subprocess.Popen([str(bindir / 'localsearch-3'), '300'], start_new_session=True, stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.3)
rc, err, _ = run('refuse-only', root / 'ledger-empty.jsonl')
alive_after = alive(stray.pid)
stray.kill()
stray.wait()
assert rc != 0 and 'localsearch-3 pid %d started 20' % stray.pid in err, err
assert 'this harness did not start it' in err, err
assert alive_after, 'a refusal must leave the foreign indexer alone'
print('field-localsearch: all passed')
PY
