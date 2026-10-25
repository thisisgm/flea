"""Controller-only cold helper, derived from the unchanged R2 helper."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time

sys.path.insert(0, str(Path(__file__).resolve().parent / 'fair'))
from eligibility import checked_mutation, marked, read_json


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('run_root', type=Path)
    args = parser.parse_args()
    root = marked(args.run_root)
    host = socket.gethostname()
    unreadable = set()
    while not (root / 'completion.json').exists():
        for request in sorted(root.glob('cache-request-*.json')):
            response = request.with_name(request.name.replace('cache-request-', 'cache-response-', 1))
            if response.is_symlink():
                raise ValueError('cold response symlink')
            if response.exists():
                continue
            try:
                payload = read_json(request, 65536)
            except (json.JSONDecodeError, UnicodeDecodeError):
                if request not in unreadable:
                    print('cold request unreadable; retrying: ' + str(request), file=sys.stderr, flush=True)
                    unreadable.add(request)
                continue
            if (payload.get('action') != 'drop-page-cache' or payload.get('mode') != 'cold'
                    or payload.get('host') != host or not isinstance(payload.get('context'), dict)
                    or request.name != 'cache-request-' + payload['id'] + '.json'):
                raise ValueError('invalid cold request identity')
            age = time.monotonic_ns() - payload['requested_ns']
            if not 0 <= age <= 600_000_000_000:
                raise ValueError('expired cold request')
            request_hash = hashlib.sha256(request.read_bytes()).hexdigest()
            command = ['sudo', '-n', '/bin/sh', '-c', 'sync; echo 3 > /proc/sys/vm/drop_caches']
            done = subprocess.run(command, capture_output=True, text=True, timeout=120)
            if hashlib.sha256(request.read_bytes()).hexdigest() != request_hash:
                raise ValueError('cold request changed while executing')
            receipt = dict(id=payload['id'], host=host, context=payload['context'], request_sha256=request_hash,
                           exit_code=done.returncode, command=command, stderr=done.stderr[:4096],
                           completed_ns=time.monotonic_ns(), completed_wall_ns=time.time_ns())
            staged = root / ('.' + response.name + '.pending')
            with staged.open('x') as out:
                json.dump(receipt, out, sort_keys=True)
            os.link(staged, response)
            checked_mutation(root, staged).unlink()
            print(request.name + ': exit=' + str(done.returncode), flush=True)
            if done.returncode:
                return done.returncode
        time.sleep(.02)
    return 0


if __name__ == '__main__':
    sys.exit(main())
