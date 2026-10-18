"""Extraction progress acceptance, with every write confined to a marked root."""
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tarfile
import time
import zipfile

from jobs import Backend, Root
from unzip_progress_regressions import duplicate_encrypted, late_encryption, live_large_tar, long_password

SCRATCH = Path('.superpowers/b-unzip-progress').resolve()


def script(root, name, body):
    path = root / 'bin' / name
    path.parent.mkdir(exist_ok=True)
    path.write_text('#!/usr/bin/python3\n' + body)
    path.chmod(0o755)


def wrap(root, slow_index=False, cancel=False):
    real = shutil.which('bwrap')
    script(root, 'bwrap', f'''import os,subprocess,sys,time,signal
args=sys.argv[1:]
if '--bind' in args and '-x' in args or '--bind' in args and 'x' in args:
    i=args.index('--bind'); stage=args[i+2]+'/out'
    if {cancel!r}:
        child=subprocess.Popen(['/usr/bin/python3','-c','import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); time.sleep(120)'])
        open(stage+'/sibling.pid','w').write(str(child.pid))
        pos=next(j for j,a in enumerate(args) if a in ('bsdtar','7z'))
        args[pos:]=['/usr/bin/python3','-c',"import signal,time; signal.signal(signal.SIGTERM,signal.SIG_IGN); open("+repr(stage+'/partial')+",'wb').write(b'partial'); time.sleep(120)"]
        os.execv({real!r},[{real!r}]+args)
    p=subprocess.Popen([{real!r}]+args)
    rc=p.wait()
    time.sleep(.45)
    sys.exit(rc)
if {slow_index!r} and ('l' in args or '-t' in args): time.sleep(2.5)
os.execv({real!r},[{real!r}]+args)
''')


def extract(backend, archive, dest, password=''):
    backend.send(c='archive', op='extract', path=str(archive), dest=str(dest), password=password)
    return backend.wait(lambda e: e['t'] == 'extractstarted')['id']


def finish(backend, ident):
    backend.wait(lambda e: e['t'] == 'archivedone' and e['id'] == ident, timeout=35)
    return backend.jobs[ident]


def samples(backend, ident):
    return [e['job'] for e in backend.events if e['t'] == 'job' and e['job']['id'] == ident]


def progress(backend, ident, count, size, unknown=False):
    rows = samples(backend, ident)
    if size > 1024 * 1024:
        assert any(r['state'] == 'running' and r['done'] > 0 for r in rows), 'no running entry progress'
        assert any(r['state'] == 'running' and r['bytes'] > 0 for r in rows), 'no running unpacked byte progress'
    assert all(a['done'] <= b['done'] and a['bytes'] <= b['bytes'] for a,b in zip(rows, rows[1:])), rows
    last = rows[-1]
    assert last['state'] == 'done' and not last['cancelled'], last
    assert last['done'] == count and last['bytes'] == size, last
    assert last['total'] == (0 if unknown else count), last
    assert last['totalBytes'] == (0 if unknown else size), last
    if unknown:
        assert all(r['left'] is None for r in rows), rows
    else:
        assert any(r['itemBytes'] > 0 and r['file'] for r in rows), rows
    print('MEASURE', json.dumps(dict(entries=count, bytes=size, samples=len(rows), unknown=unknown)))


def fixture(root, kind):
    source = root / 'source'
    source.mkdir()
    for i in range(200):
        (source / f'{i:03}.txt').write_bytes(b'x' * 91)
    (source / 'large.bin').write_bytes(b'0123456789abcdef' * (4 * 1024 * 1024))
    archive = root / ('sample.' + kind)
    if kind == 'zip':
        with zipfile.ZipFile(archive, 'w', compression=zipfile.ZIP_DEFLATED) as z:
            for p in source.iterdir(): z.write(p, p.name)
    elif kind == 'tar.gz':
        with tarfile.open(archive, 'w:gz') as t:
            for p in source.iterdir(): t.add(p, arcname=p.name)
    else:
        subprocess.run(['7z','a','-bd','-ms=on',str(archive),'.'], cwd=source, check=True, stdout=subprocess.DEVNULL)
    return archive, 201, sum(p.stat().st_size for p in source.iterdir())


def ordinary(root, backend, kind, unknown=False):
    archive, count, size = fixture(root, kind)
    wrap(root, slow_index=unknown)
    ident = extract(backend, archive, root / 'out')
    finish(backend, ident)
    progress(backend, ident, count, size, unknown)
    assert len(list((root/'out').iterdir())) == count
    assert (root/'out/large.bin').read_bytes() == (root/'source/large.bin').read_bytes()


def password(root, backend):
    password_kind(root, backend, '7z', ['-mhe=on'])
    password_kind(root, backend, 'zip', ['-mem=AES256'])


def password_kind(root, backend, kind, flags):
    source = root / 'secret.txt'; source.write_text('protected contents')
    archive = root / ('locked.' + kind)
    subprocess.run(['7z','a','-bd','-ptest-password',*flags,str(archive),str(source)], check=True, stdout=subprocess.DEVNULL)
    wrap(root)
    destination = root / ('out-' + kind)
    for secret, expected in [('', 'encrypted'), ('incorrect', 'wrong-password')]:
        ident = extract(backend, archive, destination, secret)
        finish(backend, ident)
        reply = next(e for e in reversed(backend.events) if e['t']=='archivedone')
        assert reply['err'] == expected, reply
        assert all(r['done']==0 and r['bytes']==0 for r in samples(backend, ident))
        assert not (destination).exists()
    ident = extract(backend, archive, destination, 'test-password')
    finish(backend, ident)
    progress(backend, ident, 1, len(source.read_bytes()))


def cancellation(root, backend):
    archive, _, _ = fixture(root, 'zip')
    wrap(root, cancel=True)
    ident = extract(backend, archive, root/'out')
    deadline = time.monotonic()+15
    while not list(root.glob('.flea-work-ext-*/out/partial')):
        assert time.monotonic()<deadline, 'extract never entered its staged child'
        time.sleep(.02)
    pid = int(next(root.glob('.flea-work-ext-*/out/sibling.pid')).read_text())
    backend.send(c='jobs', op='cancel', id=ident)
    result = finish(backend, ident)
    assert result['cancelled'] and result['state']=='done', result
    assert not (root/'out').exists(), 'cancel published partial destination'
    assert not list(root.glob('.flea-work-ext-*')), 'cancel left staging folder'
    deadline = time.monotonic()+3
    while Path(f'/proc/{pid}/stat').exists():
        state=Path(f'/proc/{pid}/stat').read_text().rsplit(')',1)[1].split()[0]
        if state=='Z': break
        assert time.monotonic()<deadline, 'SIGTERM-ignoring process survived Cancel'
        time.sleep(.02)


def corrupt(root, backend):
    archive=root/'broken.zip'; archive.write_bytes(b'not an archive')
    ident=extract(backend,archive,root/'out')
    result=finish(backend,ident)
    assert result['state']=='failed' and result['reason'],result
    assert not (root/'out').exists()


def rar(root, backend):
    archive=root/'sample.rar'
    subprocess.run(['./tools/flea-rar-fixture',str(archive),'hello.txt','hello from rar'],check=True)
    wrap(root)
    ident=extract(backend,archive,root/'out')
    finish(backend,ident)
    progress(backend,ident,1,len('hello from rar'))


def tar_family(root, backend, kind):
    source = root / 'source'
    source.mkdir()
    (source / 'entry.txt').write_bytes(b'tar family contents')
    archive = root / ('sample.' + kind)
    subprocess.run(['bsdtar', '-a', '-c', '-f', str(archive), '-C', str(source), '.'], check=True)
    wrap(root)
    ident = extract(backend, archive, root / 'out')
    finish(backend, ident)
    progress(backend, ident, 1, len(b'tar family contents'))
    assert (root / 'out/entry.txt').read_bytes() == b'tar family contents'


def main():
    SCRATCH.mkdir(parents=True,exist_ok=True)
    failures=[]
    cases=[('zip',lambda r,b:ordinary(r,b,'zip')),('tar.gz',lambda r,b:ordinary(r,b,'tar.gz')),
           ('cancel',cancellation),('corrupt',corrupt)]
    cases += [(kind, lambda r,b,kind=kind:tar_family(r,b,kind)) for kind in ('tar','tar.bz2','tar.xz','tar.zst')]
    cases += [('unknown-tar', lambda r,b:ordinary(r,b,'tar.gz',True))]
    cases += [('live-byte-progress', live_large_tar)]
    if shutil.which('7z'):
        cases += [('solid-7z',lambda r,b:ordinary(r,b,'7z')),('unknown-7z',lambda r,b:ordinary(r,b,'7z',True)),('password',password),('rar',rar)]
        cases += [('duplicate-encrypted', duplicate_encrypted)]
        cases += [('late-encryption', late_encryption), ('long-password', long_password)]
    else: print('SKIP 7z, password, rar: 7z unavailable')
    for name,case in cases:
        sandbox=Root(SCRATCH); backend=None
        try:
            backend=Backend(sandbox.path)
            case(sandbox.path,backend)
            print('PASS',name)
        except Exception as error:
            failures.append(name)
            print('FAIL',name,repr(error),flush=True)
        finally:
            if backend:
                (SCRATCH/(name+'.jsonl')).write_text(''.join(json.dumps(e)+'\n' for e in backend.events))
                backend.close()
            sandbox.close()
    assert not failures, failures
    print(f'PASS unzip-progress: {len(cases)} cases passed', flush=True)


if __name__=='__main__': main()
