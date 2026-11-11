"""Prove archive jobs share the destination-drive queue through the real backend."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import time

sys.dont_write_bytecode = True
from jobs import Backend, MARKER, Root


def barrier(root):
    launcher = root / 'bin/prlimit'
    launcher.parent.mkdir()
    real = shutil.which('prlimit')
    assert real, 'prlimit is required by the extraction sandbox'
    launcher.write_text('''#!/usr/bin/python3
import os
from pathlib import Path
import sys
import time
root = Path(__file__).resolve().parent.parent
args = sys.argv[1:]
if 'bsdtar' in args:
    tail = args[args.index('bsdtar') + 1:]
    if '-x' in tail and '-f' in tail:
        name = Path(tail[tail.index('-f') + 1]).stem
        if name.startswith('held-'):
            (root / (name + '.ready')).write_text(str(os.getpid()))
            deadline = time.monotonic() + 45
            while not (root / (name + '.release')).exists():
                assert time.monotonic() < deadline, 'extraction barrier timed out'
                time.sleep(0.01)
os.execv(''' + repr(real) + ''', [''' + repr(real) + ''', *args])
''')
    launcher.chmod(0o700)


def fixture(root, name):
    source = root / (name + '-source')
    (source / 'sub').mkdir(parents=True)
    (source / 'a.txt').write_text(name + ' alpha')
    (source / 'sub/b.txt').write_text(name + ' beta')
    archive = root / (name + '.tar')
    subprocess.run(['bsdtar', '-cf', str(archive), '-C', str(source), 'a.txt', 'sub'], check=True)
    return archive


def contents(destination, name):
    assert (destination / 'a.txt').read_text() == name + ' alpha'
    assert (destination / 'sub/b.txt').read_text() == name + ' beta'
    assert sorted(str(p.relative_to(destination)) for p in destination.rglob('*')) == ['a.txt', 'sub', 'sub/b.txt']


def event(backend, kind, ident):
    predicate = lambda e: e['t'] == kind and e.get('id') == ident
    return next((e for e in backend.events if predicate(e)), None) or backend.wait(predicate)


def extract(backend, archive, destination):
    backend.send(c='archive', op='extract', path=str(archive), dest=str(destination))
    reply = backend.wait(lambda e: e['t'] in ('archivestarted', 'error'))
    assert reply['t'] == 'archivestarted', reply
    return reply['id']


def ready(backend, archive, ident):
    backend.state(ident, 'running')
    deadline = time.monotonic() + 20
    marker = backend.root / (archive.stem + '.ready')
    while not marker.exists():
        assert time.monotonic() < deadline, 'extract never reached its real launcher'
        time.sleep(0.01)


def release(backend, archive):
    (backend.root / (archive.stem + '.release')).touch()


def waiting(backend, ident):
    backend.wait(lambda e: backend.jobs.get(ident, {}).get('reason', '').startswith('waiting for the extraction to '))
    job = backend.jobs[ident]
    assert job['state'] == 'waiting' and job['reason'].removeprefix('waiting for the extraction to '), job
    assert job['done'] == 0 and job['bytes'] == 0, job


def success(backend, ident, archive, destination):
    done = event(backend, 'archivedone', ident)
    assert done == dict(t='archivedone', id=ident, ok=True, verified=True, err='',
                        path=str(archive), dest=str(destination)), done
    rows = [e['job'] for e in backend.events if e['t'] == 'job' and e['job']['id'] == ident]
    source = archive.parent / (archive.stem + '-source')
    expected_bytes = sum(p.stat().st_size for p in source.rglob('*') if p.is_file())
    assert all(a['done'] <= b['done'] and a['bytes'] <= b['bytes'] for a, b in zip(rows, rows[1:])), rows
    final = rows[-1]
    assert final['state'] == 'done' and not final['cancelled'], final
    assert final['done'] == final['total'] == 3, final
    assert final['bytes'] == final['totalBytes'] == expected_bytes, final
    assert final['file'] and final['itemBytes'] > 0, final


def transfers_done(backend, ident):
    done = event(backend, 'transferdone', ident)
    assert done['ok'] == 1 and done['failed'] == 0 and not done['cancelled'], done


def scenario(backend, root, temporary, archives):
    first, cancelled, second, other, running_cancel, later = archives
    one = extract(backend, first, root / 'first')
    ready(backend, first, one)
    assert event(backend, 'extractstarted', one) == dict(t='extractstarted', id=one)
    assert event(backend, 'transferprogress', one) == dict(t='transferprogress', id=one, index=0,
                                                         name=first.name, bytes=0, total=0, scanned=0)
    two = extract(backend, cancelled, root / 'cancelled-waiting')
    waiting(backend, two)
    assert not (root / (cancelled.stem + '.ready')).exists()
    backend.send(c='transfercancel', id=two)
    assert backend.state(two, 'done')['cancelled']
    terminal = event(backend, 'archivedone', two)
    assert terminal == dict(t='archivedone', id=two, ok=False, verified=True, err='cancelled'), terminal
    assert backend.jobs[one]['state'] == 'running' and not backend.jobs[one]['cancelled']
    assert not (root / 'cancelled-waiting').exists() and not (root / 'first').exists()
    assert not any(e['t'] == 'archivedone' and e['id'] == one for e in backend.events)
    print('PASS second extract numbered, named same-drive wait, waiting Cancel preserves running extract')

    three = extract(backend, second, root / 'second')
    waiting(backend, three)
    source = root / 'copy.txt'
    source.write_text('copy during extraction')
    local_copy = root / 'copy'
    local_copy.mkdir()
    four = backend.copy([source], local_copy)
    waiting(backend, four)
    move_source = root / 'move.txt'
    move_source.write_text('move during extraction')
    local_move = root / 'move'
    local_move.mkdir()
    backend.send(c='transfer', op='move', paths=[str(move_source)], dest=str(local_move))
    five = backend.wait(lambda e: e['t'] == 'transferstarted')['id']
    waiting(backend, five)
    assert not (local_copy / source.name).exists() and not (local_move / move_source.name).exists()
    assert move_source.read_text() == 'move during extraction'

    remote_copy = temporary / 'copy'
    remote_copy.mkdir()
    six = backend.copy([source], remote_copy)
    transfers_done(backend, six)
    assert (remote_copy / source.name).read_text() == 'copy during extraction'
    assert backend.jobs[one]['state'] == 'running'
    seven = extract(backend, other, temporary / 'other')
    ready(backend, other, seven)
    assert all(backend.jobs[i]['state'] == 'running' for i in (one, seven))
    release(backend, other)
    success(backend, seven, other, temporary / 'other')
    contents(temporary / 'other', other.stem)
    assert backend.jobs[one]['state'] == 'running' and not (root / 'first').exists()
    print('PASS different tmpfs drive runs an extract and a copy beside the held extract')

    release(backend, first)
    success(backend, one, first, root / 'first')
    contents(root / 'first', first.stem)
    ready(backend, second, three)
    assert all(backend.jobs[i]['state'] == 'waiting' for i in (four, five))
    assert not (local_copy / source.name).exists() and move_source.exists()
    release(backend, second)
    success(backend, three, second, root / 'second')
    contents(root / 'second', second.stem)
    transfers_done(backend, four)
    transfers_done(backend, five)
    assert (local_copy / source.name).read_text() == 'copy during extraction'
    assert (local_move / move_source.name).read_text() == 'move during extraction' and not move_source.exists()
    assert [one, two, three, four, five, six, seven] == list(range(1, 8))
    assert not any(e['t'] == 'error' and e.get('msg') == 'an operation is already running' for e in backend.events)
    print('PASS same-drive extract, copy and move run in arrival order, with exact extracted payloads')

    eight = extract(backend, running_cancel, root / 'cancelled-running')
    ready(backend, running_cancel, eight)
    started = time.monotonic()
    backend.send(c='transfercancel', id=eight)
    assert event(backend, 'archivedone', eight) == dict(t='archivedone', id=eight, ok=False, verified=True, err='cancelled')
    assert backend.state(eight, 'done')['cancelled']
    elapsed = time.monotonic() - started
    assert not (root / 'cancelled-running').exists()
    nine = extract(backend, later, root / 'later')
    success(backend, nine, later, root / 'later')
    contents(root / 'later', later.stem)
    assert not list(root.rglob('.flea-work-*')) and not list(temporary.rglob('.flea-work-*'))
    print(f'PASS running Cancel ({elapsed:.4f}s), no destination or work directory, later extraction succeeds')


def main():
    parent = Path(sys.argv[2]).resolve()
    assert parent.is_absolute() and (parent / MARKER).is_file()
    root = Root(str(parent))
    temporary = Root(tempfile.gettempdir())
    backend = None
    archives = []
    try:
        assert root.path.stat().st_dev != temporary.path.stat().st_dev, 'requires a different drive'
        assert subprocess.check_output(['stat', '-f', '-c', '%T', str(temporary.path)], text=True).strip() == 'tmpfs'
        barrier(root.path)
        archives = [fixture(root.path, name) for name in
                    ('held-first', 'held-waiting', 'held-second', 'held-other', 'held-cancel', 'later')]
        backend = Backend(root.path)
        scenario(backend, root.path, temporary.path, archives)
    finally:
        if backend:
            for archive in archives:
                release(backend, archive)
            backend.close()
        temporary.close()
        root.close()


if __name__ == '__main__':
    main()
