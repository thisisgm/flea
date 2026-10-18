#!/usr/bin/python3
"""Replay rclone 1.75 JSON stats and check logs without a network remote."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

root = Path(os.environ['RCLONE_CONFIG']).parent
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
control = json.loads((root / 'control.json').read_text())
args = sys.argv[1:]
with (root / 'argv.jsonl').open('a') as log:
    log.write(json.dumps(args) + '\n')
if control.get('real'):
    if args[0] == 'mkdir':
        destination = args[args.index('--') + 1]
        target_root = Path(destination.split(':', 1)[1])
        assert str(target_root) and target_root.is_absolute() and target_root.resolve().is_relative_to(root)
    if args[0] in ('copy', 'copyto', 'check'):
        source, destination = args[args.index('--') + 1:]
        source_path = Path(source).resolve(strict=True)
        target_root = Path(destination.split(':', 1)[1]).resolve()
        assert Path(destination.split(':', 1)[1]).is_absolute(), 'real remote destination must be absolute'
        assert source_path.is_relative_to(root) and target_root.is_relative_to(root)
        if args[0] == 'copy' and control.get('late_link'):
            link = source_path / 'outside-link'
            outside = root / 'unselected.txt'
            assert link.is_absolute() and source_path.is_relative_to(root)
            assert outside.resolve(strict=True).is_relative_to(root)
            link.symlink_to(outside)
        if args[0] == 'copy' and control.get('transient_skip'):
            omitted = source_path / 'transient-fifo'
            assert omitted.is_absolute() and omitted.parent.resolve().is_relative_to(root)
            os.mkfifo(omitted)
            result = subprocess.run([control['real'], *args], check=False)
            assert str(omitted) and omitted.is_absolute() and omitted.parent.resolve().is_relative_to(root)
            assert (root / '.flea-test-sandbox').is_file()
            omitted.unlink()
            sys.exit(result.returncode)
    if args[0] == 'check' and control.get('mismatch'):
        source, destination = args[args.index('--') + 1:]
        if Path(source).name == control['mismatch']:
            target = Path(destination.split(':', 1)[1]) / Path(source).name
            assert str(target) and target.is_absolute() and target.is_relative_to(root)
            assert target.resolve(strict=True).is_relative_to(root)
            before = target.stat()
            target.write_bytes(b'X' * before.st_size)
            os.utime(target, ns=(before.st_atime_ns, before.st_mtime_ns))
            after = target.stat()
            assert after.st_size == before.st_size and after.st_mtime_ns == before.st_mtime_ns
            assert target.read_bytes() != Path(source).read_bytes()
            (root / 'injected.json').write_text(json.dumps(dict(source=source, target=str(target),
                size=before.st_size, mtime_ns=before.st_mtime_ns)))
    os.execv(control['real'], [control['real'], *args])
failure = control.get({'listremotes': 'list_error', 'backend': 'features_error', 'mkdir': 'mkdir_error'}.get(args[0], ''))
if failure:
    # rclone 1.75 ends a failed command with this notice row under --use-json-log, and a dated text line otherwise.
    message = 'Failed to ' + args[0] + ': ' + failure
    if '--use-json-log' in args:
        print(json.dumps(dict(time='2026-10-18T12:00:00.000000000Z', level='notice', msg=message,
                              source='cmd/cmd.go:334')), file=sys.stderr)
    else:
        print('2026/10/18 12:00:00 NOTICE: ' + message, file=sys.stderr)
    sys.exit(1)
if args[0] == 'listremotes':
    if control.get('list_hang'):
        signal.signal(signal.SIGTERM, signal.SIG_IGN)
        time.sleep(60)
    print(control.get('remotes', 'gdrive:\nb2-photos:\nnas-crypt:'))
    sys.exit(0)
if args[:2] == ['backend', 'features']:
    print(json.dumps({'Hashes': [] if control.get('sizes') else ['md5', 'sha1'],
                      'Features': {'CanHaveEmptyDirectories': control.get('empty_folders', True)}}))
    sys.exit(0)
assert '--' in args, args
source, destination = args[args.index('--') + 1:]
assert Path(source).is_absolute()
name = Path(source).name
assert str(root) in source and destination.startswith('gdrive:')


def emit(**fields):
    row = dict(time='2026-10-18T12:00:00.000000000Z', level='notice', source='accounting/stats.go:508')
    row.update(fields)
    print(json.dumps(row), file=sys.stderr, flush=True)


if control.get('hang') == args[0]:
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    child = subprocess.Popen(['/bin/sleep', '60'])
    (root / 'pids.json').write_text(json.dumps([os.getpid(), child.pid]))
    time.sleep(60)
if args[0] in ('copy', 'copyto'):
    assert '--use-json-log' in args and args[args.index('--stats') + 1] == '150ms'
    size = Path(source).stat().st_size
    for count in (size // 3, size // 5, size):
        emit(msg='\nTransferred: ...\n', stats=dict(bytes=count, totalBytes=size, speed=240000.0,
             eta=0.003, transfers=0, transferring=[dict(name=name, size=size, bytes=count)]))
        time.sleep(0.17)
    if name == control.get('fail'):
        emit(level='error', object=name, msg='Failed to copy: connection reset')
        sys.exit(1)
    sys.exit(0)
if args[0] == 'check':
    assert '--one-way' in args
    time.sleep(0.18)
    if name == control.get('mismatch'):
        emit(level='error', object=name, msg='md5 differ', source='operations/check.go:272')
        sys.exit(1)
    if control.get('sizes') or control.get('no_hash'):
        emit(msg='1 hashes could not be checked', source='operations/check.go:243')
    emit(msg='1 matching files', source='operations/check.go:246')
    sys.exit(0)
raise AssertionError(args)
