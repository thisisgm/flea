"""Password fallback and live byte progress through real jailed archive tools."""
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tarfile
import time
import zipfile


def duplicate_encrypted(root, backend):
    first, second = root / 'aa.txt', root / 'bb.txt'
    first.write_bytes(b'first secret')
    second.write_bytes(b'other secret')
    archive = root / 'duplicate.zip'
    subprocess.run(['7z', 'a', '-bd', '-ptest-password', '-mem=AES256',
                    str(archive), str(first), str(second)], check=True,
                   stdout=subprocess.DEVNULL)
    data = archive.read_bytes()
    assert data.count(b'bb.txt') == 2
    archive.write_bytes(data.replace(b'bb.txt', b'aa.txt'))
    listing = subprocess.run(['7z', 'l', '-slt', str(archive)], check=True,
                             capture_output=True, text=True).stdout
    assert listing.count('Encrypted = +') == 2
    backend.send(c='archive', op='extract', path=str(archive), dest=str(root / 'out'))
    reply = backend.wait(lambda event: event['t'] == 'archivedone')
    print('DUPLICATE_ENCRYPTED', json.dumps(reply), flush=True)
    assert reply['err'] == 'encrypted', reply
    assert not (root / 'out').exists()
    assert all(event['job']['done'] == 0 and event['job']['bytes'] == 0
               for event in backend.events if event['t'] == 'job')
    backend.send(c='archive', op='extract', path=str(archive), dest=str(root / 'out'),
                 password='test-password')
    reply = backend.wait(lambda event: event['t'] == 'archivedone')
    assert reply['ok'], reply
    assert (root / 'out/aa.txt').read_bytes() == second.read_bytes()
    job = backend.jobs[reply['id']]
    assert (job['total'], job['totalBytes']) == (2, 24), job
    print('PASSWORD_FLOW PASS duplicate members keep encrypted and accept the password', flush=True)
    for mode in ('bytes', 'timeout'):
        bounded_encrypted(root, backend, archive, mode)


def bounded_encrypted(root, backend, archive, mode):
    real = shutil.which('bwrap')
    producer = f'''import subprocess,sys,time
result = subprocess.run(sys.argv[1:], stdin=sys.stdin, stdout=subprocess.PIPE)
sys.stdout.buffer.write(result.stdout)
sys.stdout.buffer.flush()
if {mode!r} == 'bytes':
    sys.stdout.buffer.write(b'x' * (9 * 1024 * 1024))
    sys.stdout.buffer.flush()
else:
    time.sleep(3)
sys.exit(result.returncode)
'''
    wrapper = root / 'bin/bwrap'
    wrapper.parent.mkdir(exist_ok=True)
    wrapper.write_text('#!/usr/bin/python3\n' + f'''import os,sys
args = sys.argv[1:]
if '7z' in args and 'l' in args and '-slt' in args:
    position = args.index('7z')
    args[position:] = ['/usr/bin/python3','-c',{producer!r}] + args[position:]
os.execv({real!r}, [{real!r}] + args)
''')
    wrapper.chmod(0o755)
    destination = root / ('out-' + mode)
    backend.send(c='archive', op='extract', path=str(archive), dest=str(destination))
    reply = backend.wait(lambda event: event['t'] == 'archivedone', timeout=10)
    assert reply['err'] == 'encrypted', (mode, reply)
    assert not destination.exists()
    print('ENCRYPTED_FALLBACK', mode, 'encrypted', flush=True)


def live_large_tar(root, backend):
    archive = root / 'large.tar'
    large_bytes = 8 * 1024 * 1024
    with tarfile.open(archive, 'w') as output:
        for index in range(1200):
            member = tarfile.TarInfo(f'{index:04}.txt')
            member.size = 1
            output.addfile(member, io.BytesIO(b'x'))
        member = tarfile.TarInfo('large.bin')
        member.size = large_bytes
        output.addfile(member, io.BytesIO(b'z' * large_bytes))
    real = shutil.which('bwrap')
    throttle = '''import subprocess,sys,time
p = subprocess.Popen(['bsdtar','-v','-x','-f','-','-C',sys.argv[2]], stdin=subprocess.PIPE)
with open(sys.argv[1], 'rb') as source:
    while True:
        chunk = source.read(65536)
        if not chunk:
            break
        p.stdin.write(chunk)
        p.stdin.flush()
        time.sleep(.025)
p.stdin.close()
sys.exit(p.wait())
'''
    wrapper = root / 'bin/bwrap'
    wrapper.parent.mkdir()
    wrapper.write_text('#!/usr/bin/python3\n' + f'''import os,sys
args = sys.argv[1:]
if '--bind' in args and '-x' in args:
    position = args.index('bsdtar')
    stage = args[args.index('--bind') + 2] + '/out'
    args[position:] = ['/usr/bin/python3','-c',{throttle!r},{str(archive)!r},stage]
os.execv({real!r}, [{real!r}] + args)
''')
    wrapper.chmod(0o755)
    backend.send(c='archive', op='extract', path=str(archive), dest=str(root / 'out'))
    started = time.monotonic()
    observations = []
    while True:
        event = backend.read(15)
        if event['t'] == 'job':
            job = event['job']
            stage = next(root.glob('.flea-work-ext-*/out/large.bin'), None)
            try:
                written = stage.stat().st_size if stage else 0
            except FileNotFoundError:
                written = 0
            observations.append((job, written))
            print('LIVE_SAMPLE', round(time.monotonic() - started, 2), job['state'],
                  job['done'], job['bytes'], repr(job['file']),
                  'staged_bytes=' + str(written), flush=True)
        if event['t'] == 'archivedone':
            assert event['ok'], event
            break
    partial = [(job, written) for job, written in observations
               if job['state'] == 'running' and job['file'] == 'large.bin'
               and 1200 < job['bytes'] < 1200 + large_bytes and 0 < written < large_bytes]
    print('LIVE_LARGE_FILE_INTERMEDIATE', len(partial), flush=True)
    assert len(partial) >= 3, 'no repeated byte samples while the staged file grew'
    assert any(a['bytes'] < b['bytes'] and x < y
               for (a, x), (b, y) in zip(partial, partial[1:])), partial
    rows = [job for job, _ in observations]
    assert all(a['done'] <= b['done'] and a['bytes'] <= b['bytes']
               for a, b in zip(rows, rows[1:])), rows
    final = rows[-1]
    assert final['done'] == final['total'] == 1201, final
    assert final['bytes'] == final['totalBytes'] == 1200 + large_bytes, final
    assert (root / 'out/large.bin').read_bytes() == b'z' * large_bytes


def late_encryption(root, backend):
    archive = root / 'late.zip'
    with zipfile.ZipFile(archive, 'w') as output:
        for index in range(26000):
            output.writestr(f'{index:05d}-' + 'a' * 215, b'')
    secret = root / 'zzzz-secret.txt'
    secret.write_bytes(b'encrypted tail member')
    subprocess.run(['7z', 'a', '-bd', '-ptest-password', '-mem=AES256',
                    str(archive), str(secret)], check=True, stdout=subprocess.DEVNULL)
    listing = subprocess.run(['7z', 'l', '-slt', str(archive)], check=True,
                             capture_output=True).stdout
    offset = listing.index(b'Encrypted = +')
    assert offset > 8 * 1024 * 1024, offset
    destination = root / 'out'
    backend.send(c='archive', op='extract', path=str(archive), dest=str(destination))
    reply = backend.wait(lambda event: event['t'] == 'archivedone', timeout=15)
    print('LATE_ENCRYPTION', 'index_bytes=' + str(len(listing)),
          'encrypted_offset=' + str(offset), json.dumps(reply), flush=True)
    assert reply['err'] == 'encrypted', reply
    assert reply['path'] == str(archive) and reply['dest'] == str(destination), reply
    assert not destination.exists()
    assert all(event['job']['bytes'] == event['job']['done'] == 0
               for event in backend.events if event['t'] == 'job')
    backend.send(c='archive', op='extract', path=str(archive), dest=str(destination),
                 password='test-password')
    reply = backend.wait(lambda event: event['t'] == 'archivedone', timeout=20)
    assert reply['ok'], reply
    assert (destination / secret.name).read_bytes() == secret.read_bytes()


def long_password(root, backend):
    archive = root / 'plain.zip'
    with zipfile.ZipFile(archive, 'w') as output:
        for index in range(2000):
            output.writestr(f'{index:05d}.txt', b'x')
    real = shutil.which('bwrap')
    wrapper = root / 'bin/bwrap'
    wrapper.parent.mkdir()
    for mode in ('plain', 'deadline', 'cancel'):
        pid_file = root / ('index-' + mode + '.pid')
        wrapper.write_text('#!/usr/bin/python3\n' + f'''import os,sys
args = sys.argv[1:]
if '7z' in args and 'l' in args and '-slt' in args:
    open({str(pid_file)!r}, 'w').write(str(os.getpid()))
    if {mode!r} != 'plain':
        position = args.index('7z')
        args[position:] = ['/usr/bin/python3','-c','import time; time.sleep(30)']
os.execv({real!r}, [{real!r}] + args)
''')
        wrapper.chmod(0o755)
        destination = root / ('out-' + mode)
        backend.send(c='archive', op='extract', path=str(archive),
                     dest=str(destination), password='p' * 131072)
        ident = backend.wait(lambda event: event['t'] == 'extractstarted')['id']
        started = time.monotonic()
        replied = False
        try:
            if mode == 'cancel':
                while not pid_file.exists():
                    assert time.monotonic() - started < 2, 'index did not start'
                    time.sleep(.01)
                time.sleep(.1)
                backend.send(c='jobs', op='cancel', id=ident)
                started = time.monotonic()
            reply = backend.wait(lambda event: event['t'] == 'archivedone'
                                 and event['id'] == ident, timeout=5)
            replied = True
            elapsed = time.monotonic() - started
            job = backend.jobs[ident]
            print('LONG_PASSWORD', mode, 'seconds=' + str(round(elapsed, 3)),
                  json.dumps(reply), flush=True)
            if mode == 'cancel':
                assert job['cancelled'] and elapsed < 3, job
                assert not destination.exists()
            else:
                assert reply['ok'], reply
                assert elapsed < (3.5 if mode == 'plain' else 5), elapsed
                assert (destination / '01999.txt').read_bytes() == b'x'
            assert not list(root.glob('.flea-work-ext-*'))
            index_stat = Path('/proc') / pid_file.read_text() / 'stat'
            assert not index_stat.exists() or index_stat.read_text().rsplit(')', 1)[1].split()[0] == 'Z'
        finally:
            if not replied and pid_file.exists():
                assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
                try:
                    os.killpg(int(pid_file.read_text()), signal.SIGKILL)
                except ProcessLookupError:
                    pass
