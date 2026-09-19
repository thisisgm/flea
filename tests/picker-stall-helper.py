#!/usr/bin/env python3
"""A sequential backend blocked in a real FIFO read, with no user mount or cloud traffic."""
import json
import os
from pathlib import Path
import signal
import sys

root = Path(os.environ['FLEA_PICKER_FIXTURE'])
assert (root / '.flea-test-sandbox').is_file()
with (root / 'pids').open('a') as log:
    log.write(f'{os.getpid()}\n')
signal.signal(signal.SIGTERM, signal.SIG_IGN)
for line in sys.stdin:
    request = json.loads(line)
    with (root / 'requests').open('a') as log:
        log.write(line)
    command = request['c']
    if command == 'list' and request['path'] == '/stalled':
        print('{"t":"blocked"}', flush=True)
        # Leave an incomplete reply behind: a reused stdout parser would join it to the next worker.
        print('{"t":"listed","path":"/stale",', end='', flush=True)
        with (root / 'stall.fifo').open() as pipe:
            pipe.read()
    elif command == 'picker':
        print('{"t":"picker","id":1,"op":"blocked"}', flush=True)
        with (root / 'stall.fifo').open() as pipe:
            pipe.read()
    elif command in ('list', 'listpaths'):
        print(json.dumps({'t': 'listed', 'path': request.get('path', '/'), 'n': 1, 'read': 0, 'sort': 0}), flush=True)
        print('{"t":"rows","start":0,"rows":[{"n":"local.txt","d":false,"s":1,"m":1,"p":33188,"i":"text-x-generic","t":false,"k":0}],"kinds":[]}', flush=True)
    elif command == 'window':
        print('{"t":"rows","start":1,"rows":[{"n":"next.txt"}],"kinds":[]}', flush=True)
    elif command == 'quit':
        break
