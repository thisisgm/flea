"""Shared fixture and detached PTY session for the TUI detail and owner suites."""
import fcntl
import os
import pty
import re
import select
import signal
import struct
import subprocess
import termios
import time

COLUMNS = 130
LINES = 31
READ_BYTES = 262144
POLL_SECONDS = 0.03
FIXTURE_TIME = 1790158470


def fixture(root):
    assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
    for name in ('home', 'config', 'cache', 'state', 'runtime', 'work'):
        (root / name).mkdir()
    (root / 'runtime').chmod(0o700)
    work = root / 'work'
    for name in ('A', 'Z'):
        (work / name).mkdir()
    (work / 'A' / 'large').write_text('x' * 8192)
    (work / 'small.txt').write_text('tiny\n')
    (work / 'large.txt').write_text('large file\n' * 20)
    for item in work.iterdir():
        os.utime(item, (FIXTURE_TIME, FIXTURE_TIME))
        item.chmod(0o755 if item.is_dir() else 0o644)
    env = dict(PATH='/usr/bin:/bin', HOME=str(root / 'home'), XDG_CONFIG_HOME=str(root / 'config'),
               XDG_CACHE_HOME=str(root / 'cache'), XDG_STATE_HOME=str(root / 'state'),
               XDG_RUNTIME_DIR=str(root / 'runtime'), LANG='C.UTF-8', TERM='xterm-256color', TZ='UTC0',
               FLEA_TUI_INPUT_TIMEOUT_MS='30', FLEA_TUI_TEST_NO_ICONS='1')
    return work, env


def controlling_terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


class Session:
    def __init__(self, binary, root, work, env):
        self.root = root
        self.data = b''
        self.last_grid = [' ' * COLUMNS for _ in range(LINES)]
        self.master, self.slave = pty.openpty()
        self.saved = termios.tcgetattr(self.slave)
        fcntl.ioctl(self.slave, termios.TIOCSWINSZ, struct.pack('HHHH', LINES, COLUMNS, 0, 0))
        try:
            self.child = subprocess.Popen([str(binary), '--tui', str(work)], env=env,
                                          stdin=self.slave, stdout=self.slave, stderr=self.slave,
                                          preexec_fn=controlling_terminal)
        except BaseException:
            os.close(self.master)
            os.close(self.slave)
            raise

    def __enter__(self):
        return self

    # Sample input: b'\x1b[H\x1b[1;1Hname\x1b[31;1H' + b' ' * 130.
    def grid(self):
        frame = self.data.rsplit(b'\x1b[H', 1)[-1].decode('utf-8', errors='replace')
        cells = [[' '] * COLUMNS for _ in range(LINES)]
        x = y = 0
        footer = set()
        for match in re.finditer(r'\x1b\[[0-?]*[ -/]*[@-~]|[^\x1b]', frame):
            token = match.group()
            if token.startswith('\x1b['):
                if token.endswith('H'):
                    parts = token[2:-1].split(';')
                    y = int(parts[0] or 1) - 1
                    x = int(parts[1] or 1) - 1 if len(parts) > 1 else 0
            elif token >= ' ' and 0 <= y < LINES and 0 <= x < COLUMNS:
                cells[y][x] = token
                if y == LINES - 1:
                    footer.add(x)
                x += 1
        if len(footer) == COLUMNS:
            self.last_grid = [''.join(line) for line in cells]
        return self.last_grid

    def wait(self, predicate, timeout=8):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            screen = self.grid()
            if predicate(screen):
                return screen
            assert self.child.poll() is None, (self.child.returncode, self.data[-2000:])
            if select.select([self.master], [], [], POLL_SECONDS)[0]:
                self.data += os.read(self.master, READ_BYTES)
        raise AssertionError(('TUI state timed out', self.grid()))

    def send(self, keys):
        os.write(self.master, keys)

    def finish(self):
        self.child.wait(timeout=8)
        while select.select([self.master], [], [], POLL_SECONDS)[0]:
            try:
                chunk = os.read(self.master, READ_BYTES)
            except OSError:
                break
            if not chunk:
                break
            self.data += chunk
        assert self.child.returncode == 0, self.child.returncode
        assert termios.tcgetattr(self.slave) == self.saved, 'termios changed after quit'
        assert b'\x1b[?1049l' in self.data, 'alternate screen was not restored'

    def __exit__(self, error_type, _error, _traceback):
        if error_type is not None:
            print('Last complete TUI grid:', repr(self.last_grid), flush=True)
        if self.child.poll() is None:
            os.killpg(self.child.pid, signal.SIGKILL)
            self.child.wait(timeout=3)
        os.close(self.master)
        os.close(self.slave)
        (self.root / 'pty.log').write_bytes(self.data)
