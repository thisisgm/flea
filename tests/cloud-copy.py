#!/usr/bin/env python3
"""Direct-copy boundary tests: isolated local alias, never a production remote or mount.

The real rclone cases prove file/folder basename handling, hidden files, checksum
confirmation, collision refusal and cancellation. Every fixture is marked and lives
outside HOME. Fake transports exercise failures without network access.
"""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import time

repo = Path(__file__).resolve().parent.parent
root = Path('/home/flea-sandbox')
root.mkdir(exist_ok=True)
fixture = Path(tempfile.mkdtemp(prefix='cloud-copy-', dir=root))
(fixture / '.flea-test-sandbox').write_text('owned cloud copy test\n')
assert not fixture.is_relative_to(Path.home())
try:
    config = fixture / 'config/flea'
    config.mkdir(parents=True)
    remote = fixture / 'remote'
    remote.mkdir()
    (config / 'cloud-targets.json').write_text(json.dumps({'targets': [{'id': 'test', 'label': 'Test', 'remote': 'fixture', 'root': 'target'}]}))
    rc = fixture / 'rclone.conf'
    rc.write_text('[fixture]\ntype = alias\nremote = ' + str(remote) + '\n')
    runtime = fixture / 'runtime'
    runtime.mkdir(mode=0o700)
    env = dict(os.environ, XDG_RUNTIME_DIR=str(runtime), XDG_CONFIG_HOME=str(config.parent), RCLONE_CONFIG=str(rc), TMPDIR=str(fixture))
    executable = str(repo / 'target/debug/flea')
    source = fixture / 'source'
    source.mkdir()
    single = source / 'space ; dollar $.txt'
    single.write_text('synthetic cloud upload\n')
    folder = source / 'album'
    folder.mkdir()
    (folder / '.hidden').write_text('hidden')
    (folder / 'photo.bin').write_bytes(os.urandom(40000))
    (folder / 'empty').mkdir()

    def start(path, extra=None, folder=""):
        return subprocess.Popen([executable, '--cloud-copy', 'test', folder, str(path)], env=extra or env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)

    def result(process):
        code = process.wait(timeout=30)
        events = [json.loads(line) for line in process.stdout.read().splitlines()]
        if not process.stdin.closed: process.stdin.close()
        assert events, process.stderr.read()
        return code, events

    for path in [single, folder]:
        code, events = result(start(path))
        assert code == 0 and events[-1]['state'] == 'done', events
        assert {'preparing', 'uploading', 'verifying', 'done'} <= {e['state'] for e in events}, events
    assert (remote / 'target' / single.name).read_bytes() == single.read_bytes()
    assert (remote / 'target/album/.hidden').read_text() == 'hidden'
    assert (remote / 'target/album/photo.bin').read_bytes() == (folder / 'photo.bin').read_bytes()
    assert not (remote / 'target/album/empty').exists()
    print('PASS file/folder basenames, hidden files, explicit MD5 confirmation')
    for reserved in ['--backend', '--version']:
        code, events = result(start(single, folder=reserved))
        assert code == 0 and events[-1]['state'] == 'done', events
        assert (remote / 'target' / reserved / single.name).read_bytes() == single.read_bytes()
    print('PASS reserved-looking destination folders remain data, not mode switches')
    code, events = result(start(single))
    assert code == 0 and events[-1]['state'] == 'done', events
    (remote / 'target' / single.name).write_text('existing different content')
    code, events = result(start(single))
    assert code != 0 and events[-1]['state'] == 'error', events
    assert (remote / 'target' / single.name).read_text() == 'existing different content'
    print('PASS matching retry and immutable conflict')
    (folder / 'link').symlink_to(single)
    code, events = result(start(folder))
    assert code != 0 and 'Symlinks' in events[-1]['message'], events
    print('PASS symlink refusal')
    fakebin = fixture / 'bin'
    fakebin.mkdir()
    fake = fakebin / 'rclone'
    fake.write_text('#!/usr/bin/python3\nimport os,time\nopen(os.environ["CHILD_PID"], "w").write(str(os.getpid()))\ntime.sleep(60)\n')
    fake.chmod(0o755)
    fakeenv = dict(env, PATH=str(fakebin) + ':' + env['PATH'], CHILD_PID=str(fixture / 'child.pid'))
    child = start(single, fakeenv)
    for _ in range(100):
        if (fixture / 'child.pid').exists(): break
        time.sleep(.02)
    assert (fixture / 'child.pid').exists()
    code, events = result(start(single, fakeenv))
    assert code != 0 and 'Another Flea upload' in events[-1]['message'], events
    print('PASS cross-process configured-remote lock')
    child.stdin.write('cancel\n')
    child.stdin.flush()
    code, events = result(child)
    assert events[-1]['state'] == 'cancelled', events
    pid = int((fixture / 'child.pid').read_text())
    try: os.kill(pid, 0)
    except ProcessLookupError: pass
    else: raise AssertionError('owned rclone survived cancellation')
    assert single.read_text() == 'synthetic cloud upload\n'
    print('PASS cancellation reaps child and keeps originals')
    (fixture / 'child.pid').unlink()
    abandoned = start(single, fakeenv)
    for _ in range(100):
        if (fixture / 'child.pid').exists(): break
        time.sleep(.02)
    assert (fixture / 'child.pid').exists()
    owned_pid = int((fixture / 'child.pid').read_text())
    abandoned.kill()
    abandoned.wait(timeout=5)
    abandoned.stdin.close()
    for _ in range(100):
        proc = Path('/proc') / str(owned_pid) / 'stat'
        try: state = proc.read_text().split()[2]
        except FileNotFoundError: break
        if state == 'Z': break
        time.sleep(.01)
    else: raise AssertionError('rclone survived owner death')
    print('PASS owner death stops its rclone child')
    eof = start(single, fakeenv)
    eof.stdin.close()
    code, events = result(eof)
    assert events[-1]['state'] == 'cancelled', events
    print('PASS stdin EOF cancels without removing originals')

    # An unsupported/empty hash report must never become a verified result.
    fake.write_text('#!/usr/bin/python3\nprint("not a checksum")\n')
    code, events = result(start(single, fakeenv))
    assert code != 0 and events[-1]['state'] == 'error' and not any(e['state'] == 'uploading' for e in events)
    print('PASS malformed manifest fails before any upload')
    # Use real local copies, then edit the original after upload: remote correctness
    # cannot be advertised as confirmation of the now-changed local source.
    change = source / 'changing.txt'
    change.write_text('before upload')
    real_rclone = shutil.which('rclone')
    fake.write_text('#!/usr/bin/python3\nimport os,subprocess,sys\ncode=subprocess.call([' + repr(real_rclone) + '] + sys.argv[1:])\nif sys.argv[1] == "copy" and code == 0:\n open(sys.argv[2], "a").write("changed")\nsys.exit(code)\n')
    code, events = result(start(change, fakeenv))
    assert code != 0 and events[-1]['state'] == 'error' and 'Source changed' in events[-1]['message'], events
    assert change.read_text() == 'before uploadchanged'
    print('PASS changing source does not produce verified success')

    print('cloud-copy: all checks passed')
finally:
    assert fixture.parent == root and (fixture / '.flea-test-sandbox').is_file()
    shutil.rmtree(fixture)
