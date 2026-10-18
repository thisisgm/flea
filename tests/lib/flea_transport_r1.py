"""Lifecycle and QML compatibility probes, also runnable against the saved r1 sources."""
import argparse
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time

REPO = Path(__file__).resolve().parents[2]
parser = argparse.ArgumentParser()
parser.add_argument('--module-dir', type=Path, default=Path(__file__).parent)
parser.add_argument('--old', action='store_true')
args = parser.parse_args()
sys.path.insert(0, str(args.module_dir))
from flea_transport import Window, TransportError, identity

parent = REPO / '.superpowers/g010-transport'
parent.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix='r1.', dir=parent))
(root / '.flea-test-sandbox').touch()
(root / 'target/debug').mkdir(parents=True)
fixture = r'''#!/usr/bin/env python3
import json, os, signal, socket, subprocess, sys, time
from pathlib import Path
root = Path(os.environ['FLEA_TEST_WINDOW'])
mode = os.environ.get('PROBE_MODE', 'normal')
def remember(label, pid=None):
    pid = os.getpid() if pid is None else pid
    fields = Path(f'/proc/{pid}/stat').read_text().rsplit(') ', 1)[1].split()
    (root / label).write_text(json.dumps([pid, fields[19]]))
remember('process')
remember('supervisor', os.getppid())
if mode == 'startup-ignore':
    signal.signal(signal.SIGTERM, signal.SIG_IGN)
    subprocess.Popen([sys.executable, '-c', "import os,signal,time; from pathlib import Path; signal.signal(signal.SIGTERM,signal.SIG_IGN); p=Path(os.environ['FLEA_TEST_WINDOW']); s=Path('/proc/self/stat').read_text().rsplit(') ',1)[1].split(); (p/'descendant').write_text(__import__('json').dumps([os.getpid(),s[19]])); time.sleep(120)"])
    time.sleep(120)
    sys.exit()
def terminate(*_):
    (root / 'forced').touch()
    sys.exit(0)
signal.signal(signal.SIGTERM, terminate)
directory = Path(os.environ['XDG_RUNTIME_DIR']) / 'flea-ipc'
directory.mkdir(mode=0o700)
path = directory / f'{os.getpid()}.sock'
server = socket.socket(socket.AF_UNIX)
server.bind(str(path))
os.chmod(path, 0o600)
server.listen()
while True:
    connection, _ = server.accept()
    with connection:
        data = connection.recv(65536)
        if not data:
            continue
        request = json.loads(data)
        quit = request.get('call', ['', ''])[1] == 'quit'
        reply = dict(code=0, stdout='' if quit else 'owned\n', stderr='')
        connection.sendall((json.dumps(reply) + '\n').encode())
        if quit and mode != 'quit-ignore':
            (root / 'natural').touch()
            break
'''
fixture = fixture.replace(r'\\n', r'\n')
for name in ('flea', 'flea-gui'):
    path = root / 'target/debug' / name
    path.write_text(fixture)
    path.chmod(0o700)
os.environ.update(CARGO_TARGET_DIR=str(root / 'target'), FLEA_TEST_DRAIN_TIMEOUT='0.3')
for key in ('FLEA_BIN', 'FLEA_TEST_WINDOW', 'FLEA_TEST_WINDOWS_ROOT', 'FLEA_TEST_OWNER_PID'):
    os.environ.pop(key, None)


def cleanup_pid(pid, start):
    try:
        fd = os.pidfd_open(pid)
        try:
            if identity(pid)['start'] == start:
                signal.pidfd_send_signal(fd, signal.SIGKILL)
        finally:
            os.close(fd)
    except (FileNotFoundError, ProcessLookupError):
        pass


def start(mode='normal', frontend='gpui'):
    os.environ['PROBE_MODE'] = mode
    case = Path(tempfile.mkdtemp(prefix='case.', dir=root))
    (case / '.flea-test-sandbox').touch()
    return Window.start(root, parent=case, frontend=frontend, timeout=1)


def startup():
    os.environ['PROBE_MODE'] = 'startup-ignore'
    case = Path(tempfile.mkdtemp(prefix='startup.', dir=root))
    (case / '.flea-test-sandbox').touch()
    try:
        try:
            Window.start(root, parent=case, frontend='gpui', timeout=1)
            raise AssertionError('startup failure accepted')
        except TransportError as error:
            assert 'socket' in str(error) and 'timed out' in str(error), error
        window_root, = case.glob('w.*')
        processes = [json.loads((window_root / name).read_text()) for name in ('process', 'descendant', 'supervisor')]
        absent = all(not Path(f'/proc/{pid}').exists() for pid, _ in processes)
        retry = True
        try:
            Window(window_root).stop(timeout=2)
            Window(window_root).stop(timeout=2)
        except TransportError:
            retry = False
        print(f'R1 STARTUP reaped={absent} retry={retry}', flush=True)
        assert absent and retry, 'startup left owned processes or poisoned cleanup retry'
    finally:
        for file in case.glob('w.*/*'):
            if file.name in ('process', 'descendant', 'supervisor'):
                cleanup_pid(*json.loads(file.read_text()))


def quit_probe():
    twin = start()
    try:
        twin.quit()
        assert (twin.root / 'natural').exists() and not (twin.root / 'forced').exists()
        print('R1 TWIN natural-quit exit=0', flush=True)
    finally:
        twin.stop()
    mutant = start('quit-ignore')
    try:
        refused = False
        try:
            mutant.quit()
        except TransportError:
            refused = True
        forced = (mutant.root / 'forced').exists()
        print(f'R1 QUIT ignored-refused={refused} forced={forced}', flush=True)
        assert refused and forced, 'acknowledged quit accepted forced termination'
    finally:
        mutant.stop()


def qml():
    stub = root / 'clients'
    stub.mkdir()
    qs = stub / 'qs'
    qs.write_text(r'''#!/usr/bin/env python3
import json, os, sys
if sys.argv[1:2] == ['list']:
    rows = [dict(id='qml-instance-id', pid=int(os.environ['PROBE_QML_PID']), shell_id='flea', config_path='/qml/boot/shell.qml', generation='keep')]
    print(json.dumps(rows) if '--json' in sys.argv else 'Instance qml-instance-id:\n  Process ID: ' + os.environ['PROBE_QML_PID'] + '\n  Shell ID: flea\n  Config path: /qml/boot/shell.qml')
else:
    print('owned')
''')
    qs.chmod(0o700)
    os.environ['PATH'] = str(stub) + ':' + os.environ['PATH']
    window = start(frontend='qml')
    os.environ['PROBE_QML_PID'] = str(window.pid)
    environment = dict(os.environ, FLEA_TEST_WINDOWS_ROOT=str(window.root.parent))
    cli = str(args.module_dir / 'flea_transport_cli.py')
    try:
        comparisons = []
        for flags in (['--all', '--json'], ['--all']):
            direct = subprocess.run(['qs', 'list', *flags], capture_output=True, check=True)
            adapted = subprocess.run([sys.executable, '-B', cli, 'list', '--', *flags], env=environment, capture_output=True)
            comparisons.append((direct.returncode, direct.stdout, direct.stderr) == (adapted.returncode, adapted.stdout, adapted.stderr))
        print(f'R1 QML list-bytes={all(comparisons)}', flush=True)
        for selector in ('-i', '--id'):
            direct = subprocess.run(['qs', 'ipc', selector, 'qml-instance-id', 'call', 'flea', 'path'], capture_output=True, check=True)
            adapted = subprocess.run([sys.executable, '-B', cli, 'route', '--', selector, 'qml-instance-id', 'call', 'flea', 'path'], env=environment, capture_output=True)
            equal = (direct.returncode, direct.stdout, direct.stderr) == (adapted.returncode, adapted.stdout, adapted.stderr)
            comparisons.append(equal)
            print(f'R1 QML selector={selector} status={adapted.returncode} bytes={equal}', flush=True)
        assert all(comparisons), 'QML list or instance ID compatibility changed'
        print('R1 QML list-bytes=equal ids=stable selectors=ok', flush=True)
    finally:
        window.stop()


def xvfb():
    child = subprocess.Popen([sys.executable, '-c', 'import time; time.sleep(120)'], start_new_session=True)
    start = identity(child.pid)['start']
    try:
        if args.old:
            source = (args.module_dir / 'native-transport-gpui.sh').read_text()
            body = re.search(r"trap '([^']+)' EXIT", source).group(1)
            def guarded(expected):
                script = 'kill() { printf "SIGNAL\\n"; }; wait() { :; }; server=$1; server_start=$2; ' + body
                result = subprocess.run(['bash', '-c', script, 'probe', str(child.pid), expected], capture_output=True, check=True)
                return b'SIGNAL' in result.stdout
        else:
            from unittest.mock import patch
            import flea_transport_xvfb as supervisor
            def guarded(expected):
                case = Path(tempfile.mkdtemp(prefix='xvfb.', dir=root))
                (case / '.flea-test-sandbox').touch()
                sent = []
                original_signal = supervisor.signal_if_matches
                class Server:
                    pid = child.pid
                    def wait(self, timeout):
                        if expected == start:
                            return child.wait(timeout=timeout)
                        return 0
                def launch(command, *, stdout, **kwargs):
                    stdout.write(b'99\n')
                    stdout.flush()
                    return Server()
                def pinned_signal(pid, recorded, signum, **kwargs):
                    result = original_signal(pid, recorded, signum, **kwargs)
                    sent.append(result)
                    return result
                recorded = dict(identity(child.pid), start=expected)
                with patch.object(supervisor.subprocess, 'Popen', launch), \
                     patch.object(supervisor.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0)), \
                     patch.object(supervisor, 'identity', return_value=recorded), \
                     patch.object(supervisor, 'signal_if_matches', pinned_signal), \
                     patch.object(sys, 'argv', ['xvfb-probe', str(case), 'test-body']):
                    assert supervisor.main() == 0
                assert sent, 'Xvfb supervisor skipped its identity guard'
                return any(sent)
        mismatch = guarded(str(int(start) + 1))
        assert not mismatch and child.poll() is None, 'Xvfb teardown signalled reused PID'
        print('R1 XVFB reused-pid=refused survivor=alive', flush=True)
        assert guarded(start), 'owned Xvfb teardown refused matching start time'
        print('R1 TWIN xvfb-matching-start exit=0', flush=True)
    finally:
        cleanup_pid(child.pid, start)
        child.wait(timeout=3)


failed = 0
for name, probe in (('startup', startup), ('quit', quit_probe), ('qml', qml), ('xvfb', xvfb)):
    try:
        probe()
        print(f'R1_PROBE {name} PASS', flush=True)
    except Exception as error:
        failed += 1
        print(f'R1_PROBE {name} REJECTED {error}', flush=True)
print(f'TRANSPORT_R1 checks=4 failed={failed}', flush=True)
sys.exit(1 if failed else 0)
