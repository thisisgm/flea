#!/usr/bin/env python3
"""Owned helper fixtures only; never calls rclone or touches a user's mount."""
import json
import os
from pathlib import Path
import signal
import sys
import time

root = Path(os.environ['FLEA_CLOUD_FIXTURE'])
assert (root / '.flea-test-sandbox').is_file()
scenario = os.environ['FLEA_CLOUD_CASE']
state = root / 'calls'
count = int(state.read_text()) + 1 if state.exists() else 1
state.write_text(str(count))
(root / 'helper.pid').write_text(str(os.getpid()))
if scenario == 'hang':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    time.sleep(30)
if scenario == 'stale':
    time.sleep(.4)
if scenario == 'recover' and count == 1:
    sys.exit(1)
print(json.dumps({'path': sys.argv[2], 'mount': '/A',
                  'state': 'pending' if scenario == 'stale' else 'idle', 'queued': count}))
