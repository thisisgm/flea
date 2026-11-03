#!/usr/bin/env bash
# Converts a real office fixture through the release backend and its production sandbox.
set -euo pipefail
repo=$(cd "$(dirname "$0")/.." && pwd -P)
binary=${FLEA_BIN:-$repo/target/release/flea}
if ! command -v soffice >/dev/null 2>&1; then
    printf 'ok   print-office-real: soffice not installed, real conversion needs minipc\n'
    exit 0
fi
[ -x "$binary" ] || { printf 'FAIL print-office-real: no release binary at %s\n' "$binary"; exit 1; }
parent="$repo/.superpowers/print-office-real"
mkdir -p "$parent"
box=$(mktemp -d "$parent/suite.XXXXXXXX")
[[ -n "$box" && "$box" == /* ]] || { printf 'FAIL print-office-real: bad sandbox root %s\n' "$box"; exit 1; }
: > "$box/.flea-test-sandbox"
cleanup() {
    local status=$?
    if [ "$status" -ne 0 ]; then
        printf 'print-office-real: evidence=%s\n' "$box"
        return
    fi
    case $box in
        /*) [ -n "$box" ] && [ "${box%/*}" = "$parent" ] && [ -f "$box/.flea-test-sandbox" ] && rm -rf -- "$box" ;;
    esac
}
trap cleanup EXIT
python3 -B - "$binary" "$box" <<'PY'
import json
import os
from pathlib import Path
import selectors
import signal
import subprocess
import sys
import time

binary = str(Path(sys.argv[1]).resolve(strict=True))
root = Path(sys.argv[2])
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
env = {'PATH': os.environ['PATH'], 'LC_ALL': 'C.UTF-8', 'LANG': 'C.UTF-8'}
for key, leaf in [('HOME', 'home'), ('XDG_CONFIG_HOME', 'config'), ('XDG_CACHE_HOME', 'cache'),
                  ('XDG_STATE_HOME', 'state'), ('XDG_DATA_HOME', 'data'), ('XDG_RUNTIME_DIR', 'runtime')]:
    folder = root / leaf
    folder.mkdir(mode=0o700)
    env[key] = str(folder)
source = root / 'letter.rtf'
fixture = b'{\\rtf1\\ansi Print and convert fixture.}'
source.write_bytes(fixture)
output = root / 'letter (converted).pdf'
request = {'c': 'convertbatch', 'requestId': 41, 'path': str(source), 'format': 'pdf'}
replies = []
try:
    with (root / 'backend.stderr').open('wb') as errors:
        child = subprocess.Popen([binary, '--backend'], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                 stderr=errors, env=env, start_new_session=True)
        try:
            child.stdin.write((json.dumps(request) + '\n').encode())
            child.stdin.flush()
            deadline = time.monotonic() + 75
            pending = b''
            done = None
            with selectors.DefaultSelector() as selector:
                selector.register(child.stdout, selectors.EVENT_READ)
                while done is None:
                    remaining = deadline - time.monotonic()
                    assert remaining > 0, f'backend timed out: {replies}'
                    assert selector.select(remaining), f'backend timed out: {replies}'
                    chunk = os.read(child.stdout.fileno(), 65536)
                    assert chunk, f'backend ended before convertdone: {replies}'
                    pending += chunk
                    while b'\n' in pending:
                        line, pending = pending.split(b'\n', 1)
                        reply = json.loads(line)
                        replies.append(reply)
                        if reply.get('t') == 'convertdone' and reply.get('requestId') == 41:
                            done = reply
            assert done.get('ok') is True, f'office conversion refused: {done}'
            assert done.get('path') == str(output) and done.get('paths') == [str(output)], done
            assert output.is_file() and not output.is_symlink(), 'converter produced no regular PDF'
            assert output.read_bytes().startswith(b'%PDF'), 'converter output is not a PDF'
            assert source.read_bytes() == fixture, 'conversion changed its source'
            child.communicate(b'{"c":"quit"}\n', timeout=10)
            assert child.returncode == 0, f'backend exit {child.returncode}'
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGKILL)
            child.wait()
except Exception as error:
    print(f'FAIL print-office-real: {error}', file=sys.stderr)
    raise SystemExit(1)
finally:
    (root / 'replies.json').write_text(json.dumps(replies, indent=2) + '\n')
print('ok   print-office-real: real office conversion produced a PDF')
PY
# The ignored unit test drives print's office leg through the office service; --exact and the count stop a renamed test passing as zero run.
test=backend::printreq::tests::real::print_real_office_leg_converts_through_the_office_service
log="$box/office-leg-test.log"
if ! (cd "$repo" && cargo test --locked --bin flea -- --ignored --exact "$test") > "$log" 2>&1 || ! grep -q '^test result: ok. 1 passed' "$log"; then
    printf 'FAIL print-office-real: %s did not pass, see %s\n' "$test" "$log"
    exit 1
fi
printf 'ok   print-office-real: print office leg unit test passed\n'
