"""Exit an authenticated app-created window while the real CLI enumerates its records."""
import json
from pathlib import Path
import subprocess
import sys
import time

from flea_transport import REPO, load


def vanishing_peer(first, second, environment, check):
    child = int(first.call("call", "flea", "spawn").stdout)
    socket = Path(load(first.root)["runtime_dir"]) / "flea-ipc" / f"{child}.sock"
    deadline = time.monotonic() + 2
    while not socket.exists() and time.monotonic() < deadline:
        time.sleep(0.01)
    assert socket.exists(), "fixture child never served its endpoint"
    marker = first.root / "peer-exit-injected"
    probe = '''
import os
from pathlib import Path
import signal
import sys
import time
import flea_transport_cli as cli
from flea_transport import MARKER, owned
from flea_transport_peers import PeerWindow
from flea_transport_lifecycle import identity, signal_if_matches

original = PeerWindow.record
calls = 0

def record(self):
    global calls
    result = original(self)
    if result['pid'] == int(os.environ['FLEA_CONTROL_PEER_PID']):
        calls += 1
        if calls == 2:
            assert signal_if_matches(result['pid'], result['start'], signal.SIGTERM,
                                     group=result['group'], verify=lambda pid: owned(pid, result))
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline:
                try:
                    if identity(result['pid'])['state'] == 'Z':
                        break
                except FileNotFoundError:
                    break
                time.sleep(.01)
            marker = Path(os.environ['FLEA_CONTROL_PEER_MARKER'])
            assert marker.is_absolute() and (marker.parent / MARKER).is_file()
            marker.touch()
            return original(self)
    return result

PeerWindow.record = record
sys.exit(cli.main())
'''
    env = dict(environment, PYTHONPATH=str(REPO / "tests/lib"), FLEA_CONTROL_PEER_PID=str(child),
               FLEA_CONTROL_PEER_MARKER=str(marker))
    result = subprocess.run([sys.executable, "-B", "-c", probe, "list", "--", "--all", "--json"],
                            env=env, capture_output=True, timeout=5)
    check(marker.is_file(), "peer-exit-after-discovery-injected")
    assert result.returncode == 0, result.stderr.decode()
    check({row["pid"] for row in json.loads(result.stdout)} == {first.pid, second.pid},
          "peer-exit-preserves-surviving-window-list")
