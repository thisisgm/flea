"""Real CLI and controlling-PTY proof; synthetic input is separate from native terminal emission."""
import atexit
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import struct
import subprocess
import sys
import termios
import time

binary = str(Path(sys.argv[1]).resolve())
root = Path(sys.argv[2]).resolve()
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
sessions = []


def close_sessions():
    for session in sessions:
        if not session.closed:
            session.abort()


atexit.register(close_sessions)


def environment(name):
    case = root / name
    case.mkdir()
    for name in ['home', 'state', 'config', 'cache', 'runtime', 'bin', 'fixture']:
        (case / name).mkdir()
    fixture = case / 'fixture'
    (fixture / 'enter-here').mkdir()
    (fixture / 'enter-here' / 'inside-file.txt').write_text('inside\n')
    (fixture / 'start-file.txt').write_text('original\n')
    env = dict(PATH=str(case / 'bin') + ':/usr/local/bin:/usr/bin:/bin', HOME=str(case / 'home'),
               XDG_STATE_HOME=str(case / 'state'), XDG_CONFIG_HOME=str(case / 'config'),
               XDG_CACHE_HOME=str(case / 'cache'), XDG_RUNTIME_DIR=str(case / 'runtime'),
               TERM='xterm-256color', LANG='C.UTF-8', FLEA_TUI_INPUT_TIMEOUT_MS='50', T2_ROOT=str(case))
    return case, fixture, env


class Session:
    def __init__(self, fixture, env, *args):
        self.master, self.slave = pty.openpty()
        self.saved = termios.tcgetattr(self.slave)
        self.saved_stty = subprocess.check_output(['stty', '-g'], stdin=self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', 31, 130, 0, 0))
        def controlling_terminal():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)
        self.child = subprocess.Popen([binary, '--tui', *map(str, args), str(fixture)], env=env,
                                      stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                      preexec_fn=controlling_terminal)
        self.output = b''
        self.closed = False
        sessions.append(self)
        self.wait(b'start-file.txt')

    def read(self):
        if select.select([self.master], [], [], 0.02)[0]:
            try:
                self.output += os.read(self.master, 262144)
            except OSError:
                pass

    def wait(self, needle, start=0, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            self.read()
            if needle in self.output[start:]:
                return
            if self.child.poll() is not None:
                break
        self.abort()
        raise AssertionError(f'missing {needle!r}: {self.output[-4000:]!r}')

    def send(self, data):
        start = len(self.output)
        os.write(self.master, data)
        return start

    def finish(self, expected=0):
        deadline = time.monotonic() + 8
        while self.child.poll() is None and time.monotonic() < deadline:
            self.read()
        if self.child.poll() is None:
            self.abort()
            raise AssertionError('TUI did not quit')
        self.read()
        assert self.child.returncode == expected, (self.child.returncode, self.output[-4000:])
        assert termios.tcgetattr(self.slave) == self.saved, 'termios was not restored'
        assert b'\x1b[?1049l' in self.output, 'alternate screen was not restored'
        os.close(self.master)
        os.close(self.slave)
        self.closed = True

    def abort(self):
        if self.child.poll() is None:
            os.killpg(self.child.pid, signal.SIGKILL)
            self.child.wait(timeout=3)
        if not self.closed:
            os.close(self.master)
            os.close(self.slave)
            self.closed = True


case, fixture, env = environment('arguments')
for args in [['--tui', '--cwd-file'], ['--tui', '--chooser-file'], ['--tui', '--cwd-file', '--gui'],
             ['--tui', '--tui-keymap', 'invalid'], ['--cwd-file', str(case / 'out')],
             ['--tui', '--cwd-file', ''], ['--tui', '--cwd-file', b'bad\xff'],
             ['--tui', '--cwd-file', str(case / 'out')]]:
    result = subprocess.run([binary, *args], env=env, capture_output=True, timeout=8)
    assert result.returncode == 2, (args, result.stderr)
    assert result.stdout == b''
    assert not (case / 'out').exists()
result = subprocess.run([binary], env=env, capture_output=True, timeout=8)
assert result.returncode == 2 and b'graphical session' in result.stderr
print('ok arguments, non-UTF8 refusal, no-TTY refusal, GUI default, stdout')

case, fixture, env = environment('navigation')
s = Session(fixture, env, '--cwd-file', case / 'cwd')
start = s.send(b'\r')
s.wait(b'1 item', start)
s.wait(b'inside-file.txt', start)
start = s.send(b'\x0c' + str(case / 'missing-folder').encode() + b'\r')
s.wait(b'item not found', start)
s.send(b'q')
s.finish()
assert (case / 'cwd').read_text() == str(fixture / 'enter-here') + '\n'
print('ok cwd after successful and failed navigation')

case, fixture, env = environment('tab-close')
s = Session(fixture, env, '--cwd-file', case / 'cwd')
start = s.send(b'O')
s.wait(b'1 item', start)
s.wait(b'inside-file.txt', start)
start = s.send(b'\x17')
s.wait(b'2 items', start)
s.wait(b'start-file.txt', start)
s.send(b'w')
s.finish()
assert (case / 'cwd').read_text() == str(fixture) + '\n'
print('ok cwd after closing another tab and last tab')

for mode in ['confirm', 'cancel', 'multi', 'signal']:
    case, fixture, env = environment('chooser-' + mode)
    output = case / 'chosen'
    output.write_text('old selection')
    s = Session(fixture, env, '--chooser-file', output)
    if mode == 'confirm':
        s.send(b'j\r')
        s.finish()
        assert output.read_text() == str(fixture / 'start-file.txt') + '\n'
    elif mode == 'multi':
        s.send(b'vjv\r')
        s.finish()
        assert output.read_text() == str(fixture / 'enter-here') + '\n' + str(fixture / 'start-file.txt') + '\n'
    elif mode == 'cancel':
        s.send(b'q')
        s.finish(1)
        assert output.read_text() == 'old selection'
    else:
        os.kill(s.child.pid, signal.SIGTERM)
        s.finish(130)
        assert output.read_text() == 'old selection'
print('ok chooser confirmation, listing order, cancellation and signal preservation')

case, fixture, env = environment('write-refusal')
s = Session(fixture, env, '--cwd-file', case / 'missing' / 'out')
s.send(b'q')
s.finish(2)
assert b'flea: terminal interface:' in s.output
print('ok output write failure exits 2 after terminal restoration')

def check_editor_guard(tool, fixture, env):
    master, slave = pty.openpty()
    try:
        for first, second, expected in [('--wait', '--', 0), ('WRONG', '--', 1), ('--wait', 'WRONG', 1)]:
            target = fixture / '.argv-control'
            target.write_text('original\n')
            result = subprocess.run([str(tool), first, second, str(target)], env=env,
                                    stdin=slave, capture_output=True, timeout=8)
            assert (result.returncode == 0) == (expected == 0), (first, second, result.returncode)
            assert target.read_text() == ('edited\n' if expected == 0 else 'original\n')
    finally:
        os.close(master)
        os.close(slave)


for mode in ['editor', 'visual', 'failure', 'missing']:
    case, fixture, env = environment('edit-' + mode)
    tool = case / 'bin' / 'editor with spaces'
    tool.write_text('''#!/bin/sh
set -eu
[ "$1" = --wait ] || exit 4
[ "$2" = -- ] || exit 4
case "$T2_ROOT" in /*) ;; *) exit 9 ;; esac
[ -f "$T2_ROOT/../.flea-test-sandbox" ] || exit 9
case "$3" in "$T2_ROOT"/fixture/*) ;; *) exit 9 ;; esac
stty -g > "$T2_ROOT/editor-termios"
printf '%s\\n' "$3" > "$T2_ROOT/editor-path"
printf 'EDITOR_READY\\n'
if [ "$T2_EDITOR_FAIL" = 1 ]; then exit 7; fi
printf 'edited\\n' > "$3"
''')
    tool.chmod(0o700)
    env['T2_EDITOR_FAIL'] = '1' if mode == 'failure' else '0'
    if mode == 'editor':
        check_editor_guard(tool, fixture, env)
    if mode != 'missing':
        env['VISUAL' if mode == 'visual' else 'EDITOR'] = "'" + str(tool) + "' --wait"
    s = Session(fixture, env)
    s.send(b'j')
    start = s.send(b'E')
    if mode == 'missing':
        s.wait(b'Set $EDITOR or $VISUAL', start)
    else:
        s.wait(b'EDITOR_READY', start)
        restored = s.output.index(b'EDITOR_READY', start) + len(b'EDITOR_READY')
        s.wait(b'\x1b[?1049h', restored)
        s.wait(b'could not finish' if mode == 'failure' else b'start-file.txt', start)
        assert (case / 'editor-path').read_text() == str(fixture / 'start-file.txt') + '\n'
        assert (case / 'editor-termios').read_bytes() == s.saved_stty, 'editor did not receive original termios'
        assert (fixture / 'start-file.txt').read_text() == ('original\n' if mode == 'failure' else 'edited\n')
    s.send(b'q')
    s.finish()
    if mode != 'missing':
        assert s.output.count(b'\x1b[?1049h') == 2
        assert s.output.count(b'\x1b[?1049l') == 2
print('ok E uses argv, EDITOR/VISUAL, reports errors, restores terminal and rereads')

case, fixture, env = environment('agent')
tool = case / 'bin' / 'omarchy'
tool.write_text('''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
os.write(1, b'AGENT_STDOUT_SENTINEL\\n')
os.write(2, b'AGENT_STDERR_SENTINEL\\n')
data = dict(cwd=os.getcwd(), argv=sys.argv[1:], pid=os.getpid(), group=os.getpgrp(),
            fds=[os.readlink('/proc/self/fd/' + str(fd)) for fd in (0, 1, 2)])
root = Path(os.environ['T2_ROOT'])
(root / 'agent-result.tmp').write_text(json.dumps(data))
(root / 'agent-result.tmp').rename(root / 'agent-result')
''')
tool.chmod(0o700)
s = Session(fixture, env)
s.send(b'A')
result_file = case / 'agent-result'
deadline = time.monotonic() + 8
while not result_file.exists() and time.monotonic() < deadline:
    s.read()
assert result_file.exists(), 'agent did not report its launch'
result = json.loads(result_file.read_text())
s.read()
assert result['cwd'] == str(fixture) and result['argv'] == ['agent', '--pick'], result
assert result['fds'] == ['/dev/null'] * 3, result
assert result['pid'] == result['group'], result
assert b'AGENT_STDOUT_SENTINEL' not in s.output and b'AGENT_STDERR_SENTINEL' not in s.output
s.send(b'q')
s.finish()
print('ok A executes omarchy agent in the shown folder')
