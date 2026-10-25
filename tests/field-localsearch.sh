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
# A PID namespace of its own, so a host indexer can neither fail the suite nor be seen by its pgrep.
if ! why=$(unshare --user --pid --fork --mount-proc true 2>&1); then
  echo "FAIL field-localsearch: unshare --user --pid --fork --mount-proc is unavailable on this host: ${why:-no message}"
  exit 1
fi
unshare --user --pid --fork --mount-proc python3 -B - "$scratch" "${FIELD_LOCALSEARCH_TOOL:-tools/flea-field-search}" <<'PY'
import json
import os
import signal
import subprocess
import sys
import time
from importlib.machinery import SourceFileLoader
from pathlib import Path
from unittest.mock import patch

root, tool = Path(sys.argv[1]), str(Path(sys.argv[2]).resolve())
bindir = root / 'bin'
bindir.mkdir()
for name in ('localsearch-3', 'localsearch-ext'):
    (bindir / name).write_bytes(Path('/usr/bin/sleep').read_bytes())
    (bindir / name).chmod(0o755)

# The driver loads the real tool, activates both indexers outside its own tree as dbus would, then fails or waits as asked.
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
assert rc == 143 and len(pids) == 2, (rc, err, pids)
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

# 5. A run killed outright ends where the next recorded run begins, so it never claims what a later run started.
stale = root / 'ledger-stale.jsonl'
now = time.time()
stale.write_text('{"id": "old", "pid": 4242, "start": %f}\n{"id": "next", "pid": 4343, "start": %f, "end": %f}\n'
                 % (now - 2 * 86400, now - 86400, now - 86400 + 60))
stray = subprocess.Popen([str(bindir / 'localsearch-3'), '300'], start_new_session=True, stdin=subprocess.DEVNULL,
                         stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
time.sleep(0.3)
rc, err, _ = run('refuse-only', stale)
stray.kill()
stray.wait()
assert rc != 0 and 'this harness did not start it' in err and 'this harness started it' not in err, err

# 6. Attribution follows the windows, never the calendar: the covering run is named wherever it sits in the ledger.
S = SourceFileLoader('search_windows', tool).load_module()
def attribute(lines, began):
    S.LEDGER = str(root / 'ledger-unit.jsonl')
    Path(S.LEDGER).write_text(''.join(json.dumps(line) + '\n' for line in lines))
    with patch.object(S, 'process_start', return_value=began):
        return S.describe_leftover('localsearch-3', 99)
midnight = time.mktime(time.strptime(time.strftime('%Y-%m-%d'), '%Y-%m-%d'))
# A killed old run, then an ended later run covering the pid: the later run started it.
said = attribute([{'id': 'old', 'pid': 1, 'start': midnight - 86400},
                  {'id': 'new', 'pid': 2, 'start': midnight - 3600},
                  {'id': 'new', 'end': midnight - 1800}], midnight - 3000)
assert 'run pid 2 ' in said and 'this harness started it' in said, said
# The killed old run covers only up to the later run's start.
said = attribute([{'id': 'old', 'pid': 1, 'start': midnight - 86400},
                  {'id': 'new', 'pid': 2, 'start': midnight - 3600, 'end': midnight - 1800}], midnight - 7200)
assert 'run pid 1 ' in said and 'never ended' in said, said
said = attribute([{'id': 'old', 'pid': 1, 'start': midnight - 86400},
                  {'id': 'new', 'pid': 2, 'start': midnight - 3600, 'end': midnight - 1800}], midnight - 60)
assert 'this harness did not start it' in said, said
# A run begun 23:50 whose indexer started 23:55 stays attributed after midnight, open or ended.
for run in ({'id': 'late', 'pid': 3, 'start': midnight - 600},
            {'id': 'late', 'pid': 3, 'start': midnight - 600, 'end': midnight - 60}):
    said = attribute([run], midnight - 300)
    assert 'run pid 3 ' in said and 'this harness started it' in said, said
print('field-localsearch: all passed')
PY
