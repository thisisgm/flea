"""Reject an adapter or instrumentation edit after its recorded build."""
import hashlib
import json
import pathlib
import sys


def hashes(root):
    return {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest()
            for path in sorted(root.rglob('*')) if path.is_file() and '__pycache__' not in path.parts}


if __name__ == '__main__':
    mode, harness, build = sys.argv[1:]
    harness, build = pathlib.Path(harness), pathlib.Path(build)
    manifest = build / 'harness-sha256.json'
    if mode == 'write':
        with manifest.open('x') as stream:
            json.dump(hashes(harness), stream, indent=2)
    elif mode == 'check':
        if hashes(harness) != json.loads(manifest.read_text()):
            raise SystemExit('harness differs from completed build; rebuild before running')
    else:
        raise SystemExit('mode must be write or check')
