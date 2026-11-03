"""Exercise jobs and actions through the real JSON-lines backend."""
import json
import os
from pathlib import Path
import select
import shutil
import signal
import subprocess
import sys
import tempfile
import time
from jobs_barrier import CopyBarrier

BINARY = str(Path(sys.argv[1]).resolve())
MARKER = '.flea-test-sandbox'
# Request ids for barrier round trips start above every fixed id the suite sends.
BARRIER_SYNC_FIRST_ID = 10000


class Root:
    def __init__(self, parent):
        self.path = Path(tempfile.mkdtemp(prefix='flea-jobs-', dir=parent)).resolve()
        (self.path / MARKER).write_text('jobs integration sandbox\n')

    def close(self):
        path = self.path
        assert str(path) and path.is_absolute() and (path / MARKER).is_file()
        for folder, dirs, files in os.walk(path):
            os.chmod(folder, 0o700)
        shutil.rmtree(path)


class Backend:
    def __init__(self, root):
        self.root = root
        self.barrier = CopyBarrier(root)
        self.barrier_sync = BARRIER_SYNC_FIRST_ID
        self.cancel_handled = None
        env = dict(os.environ)
        for key, name in [('HOME', 'home'), ('XDG_CONFIG_HOME', 'config'),
                          ('XDG_DATA_HOME', 'data'), ('XDG_STATE_HOME', 'state'),
                          ('XDG_RUNTIME_DIR', 'runtime'), ('TMPDIR', 'tmp')]:
            folder = root / name
            folder.mkdir(exist_ok=True)
            env[key] = str(folder)
        self.actions = root / 'config/flea/actions'
        self.actions.mkdir(parents=True, exist_ok=True)
        env['PATH'] = str(root / 'bin') + ':' + env.get('PATH', '')
        self.barrier.environment(env)
        self.process = subprocess.Popen([BINARY, '--backend'], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
                                        env=env, start_new_session=True)
        self.pending = b''
        self.events = []
        self.jobs = {}

    def send(self, **request):
        self.process.stdin.write((json.dumps(request) + '\n').encode())
        self.process.stdin.flush()

    def read(self, timeout):
        end = time.monotonic() + timeout
        while b'\n' not in self.pending:
            left = end - time.monotonic()
            assert left > 0 and select.select([self.process.stdout], [], [], left)[0], 'backend timeout: ' + repr(self.events[-8:])
            chunk = os.read(self.process.stdout.fileno(), 65536)
            assert chunk, f'backend exited {self.process.poll()}'
            self.pending += chunk
        line, self.pending = self.pending.split(b'\n', 1)
        event = json.loads(line)
        self.events.append(event)
        if event['t'] == 'jobs':
            self.jobs = {job['id']: job for job in event['jobs']}
        elif event['t'] == 'job':
            self.jobs[event['job']['id']] = event['job']
        return event

    def wait(self, predicate, timeout=20):
        end = time.monotonic() + timeout
        while True:
            event = self.read(max(0.001, end - time.monotonic()))
            if predicate(event):
                return event
            assert time.monotonic() < end, 'expected backend event absent'

    def state(self, ident, state):
        if self.jobs.get(ident, {}).get('state') == state:
            return self.jobs[ident]
        self.wait(lambda e: self.jobs.get(ident, {}).get('state') == state)
        return self.jobs[ident]

    def copy(self, paths, destination):
        self.send(c='transfer', op='copy', paths=[str(p) for p in paths], dest=str(destination))
        return self.wait(lambda e: e['t'] == 'transferstarted')['id']

    def held_copy(self, paths, destination):
        before = self.barrier.ready()
        ident = self.copy(paths, destination)
        ready = self.barrier.wait(before)
        self.state(ident, 'running')
        return ident, ready

    def cancel_held(self, ident, ready):
        started = time.monotonic()
        self.send(c='jobs', op='cancel', id=ident)
        self.barrier_sync += 1
        self.send(c='actions', op='list', id=self.barrier_sync, paths=[])
        self.wait(lambda e: e['t'] == 'actions' and e['id'] == self.barrier_sync)
        self.cancel_handled = time.monotonic() - started
        self.barrier.release(ready)
        return self.state(ident, 'done')

    def manifest(self, name, **overrides):
        fields = dict(name=name, mime=['text/*'], selection='many',
                      run=['/usr/bin/true', '{path}'], mode='background')
        fields.update(overrides)
        text = '\n'.join(key + ' = ' + json.dumps(value) for key, value in fields.items())
        (self.actions / (name + '.toml')).write_text(text)
        return 'action:' + name + '.toml'

    def list(self, paths):
        self.send(c='actions', op='list', id=72, paths=[str(p) for p in paths])
        return self.wait(lambda e: e['t'] == 'actions' and e['id'] == 72)

    def action(self, action, paths):
        self.send(c='actions', op='run', id=73, action=action, paths=[str(p) for p in paths])
        event = self.wait(lambda e: e['t'] == 'actionstarted' and e['requestId'] == 73)
        assert event['ok'], event
        return event['id']

    def close(self):
        if self.process.poll() is None:
            self.send(c='quit')
            try:
                self.process.communicate(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(self.process.pid, signal.SIGKILL)
                self.process.wait()


def fixture(root):
    source = root / 'source'
    source.mkdir()
    tree = source / 'tree'
    tree.mkdir()
    (tree / 'hold-copy.txt').write_text('controlled copy read')
    for index in range(8):
        (tree / f'{index:05d}.txt').write_bytes(b'payload' * 32)
    a = source / 'first.txt'
    b = source / 'second.txt'
    a.write_text('first')
    b.write_text('second')
    return tree, a, b


def copies(backend, root, temporary):
    tree, a, b = fixture(root)
    first = root / 'NAS'
    second = root / 'Docs'
    first.mkdir()
    second.mkdir()
    one, held_one = backend.held_copy([tree], first)
    two = backend.copy([tree], second)
    backend.wait(lambda e: backend.jobs.get(two, {}).get('reason', '').startswith('waiting for the copy to '))
    queued = backend.jobs[two]
    target_name = backend.jobs[one]['head'].rsplit(' to ', 1)[1]
    assert queued['reason'] == 'waiting for the copy to ' + target_name, queued
    assert not (second / tree.name).exists(), 'queued copy started writing'
    started = time.monotonic()
    backend.send(c='jobs', op='cancel', id=two)
    assert backend.state(two, 'done')['cancelled']
    waiting = time.monotonic() - started
    assert backend.cancel_held(one, held_one)['cancelled']
    print(f'MEASURE waiting copy Cancel to done {waiting:.4f}s, running copy Cancel handled within '
          f'{backend.cancel_handled:.4f}s (timed to the reply of the request sent after it)')
    assert not (second / tree.name).exists()
    for ident in (one, two):
        backend.send(c='jobs', op='dismiss', id=ident)
        backend.wait(lambda e: e['t'] == 'jobs' and ident not in backend.jobs)
    print('PASS same-drive FIFO, named wait, independent running and waiting Cancel, Dismiss')

    one, held_one = backend.held_copy([tree], second)
    two, held_two = backend.held_copy([tree], temporary)
    backend.wait(lambda e: all(backend.jobs.get(i, {}).get('state') == 'running' for i in (one, two)))
    backend.cancel_held(one, held_one)
    assert not backend.jobs[two]['cancelled']
    backend.cancel_held(two, held_two)
    print('PASS different-drive concurrency, one Cancel preserves the other')

    destination = root / 'retry'
    destination.mkdir()
    (destination / b.name).write_text('keep')
    os.chmod(destination / b.name, 0o400)
    ident = backend.copy([a, b], destination)
    failed = backend.state(ident, 'failed')
    assert failed['reason'] and (destination / a.name).read_text() == 'first'
    assert (destination / b.name).read_text() == 'keep'
    target = destination / b.name
    assert target.is_absolute() and target.is_relative_to(root) and (root / MARKER).is_file()
    target.unlink()
    a.write_text('changed after success')
    backend.send(c='jobs', op='retry', id=ident)
    done = backend.state(ident, 'done')
    assert done['total'] == 1 and done['undo'], done
    assert (destination / a.name).read_text() == 'first', 'retry repeated a successful item'
    assert (destination / b.name).read_text() == 'second'
    backend.send(c='jobs', op='undo', id=ident)
    backend.wait(lambda e: e['t'] == 'undone' and e['ok'])
    assert not (destination / b.name).exists() and (destination / a.name).exists()
    print('PASS failed row, retry failed item only, done Undo')

    readonly = root / 'readonly'
    readonly.mkdir()
    readonly.chmod(0o500)
    backend.send(c='transfer', paths=[str(b)], dest=str(readonly))
    event = backend.wait(lambda e: e['t'] in ('transferstarted', 'error'))
    assert event['t'] == 'transferstarted', event
    backend.state(event['id'], 'failed')
    readonly.chmod(0o700)
    backend.send(c='jobs', op='retry', id=event['id'])
    backend.state(event['id'], 'done')
    assert (readonly / b.name).read_text() == 'second'
    print('PASS read-only destination retains failed job for Retry')
    return a, b


def manifests(backend, root, a, b):
    single = backend.manifest('one', selection='one')
    many = backend.manifest('many')
    image = backend.manifest('image', mime=['image/*'])
    assert {single, many}.issubset({r['id'] for r in backend.list([a])['actions']})
    assert {r['id'] for r in backend.list([a, b])['actions']} == {many}
    photo = root / 'photo.jpg'
    photo.write_bytes(b'not decoded for MIME selection')
    assert {r['id'] for r in backend.list([photo])['actions']} == {image}
    assert not backend.list([a, photo])['actions']
    valid = (backend.actions / 'many.toml').read_text()
    malformed = [valid + '\nname="again"', valid + '\nother="x"',
                 valid.replace('"many"', '"zero"'), valid.replace('"background"', '"shell"'),
                 valid.replace('["text/*"]', '"text/*"'), valid.replace('name = "many"', 'name = []'),
                 valid.replace('"{path}"', '"{unknown}"'), valid.replace('"{path}"', '"{path"'),
                 valid.replace('"{path}"', '"bad\\q"'), valid.replace('"/usr/bin/true"', '"{path}"'),
                 valid.replace('run = ["/usr/bin/true", "{path}"]', 'run = []'),
                 valid.replace('mode = "background"', ''), valid + ' garbage',
                 valid.replace('["text/*"]', '["text/*" "image/*"]')]
    for index, value in enumerate(malformed):
        (backend.actions / f'bad{index}.toml').write_text(value)
    (backend.actions / 'oversize.toml').write_text('x' * 16385)
    fifo = backend.actions / 'fifo.toml'
    os.mkfifo(fifo)
    result = backend.list([a])
    assert len(result['errors']) == len(malformed) + 2, result
    assert {single, many}.issubset({r['id'] for r in result['actions']})
    script = root / 'config/flea/scripts/old-script.sh'
    script.parent.mkdir(parents=True)
    script.write_text('#!/bin/sh\nexit 0\n')
    script.chmod(0o700)
    assert any(r['id'] == 'script:old-script.sh' and r['name'] == 'old-script' for r in backend.list([a])['actions'])
    print(f'PASS manifest valid, {len(malformed)} malformed shapes, oversized file, FIFO, MIME one/many, legacy scripts')


def actions(backend, root, a, b):
    source = root / "- a 'quote' \"double\"\n$(touch SHOULD_NOT_RUN) {stem}.txt"
    source.write_text('content')
    helper = root / 'argv.py'
    output = root / 'argv.json'
    helper.write_text('import json,sys\nopen(sys.argv[1], "w").write(json.dumps(sys.argv[2:]))\n')
    action = backend.manifest('argv', run=['/usr/bin/python3', str(helper), str(output),
                                          '{path}', '{dir}', '{name}', '{stem}', '{ext}'])
    backend.state(backend.action(action, [source]), 'done')
    actual = json.loads(output.read_text())
    assert actual == [str(source), str(root), './' + source.name, './' + source.stem, 'txt'], actual
    print('PASS placeholders preserve spaces, braces and leading dashes without shell or option injection')

    helper = root / 'tail.py'
    helper.write_text('import os\nos.write(1, b"x"*100000 + b"\\n")\nos.write(2,b"last one\\nlast two\\n")\n')
    action = backend.manifest('tail', run=['/usr/bin/python3', str(helper)])
    ident = backend.action(action, [a])
    row = backend.state(ident, 'done')
    assert row['output'] == ['last one', 'last two'], row
    assert all(sum(len(s.encode()) for s in e['job']['output']) <= 8198
               for e in backend.events if e['t'] == 'job' and e['job']['id'] == ident)
    assert row['bytes'] is None and row['done'] == 1
    print('PASS bounded two-line merged output and file-count action progress')

    helper = root / 'ignore.py'
    pid = root / 'child.pid'
    helper.write_text('import os,signal,time,sys\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\n'
                      'child=os.fork()\nif child: open(sys.argv[1],"w").write(str(child))\n'
                      'while True: time.sleep(0.05)\n')
    action = backend.manifest('ignore', run=['/usr/bin/python3', str(helper), str(pid)])
    ident = backend.action(action, [a])
    end = time.monotonic() + 5
    while not pid.exists():
        assert time.monotonic() < end
        time.sleep(0.01)
    child = int(pid.read_text())
    started = time.monotonic()
    backend.send(c='jobs', op='cancel', id=ident)
    assert backend.state(ident, 'done')['cancelled']
    end = time.monotonic() + 3
    while True:
        try:
            stat = Path(f'/proc/{child}/stat').read_text()
        except FileNotFoundError:
            break
        if stat.rsplit(')', 1)[1].split()[0] == 'Z':
            break
        assert time.monotonic() < end, 'SIGTERM-ignoring child survived Cancel'
        time.sleep(0.01)
    print(f'PASS Cancel kills SIGTERM-ignoring process group in {time.monotonic()-started:.3f}s')

    action = backend.manifest('preview', mode='preview', selection='one',
                              run=['/usr/bin/cat', '{path}'])
    backend.send(c='actions', op='preview', id=84, action=action, paths=[str(b)])
    preview = backend.wait(lambda e: e['t'] == 'actionpreview' and e['id'] == 84)
    assert preview['ok'] and preview['text'] == b.read_text(), preview
    backend.send(c='actions', op='preview', id=85, action=action, paths=[str(a), str(b)])
    refused = backend.wait(lambda e: e['t'] == 'actionpreview' and e['id'] == 85)
    assert not refused['ok']
    print('PASS preview action uses sandbox and refuses multiple rows')

    action = backend.manifest('preview-cap', mode='preview', selection='one',
                              run=['/usr/bin/head', '-c', '1048577', '/dev/zero'])
    backend.send(c='actions', op='preview', id=86, action=action, paths=[str(b)])
    refused = backend.wait(lambda e: e['t'] == 'actionpreview' and e['id'] == 86)
    assert not refused['ok'] and refused['error']
    action = backend.manifest('preview-timeout', mode='preview', selection='one',
                              run=['/usr/bin/sleep', '10'])
    started = time.monotonic()
    backend.send(c='actions', op='preview', id=87, action=action, paths=[str(b)])
    refused = backend.wait(lambda e: e['t'] == 'actionpreview' and e['id'] == 87)
    assert not refused['ok'] and time.monotonic() - started < 9
    action = backend.manifest('preview-write', mode='preview', selection='one',
                              run=['/usr/bin/sh', '-c', 'printf forbidden > "$1"', 'sh', '{path}'])
    backend.send(c='actions', op='preview', id=88, action=action, paths=[str(b)])
    refused = backend.wait(lambda e: e['t'] == 'actionpreview' and e['id'] == 88)
    assert not refused['ok'] and b.read_text() == 'second'
    print('PASS preview output cap, wall timeout, and read-only sandbox')

    action = backend.manifest('terminal', mode='terminal', run=['/usr/bin/printf', '{name}'])
    backend.send(c='actions', op='run', id=89, action=action, paths=[str(source)])
    handed = backend.wait(lambda e: e['t'] == 'actionstarted' and e['requestId'] == 89)
    assert handed['ok'] and 'id' not in handed
    argv = terminal_argv(root / 'terminal.json')
    assert argv == ['--dir=' + str(root), '--', '/usr/bin/printf', './' + source.name], argv
    print('PASS terminal mode uses OEM launcher and preserves argv')

    helper = root / 'action-retry.py'
    helper.write_text('from pathlib import Path\nimport sys\np=Path(sys.argv[1])\n'
                      'log=Path(sys.argv[2])\n'
                      'with log.open("a") as stream: stream.write(p.name+"\\n")\n'
                      'sys.exit(1 if p.name=="second.txt" and not Path(sys.argv[3]).exists() else 0)\n')
    action = backend.manifest('action-retry', run=['/usr/bin/python3', str(helper), '{path}',
                                                str(root / 'action-retry.log'), str(root / 'allow-retry')])
    ident = backend.action(action, [a, b])
    backend.state(ident, 'failed')
    (root / 'allow-retry').touch()
    backend.send(c='jobs', op='retry', id=ident)
    row = backend.state(ident, 'done')
    assert row['total'] == 1
    assert (root / 'action-retry.log').read_text().splitlines() == [a.name, b.name, b.name]
    print('PASS action Retry invokes only failed files')



def review_probes(backend, root, a):
    failures = []

    def check(name, run):
        try:
            run()
            print('PASS REVIEW ' + name)
        except AssertionError as error:
            failures.append(name)
            print('REPRO ' + name + ': ' + str(error), flush=True)

    def queued_retry():
        source = root / 'review-source.txt'
        source.write_text('original failed item')
        destination = root / 'review-retry-dest'
        destination.mkdir()
        target = destination / source.name
        target.write_text('occupied')
        target.chmod(0o400)
        backend.send(c='transfer', op='move', paths=[str(source)], dest=str(destination))
        ident = backend.wait(lambda e: e['t'] == 'transferstarted')['id']
        backend.state(ident, 'failed')
        assert target.is_absolute() and target.is_relative_to(root) and (root / MARKER).is_file()
        target.unlink()
        blocker_dest = root / 'review-blocker'
        blocker_dest.mkdir()
        blocker, held = backend.held_copy([root / 'source/tree'], blocker_dest)
        backend.state(blocker, 'running')
        backend.send(c='jobs', op='retry', id=ident)
        backend.wait(lambda e: backend.jobs.get(ident, {}).get('state') == 'waiting'
                     and backend.jobs[ident]['reason'].startswith('waiting for'))
        source.rename(root / 'review-original-held.txt')
        source.write_text('unrelated replacement')
        backend.cancel_held(blocker, held)
        backend.wait(lambda e: backend.jobs.get(ident, {}).get('state') in ('done', 'failed'))
        assert source.exists(), 'replacement source exists=False, destination=' + target.read_text()
        assert source.read_text() == 'unrelated replacement'
        assert not target.exists(), 'queued Retry copied the replacement'
        assert backend.jobs[ident]['state'] == 'failed', backend.jobs[ident]

    def shared_undo():
        source = root / 'review-window-a.txt'
        source.write_text('window A')
        destination = root / 'review-window-a'
        destination.mkdir()
        ident = backend.copy([source], destination)
        assert backend.state(ident, 'done')['undo']
        backend.wait(lambda e: e['t'] == 'transferdone' and e['id'] == ident)
        other = Backend(root)
        try:
            source_b = root / 'review-window-b.txt'
            source_b.write_text('window B')
            dest_b = root / 'review-window-b'
            dest_b.mkdir()
            other_id = other.copy([source_b], dest_b)
            other.state(other_id, 'done')
            other.wait(lambda e: e['t'] == 'transferdone' and e['id'] == other_id)
            backend.send(c='jobs', op='undo', id=ident)
            backend.wait(lambda e: e['t'] in ('undone', 'joberror', 'error'))
            assert (destination / source.name).read_text() == 'window A'
            assert (dest_b / source_b.name).read_text() == 'window B', 'window B copy was changed'
            backend.wait(lambda e: ident in backend.jobs and not backend.jobs[ident]['undo'])
            other.send(c='undo')
            other.wait(lambda e: e['t'] == 'undone' and e['ok'])
            assert not (dest_b / source_b.name).exists(), 'expired job Undo consumed the newer journal entry'
        finally:
            other.close()

    def session_undo():
        source = root / 'review-index-file.txt'
        source.write_text('index probe')
        destination = root / 'review-index'
        destination.mkdir()
        ident = backend.copy([source], destination)
        assert backend.state(ident, 'done')['undo']
        backend.send(c='searchopen', path=str(destination), folder=str(destination))
        backend.wait(lambda e: e['t'] == 'searchopened')
        backend.send(c='searchquery', query=source.name, gen=1, count=100)
        initial = backend.wait(lambda e: e['t'] == 'searched' and not e.get('walking'))
        assert initial['n'] == 1, initial
        backend.send(c='jobs', op='undo', id=ident)
        backend.wait(lambda e: e['t'] == 'undone' and e['ok'])
        assert not (destination / source.name).exists()
        backend.send(c='searchquery', query=source.name, gen=2, count=100)
        after = backend.wait(lambda e: e['t'] == 'searched' and e['gen'] == 2)
        assert after['n'] == 0, 'copy exists=False, search still returns=' + str(after['n'])

    def raw_control():
        ident = backend.manifest('review-control', run=['/usr/bin/true', 'raw' + chr(1) + 'control'])
        path = backend.actions / 'review-control.toml'
        path.write_text(path.read_text().replace('\\u0001', chr(1)))
        reply = backend.list([a])
        assert any(e['file'] == path.name for e in reply['errors']), 'raw 0x01 manifest accepted=' + str(any(r['id'] == ident for r in reply['actions']))

    check('queued Retry refuses an unrelated replacement', queued_retry)
    check('expired job Undo preserves another windows copy', shared_undo)
    check('job Undo refreshes the search index', session_undo)
    check('malformed TOML raw control is refused', raw_control)
    assert not failures, 'review reproducers failed: ' + ', '.join(failures)


# Stub xdg-terminal-exec: the argv lands in a sibling file renamed into place, so log is only ever whole.
def terminal_launcher(launcher, log):
    part = log.with_name(log.name + '.part')
    launcher.parent.mkdir()
    launcher.write_text('#!/usr/bin/python3\nimport json,os,sys\n'
                        f'with open({str(part)!r}, "w") as out:\n'
                        '    out.write(json.dumps(sys.argv[1:]))\n'
                        f'os.replace({str(part)!r}, {str(log)!r})\n')
    launcher.chmod(0o700)


def terminal_argv(log, seconds=5):
    end = time.monotonic() + seconds
    while not log.exists():
        assert time.monotonic() < end, f'the terminal launcher wrote no {log.name} in {seconds}s'
        time.sleep(0.01)
    return json.loads(log.read_text())


def main():
    scratch = Path('.superpowers/b-jobs')
    scratch.mkdir(parents=True, exist_ok=True)
    root = Root(str(scratch.resolve()))
    temporary = Root(tempfile.gettempdir())
    backend = None
    barrier = None
    try:
        assert os.stat(root.path).st_dev != os.stat(temporary.path).st_dev, 'test requires a separate tmpfs'
        barrier = CopyBarrier.build(root.path)
        terminal_launcher(root.path / 'bin/xdg-terminal-exec', root.path / 'terminal.json')
        backend = Backend(root.path)
        a, b = copies(backend, root.path, temporary.path)
        manifests(backend, root.path, a, b)
        actions(backend, root.path, a, b)
        backend.send(c='jobs', op='list')
        snapshot = backend.wait(lambda e: e['t'] == 'jobs')
        assert snapshot['unfinished'] == 0 and snapshot['newest'] is None
        print('PASS final snapshot excludes failed and done jobs from unfinished count')
        review_probes(backend, root.path, a)
    finally:
        if barrier:
            barrier.close()
        if backend:
            backend.close()
        temporary.close()
        root.close()


if __name__ == '__main__':
    main()
