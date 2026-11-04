"""Keep Xvfb identity pinned until the native transport body has finished."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from flea_transport import private
from flea_transport_lifecycle import identity, signal_if_matches


def main():
    root = private(sys.argv[1])
    with (root / 'display').open('wb') as display, (root / 'xvfb.log').open('wb') as log:
        server = subprocess.Popen(['Xvfb', '-displayfd', '1', '-screen', '0', '1024x768x24', '-nolisten', 'tcp'],
                                  stdout=display, stderr=log, start_new_session=True)
    start = identity(server.pid)['start']
    (root / 'server.json').write_text(json.dumps(dict(pid=server.pid, start=start, group=server.pid)))
    try:
        deadline = time.monotonic() + 5
        while not (root / 'display').stat().st_size:
            if identity(server.pid)['state'] == 'Z':
                raise RuntimeError('Xvfb exited before publishing its display')
            if time.monotonic() >= deadline:
                raise RuntimeError('Xvfb did not publish a display')
            time.sleep(0.02)
        number = (root / 'display').read_text().strip()
        if not number.isdecimal():
            raise RuntimeError('Xvfb published an invalid display number')
        environment = dict(os.environ, DISPLAY=':' + number)
        return subprocess.run(sys.argv[2:], env=environment).returncode
    finally:
        signal_if_matches(server.pid, start, signal.SIGTERM, group=server.pid)
        try:
            server.wait(timeout=3)
        except subprocess.TimeoutExpired:
            signal_if_matches(server.pid, start, signal.SIGKILL, group=server.pid)
            server.wait(timeout=3)


if __name__ == '__main__':
    sys.exit(main())
