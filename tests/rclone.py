"""Drive checked sends, failure, retry and cancellation through flea --backend."""
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import time
from unittest.mock import patch

from jobs import BINARY, Backend as JobsBackend, Root
from rclone_edges import cases as edge_cases

REAL_RCLONE = shutil.which('rclone')
STUB = Path('tests/fixtures/rclone-stub.py').resolve()


class Backend(JobsBackend):
    def __init__(self, root, cwd=''):
        self.root = root
        env = dict(os.environ)
        for key, name in [('HOME', 'home'), ('XDG_CONFIG_HOME', 'config'),
                          ('XDG_DATA_HOME', 'data'), ('XDG_STATE_HOME', 'state'),
                          ('XDG_CACHE_HOME', 'cache'), ('XDG_RUNTIME_DIR', 'runtime'),
                          ('TMPDIR', 'tmp')]:
            folder = root / name
            folder.mkdir(exist_ok=True)
            env[key] = str(folder)
        assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
        self.env = env
        (root / cwd).mkdir(parents=True, exist_ok=True)
        self.process = subprocess.Popen([BINARY, '--backend'], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        cwd=root / cwd, env=env, start_new_session=True)
        self.pending = b''
        self.events = []
        self.jobs = {}


class Fixture:
    def __init__(self, stub=True, **control):
        scratch = Path('.superpowers/b-rclone')
        scratch.mkdir(parents=True, exist_ok=True)
        self.owned = Root(str(scratch.resolve()))
        self.root = self.owned.path
        self.control = control
        self.configure()
        bindir = self.root / 'bin'
        bindir.mkdir()
        if stub:
            (bindir / 'rclone').symlink_to(STUB)
        self.original = os.environ.copy()
        for key in list(os.environ):
            if key.startswith('RCLONE_'):
                del os.environ[key]
        os.environ['PATH'] = str(bindir)
        os.environ['XDG_CACHE_HOME'] = str(self.root / 'cache')
        os.environ['RCLONE_CONFIG'] = str(self.root / 'rclone.conf')
        if control.get('inherited_size_only'):
            os.environ['RCLONE_SIZE_ONLY'] = 'true'
        os.environ.update(control.get('inherited', {}))
        (self.root / 'rclone.conf').write_text('[gdrive]\ntype = local\n')
        self.backend = Backend(self.root, control.get('cwd', ''))
        source = self.root / 'source'
        source.mkdir()
        self.paths = [source / name for name in ['first.txt', 'two words.jpg', '-leading.txt']]
        for index, path in enumerate(self.paths):
            path.write_bytes(bytes([65 + index]) * 12000)

    def configure(self, **control):
        self.control.update(control)
        (self.root / 'control.json').write_text(json.dumps(self.control))

    def remotes(self):
        self.backend.send(c='rclone', op='remotes', id=71)
        return self.backend.wait(lambda e: e['t'] == 'rcloneremotes', timeout=5)

    def send(self, paths=None, folder='Uploads'):
        self.backend.send(c='rclone', op='send', id=72, remote='gdrive', folder=folder,
                          paths=[str(p) for p in (paths or self.paths)])
        reply = self.backend.wait(lambda e: e['t'] == 'rclonestarted', timeout=5)
        assert reply['ok'], reply
        return reply['id']

    def calls(self):
        return [json.loads(line) for line in (self.root / 'argv.jsonl').read_text().splitlines()]

    def close(self):
        self.backend.close()
        os.environ.clear()
        os.environ.update(self.original)
        self.owned.close()


def absent(f):
    reply = f.remotes()
    assert reply['remotes'] == [] and reply['visible'] is False, reply


def sent(f):
    assert f.remotes()['remotes'] == ['gdrive', 'b2-photos', 'nas-crypt']
    ident = f.send()
    job = f.backend.state(ident, 'done')
    assert not job['cancelled'] and job['check'] == 'checksum' and not job['undo'], job
    samples = [e['job'] for e in f.backend.events if e['t'] == 'job' and e['job']['id'] == ident]
    assert any(j['head'] == 'Sending 2 of 3 to gdrive' for j in samples), samples
    assert any(j['head'] == 'Checking 3 items on gdrive' and j['phase'] == 'checking' for j in samples)
    counts = [j['bytes'] for j in samples]
    assert counts == sorted(counts) and len(set(counts)) >= 4, counts
    assert any(j['rate'] == 240000 for j in samples)
    assert job['receipt'] == 'Sent 3 items to Uploads on gdrive'
    assert f.backend.wait(lambda e: e['t'] == 'rclonesent')['check'] == 'checksum'


def mismatch(f):
    ident = f.send()
    job = f.backend.state(ident, 'failed')
    assert job['head'] == 'Send failed' and 'two words.jpg' in job['reason'], job
    assert not job['undo']
    f.backend.send(c='jobs', op='dismiss', id=ident)
    f.backend.wait(lambda e: e['t'] == 'jobs' and ident not in f.backend.jobs)


def alive(pid):
    try:
        return Path(f'/proc/{pid}/stat').read_text().split(') ', 1)[1][0] != 'Z'
    except (FileNotFoundError, ProcessLookupError):
        return False


def cancel(f):
    for gone in [FileNotFoundError(), ProcessLookupError()]:
        with patch.object(Path, 'read_text', side_effect=gone):
            assert not alive(123)
    ident = f.send()
    deadline = time.monotonic() + 8
    while not (f.root / 'pids.json').exists() and time.monotonic() < deadline:
        time.sleep(0.02)
    pids = json.loads((f.root / 'pids.json').read_text())
    assert all(alive(pid) for pid in pids)
    start = time.monotonic()
    f.backend.send(c='jobs', op='cancel', id=ident)
    job = f.backend.state(ident, 'done')
    assert job['cancelled'] and not job['undo'], job
    while any(alive(pid) for pid in pids) and time.monotonic() - start < 3:
        time.sleep(0.02)
    assert not any(alive(pid) for pid in pids), pids
    print(f'  cancel latency {time.monotonic() - start:.3f}s')
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent')
    if f.control['hang'] == 'check':
        assert receipt['check'] == 'none' and receipt['note'] == 'not checked', receipt


def argv(f):
    ident = f.send(folder='-folder with spaces')
    f.backend.state(ident, 'done')
    copies = [c for c in f.calls() if c[0] in ('copy', 'copyto')]
    assert len(copies) == 3, copies
    assert [c[c.index('--') + 1] for c in copies] == [str(p) for p in f.paths]
    assert all(c[c.index('--') + 2] == 'gdrive:-folder with spaces/' + p.name
               for c, p in zip(copies, f.paths)), copies


def retry(f):
    ident = f.send()
    f.backend.state(ident, 'failed')
    before = len(f.calls())
    f.configure(fail='', mismatch='')
    f.backend.send(c='jobs', op='retry', id=ident)
    f.backend.wait(lambda e: e['t'] == 'job' and e['job']['id'] == ident and e['job']['state'] == 'running')
    job = f.backend.state(ident, 'done')
    copies = [c for c in f.calls()[before:] if c[0] in ('copy', 'copyto')]
    assert len(copies) == 1 and copies[0][-2] == str(f.paths[1]), copies
    assert job['total'] == 1 and job['check'] == 'checksum', job


def sizes(f):
    ident = f.send()
    job = f.backend.state(ident, 'done')
    assert job['check'] == 'size', job
    assert any(e['t'] == 'job' and e['job']['file'] == 'comparing sizes' for e in f.backend.events)
    checking = [e['job'] for e in f.backend.events if e['t'] == 'job' and e['job']['id'] == ident
                and e['job']['phase'] == 'checking']
    if f.control.get('sizes'):
        assert checking and all(j['check'] == 'size' and j['file'] == 'comparing sizes'
                                for j in checking), checking


def refused_paths(f):
    parent = str(f.root / 'source/..')
    for request, (paths, error) in enumerate([
            (['/'], 'rclone cannot send /, it has no file name.'),
            ([parent], 'rclone cannot send ' + parent + ', it has no file name.'),
            (['source/first.txt'], 'rclone requires an absolute selection path, not source/first.txt.'),
            (['/a\0b'], 'rclone cannot send /a, it contains a NUL byte.'),
            ([], 'Choose at least one item to send with rclone.')], 301):
        f.backend.send(c='rclone', op='send', id=request, remote='gdrive', folder='Uploads', paths=paths)
        reply = f.backend.wait(lambda e: e['t'] == 'rclonestarted' and e['requestId'] == request)
        assert not reply['ok'] and 'id' not in reply, reply
        assert reply['error'] == error, reply
    f.backend.send(c='jobs', op='list')
    assert f.backend.wait(lambda e: e['t'] == 'jobs')['jobs'] == []
    assert not (f.root / 'argv.jsonl').exists() and f.backend.process.poll() is None


def failures_carry_rclone(f):
    long_cause = 'didn\'t find section in config file ' + 'x' * 9000
    tree = f.root / 'source/tree'
    tree.mkdir()
    (tree / 'payload.txt').write_text('selected content')
    for control, paths, sentence, cause in [
            ('list_error', [f.paths[0]], 'Could not list rclone remotes.', 'Failed to listremotes: config is corrupt'),
            ('features_error', [f.paths[0]], 'Could not read the capabilities of rclone remote gdrive.',
             'Failed to backend: ' + long_cause),
            ('mkdir_error', [tree], 'Could not create the folder gdrive:Uploads/tree.',
             'Failed to mkdir: permission denied')]:
        controls = dict(list_error='', features_error='', mkdir_error='')
        controls[control] = cause.split(': ', 1)[1]
        f.configure(**controls)
        ident = f.send(paths=paths)
        reason = f.backend.state(ident, 'failed')['reason']
        assert reason == sentence + ' ' + cause[:4096], (control, reason[:300], len(reason))
        f.backend.send(c='jobs', op='dismiss', id=ident)
        f.backend.wait(lambda e: e['t'] == 'jobs' and ident not in f.backend.jobs)
    f.configure(mkdir_error='', remotes='b2-photos:')
    ident = f.send()
    assert f.backend.state(ident, 'failed')['reason'] == 'rclone remote gdrive is no longer configured.'


def unknown_backend(f):
    (f.root / 'bin/rclone').symlink_to(REAL_RCLONE)
    config = f.root / 'rclone.conf'
    assert config.is_absolute() and config.is_relative_to(f.root) and (f.root / '.flea-test-sandbox').is_file()
    config.write_text('[gdrive]\ntype = nosuchbackend\n')
    ident = f.send(paths=[f.paths[0]], folder=str(f.root / 'remote'))
    reason = f.backend.state(ident, 'failed')['reason']
    assert reason.startswith('Could not read the capabilities of rclone remote gdrive. '), reason
    assert 'nosuchbackend' in reason, reason


def env_only_remote(f):
    (f.root / 'bin/rclone').symlink_to(REAL_RCLONE)
    config = f.root / 'rclone.conf'
    assert config.is_absolute() and config.is_relative_to(f.root) and (f.root / '.flea-test-sandbox').is_file()
    config.unlink()
    native = subprocess.run([REAL_RCLONE, 'backend', 'features', '--', 'gdrive:'], env=f.backend.env,
                            cwd=f.root, text=True, capture_output=True, timeout=10)
    assert native.returncode == 0 and json.loads(native.stdout)['Hashes'], native.stderr
    assert 'not found - using defaults' in native.stderr, 'rclone wrote no stderr notice to separate'
    destination = f.root / 'remote'
    ident = f.send(paths=[f.paths[0]], folder=str(destination))
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert receipt['ok'] and receipt['check'] == 'checksum', receipt
    assert (destination / f.paths[0].name).read_bytes() == f.paths[0].read_bytes()


CARGO_STUB = '''#!/usr/bin/python3
import json
import os
from pathlib import Path
root = Path(__file__).resolve().parent.parent
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
target = Path(os.environ['CARGO_TARGET_DIR']) / 'debug/flea'
assert target.is_absolute() and target.resolve().is_relative_to(root)
if os.environ.get('STUB_BUILD') != 'absent':
    target.parent.mkdir(parents=True)
    target.write_text('#!/bin/sh\\necho fresh\\n')
    target.chmod(0o700)
(root / 'built.txt').write_text(str(target))
print(json.dumps({'reason': 'build-finished', 'success': True}))
'''
UNSHARE_STUB = '''#!/usr/bin/python3
import json
from pathlib import Path
import subprocess
import sys
root = Path(__file__).resolve().parent.parent
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
selected = Path(next(arg.split('=', 1)[1] for arg in sys.argv if arg.startswith('FLEA_BIN=')))
assert selected.is_absolute() and selected.resolve().is_relative_to(root)
marker = subprocess.check_output([str(selected)], text=True).strip()
(root / 'selected.json').write_text(json.dumps({'path': str(selected), 'marker': marker}))
'''


def ci_binary(f):
    for folder in ['tests', 'tools', 'target/debug']:
        (f.root / folder).mkdir(parents=True)
    shutil.copyfile('tests/rclone-ci.sh', f.root / 'tests/rclone-ci.sh')
    for path, text in [('tools/flea-clippy', '#!/bin/sh\nexit 0\n'), ('target/debug/flea', '#!/bin/sh\necho stale\n'),
                       ('bin/cargo', CARGO_STUB), ('bin/unshare', UNSHARE_STUB)]:
        (f.root / path).write_text(text)
        (f.root / path).chmod(0o700)
    env = dict(PATH=str(f.root / 'bin') + ':/usr/bin:/bin', LC_ALL='C.UTF-8')
    run = subprocess.run(['/bin/bash', str(f.root / 'tests/rclone-ci.sh')], env=env, cwd=f.root,
                         capture_output=True, text=True, timeout=60)
    assert run.returncode == 0, (run.stdout, run.stderr)
    selected = json.loads((f.root / 'selected.json').read_text())
    assert selected == dict(path=(f.root / 'built.txt').read_text(), marker='fresh'), selected
    (f.root / 'selected.json').unlink()
    run = subprocess.run(['/bin/bash', str(f.root / 'tests/rclone-ci.sh')], env=dict(env, STUB_BUILD='absent'),
                         cwd=f.root, capture_output=True, text=True, timeout=60)
    assert run.returncode == 1 and not (f.root / 'selected.json').exists(), (run.stdout, run.stderr)


def bounded(f):
    start = time.monotonic()
    absent(f)
    assert time.monotonic() - start < 4


def real(f):
    f.configure(real=REAL_RCLONE)
    (f.root / 'bin/rclone').symlink_to(STUB)
    assert f.remotes()['remotes'] == ['gdrive']
    destination = f.root / 'remote'
    ident = f.send(folder=str(destination))
    job = f.backend.state(ident, 'done')
    assert job['check'] == 'checksum', job
    assert job['bytes'] == sum(path.stat().st_size for path in f.paths), job
    assert all((destination / p.name).read_bytes() == p.read_bytes() for p in f.paths)
    source = f.paths[1]
    target = destination / source.name
    f.configure(mismatch=source.name)
    ident = f.send(folder=str(destination))
    failed = f.backend.state(ident, 'failed')
    assert source.name in failed['reason'], failed
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert not receipt['ok'] and receipt['check'] == 'none' and 'md5 differ' in receipt['error'], receipt
    injected = json.loads((f.root / 'injected.json').read_text())
    assert injected['source'] == str(source) and injected['target'] == str(target), injected
    assert target.stat().st_size == source.stat().st_size == injected['size']
    assert target.stat().st_mtime_ns == injected['mtime_ns']
    assert target.read_bytes() != source.read_bytes(), 'test did not corrupt the chosen remote object'
    before = len(f.calls())
    f.configure(mismatch='')
    f.backend.send(c='jobs', op='retry', id=ident)
    f.backend.wait(lambda e: e['t'] == 'job' and e['job']['id'] == ident and e['job']['state'] == 'running')
    job = f.backend.state(ident, 'done')
    assert job['total'] == 1 and job['check'] == 'checksum', job
    copies = [c for c in f.calls()[before:] if c[0] in ('copy', 'copyto')]
    assert len(copies) == 1 and copies[0][-2] == str(source), copies
    assert copies[0][-1] == 'gdrive:' + str(target), copies
    assert target.read_bytes() == source.read_bytes(), 'Retry skipped a same-size, same-time corrupt file'


def inherited_filter(f):
    (f.root / 'bin/rclone').symlink_to(REAL_RCLONE)
    tree = f.root / 'source/tree'
    tree.mkdir()
    (tree / 'must-send.txt').write_text('selected content')
    (tree / 'nested').mkdir()
    (tree / 'nested/deep.txt').write_text('selected descendant')
    destination = f.root / 'remote'
    ident = f.send(paths=[tree], folder=str(destination))
    job = f.backend.state(ident, 'done')
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert receipt['ok'] and receipt['check'] == 'checksum', receipt
    target = destination / tree.name / 'must-send.txt'
    assert target.is_file(), 'checksum receipt omitted the explicitly selected content'
    assert target.read_bytes() == (tree / 'must-send.txt').read_bytes(), job
    assert (destination / 'tree/nested/deep.txt').read_bytes() == (tree / 'nested/deep.txt').read_bytes()


def empty_directories(f):
    (f.root / 'bin/rclone').symlink_to(REAL_RCLONE)
    empty = f.root / 'source/empty'
    empty.mkdir()
    tree = f.root / 'source/tree'
    (tree / 'nested/empty-descendant').mkdir(parents=True)
    (tree / 'must-send.txt').write_text('selected content')
    destination = f.root / 'remote'
    ident = f.send(paths=[empty, tree], folder=str(destination))
    f.backend.state(ident, 'done')
    receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
    assert receipt['ok'], receipt
    assert (destination / 'empty').is_dir(), 'checked receipt for an absent selected empty directory'
    assert (destination / 'tree/nested/empty-descendant').is_dir(), 'empty descendant was omitted'


def corruption_contract(f):
    real(f)
    failures = [e for e in f.backend.events if e['t'] == 'rclonesent' and not e['ok']]
    assert len(failures) == 1 and 'md5 differ' in failures[0]['error'], failures
    assert (f.root / 'injected.json').is_file(), 'real-check corruption was never injected'
    assert not (f.root / 'Uploads').exists(), 'real send escaped its chosen destination'


def unsupported_empty_directories(f):
    for name in ['empty', 'with-empty-child']:
        source = f.root / 'source' / name
        source.mkdir()
        if name == 'with-empty-child':
            (source / 'payload.txt').write_text('content')
            (source / 'empty-child').mkdir()
        ident = f.send(paths=[source])
        job = f.backend.state(ident, 'failed')
        receipt = f.backend.wait(lambda e: e['t'] == 'rclonesent' and e['id'] == ident)
        assert not receipt['ok'] and receipt['check'] == 'none', receipt
        assert 'empty' in job['reason'] and 'cannot store' in job['reason'], job
        assert not any(c[0] in ('copy', 'copyto') for c in f.calls())


def main():
    if 'FLEA_RCLONE_PARENT_NS' in os.environ:
        assert set(os.environ) <= {'PATH', 'LC_ALL', 'FLEA_BIN', 'FLEA_RCLONE_PARENT_NS'}, os.environ.keys()
        assert not sys.stdin.isatty() and sys.stdin.buffer.read(1) == b''
        assert os.readlink('/proc/self/ns/pid') != os.environ['FLEA_RCLONE_PARENT_NS']
        try:
            with open('/dev/tty', 'rb'):
                raise AssertionError('rclone CI suite has a controlling tty')
        except OSError:
            pass
        print('PASS rclone CI environment and process isolation assertions', flush=True)
    cases = [('no rclone hides row', absent, dict(stub=False)),
             ('no remotes hides row', absent, dict(remotes='')),
             ('three files end checked with monotonic stats', sent, {}),
             ('mismatch names file and can be dismissed', mismatch, dict(mismatch='two words.jpg')),
             ('Cancel kills SIGTERM-ignoring group', cancel, dict(hang='copyto')),
             ('spaces and leading dashes remain argv', argv, {}),
             ('Retry sends only copy failure', retry, dict(fail='two words.jpg')),
             ('Retry sends only check failure', retry, dict(mismatch='two words.jpg')),
             ('size-only remote reports size check', sizes, dict(sizes=True)),
             ('paths without a file name are refused before a job', refused_paths, {}),
             ('rclone failures carry its bounded stderr and name the remote', failures_carry_rclone, {}),
             ('CI parity runs the binary its own build made', ci_binary, dict(stub=False)),
             ('missing object hashes downgrade receipt', sizes, dict(no_hash=True)),
             ('Cancel checking reports not checked', cancel, dict(hang='check')),
             ('discovery has a wall deadline', bounded, dict(list_hang=True)),
             ('unsupported empty folders are refused without a checked receipt', unsupported_empty_directories,
              dict(empty_folders=False))]
    if REAL_RCLONE:
        cases.extend([
            ('real rclone local remote and genuine corrupt-file Retry', corruption_contract,
             dict(stub=False, inherited_size_only=True)),
            ('inherited filters cannot omit selected content', inherited_filter,
             dict(stub=False, inherited={'RCLONE_EXCLUDE': '*'})),
            ('inherited minimum size cannot omit selected content', inherited_filter,
             dict(stub=False, inherited={'RCLONE_MIN_SIZE': '16M'})),
            ('inherited depth cannot omit selected descendants', inherited_filter,
             dict(stub=False, inherited={'RCLONE_MAX_DEPTH': '1'})),
            ('selected empty directory and descendants are delivered', empty_directories,
             dict(stub=False)),
            ('unknown backend failure carries real rclone stderr', unknown_backend, dict(stub=False)),
            ('env-only remote with a stderr notice still reads capabilities', env_only_remote,
             dict(stub=False, inherited={'RCLONE_CONFIG_GDRIVE_TYPE': 'local'})),
        ])
        cases.extend(edge_cases(REAL_RCLONE, STUB))
    failures = []
    for name, test, options in cases:
        fixture = Fixture(**options)
        try:
            test(fixture)
            print('PASS ' + name, flush=True)
        except Exception as error:
            failures.append(name)
            print('FAIL ' + name + ': ' + str(error), flush=True)
        finally:
            fixture.close()
    if not REAL_RCLONE:
        print('SKIP real rclone: command -v rclone found no executable')
    assert not failures, ', '.join(failures)


if __name__ == '__main__':
    main()
