import argparse
import json
import pathlib
import socket
import subprocess
import sys
import time


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('run_root', type=pathlib.Path)
    args = parser.parse_args()
    root = args.run_root
    if socket.gethostname().split('.')[0] != 'minipc':
        raise SystemExit('cold cache controller may run only on minipc')
    if not root.is_absolute() or not (root / '.findbench-root').is_file():
        raise SystemExit('absolute marked run root required')
    print('cold controller ready: ' + str(root), flush=True)
    unreadable = set()
    while not (root / 'completion.json').exists():
        for request in sorted(root.glob('cache-request-*.json')):
            response = request.with_name(request.name.replace('cache-request-', 'cache-response-', 1))
            if response.exists():
                continue
            try:
                payload = json.loads(request.read_text())
            except (json.JSONDecodeError, UnicodeDecodeError):
                if request not in unreadable:
                    print('cold request unreadable; retrying: ' + str(request), file=sys.stderr, flush=True)
                    unreadable.add(request)
                continue
            if payload.get('action') != 'drop-page-cache' or payload.get('mode') != 'cold':
                raise SystemExit('invalid cold request')
            command = ['sudo', '-n', '/bin/sh', '-c', 'sync; echo 3 > /proc/sys/vm/drop_caches']
            done = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
            # Responses are new files under the marked run root, never overwrites.
            with response.open('x') as out:
                json.dump(dict(exit_code=done.returncode, host='minipc', command=command,
                               stderr=done.stderr, completed_wall_ns=time.time_ns()), out)
            print(f'cache request {request.name}: exit={done.returncode}', flush=True)
            if done.returncode:
                raise SystemExit(done.returncode)
        time.sleep(0.02)


if __name__ == '__main__':
    main()
