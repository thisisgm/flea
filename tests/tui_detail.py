"""Behavioral PTY evidence for detail modes, calendar facts, sorting and clean terminal restoration."""
import fcntl
import os
from pathlib import Path
import pwd
import pty
import re
import select
import signal
import struct
import subprocess
import sys
import termios
import time

binary, root = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
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
    os.utime(item, (1790158470, 1790158470))
    item.chmod(0o755 if item.is_dir() else 0o644)
env = dict(PATH='/usr/bin:/bin', HOME=str(root / 'home'), XDG_CONFIG_HOME=str(root / 'config'),
           XDG_CACHE_HOME=str(root / 'cache'), XDG_STATE_HOME=str(root / 'state'),
           XDG_RUNTIME_DIR=str(root / 'runtime'), LANG='C.UTF-8', TERM='xterm-256color', TZ='UTC0',
           FLEA_TUI_INPUT_TIMEOUT_MS='30', FLEA_TUI_TEST_NO_ICONS='1')
master, slave = pty.openpty()
saved = termios.tcgetattr(slave)
fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack('HHHH', 31, 130, 0, 0))


def controlling_terminal():
    os.setsid()
    fcntl.ioctl(0, termios.TIOCSCTTY, 0)


child = subprocess.Popen([str(binary), '--tui', str(work)], env=env, stdin=slave, stdout=slave,
                         stderr=slave, preexec_fn=controlling_terminal)
data = b''
last_grid = [' ' * 130 for _ in range(31)]


def grid():
    global last_grid
    frame = data.rsplit(b'\x1b[H', 1)[-1].decode('utf-8', errors='replace')
    cells = [[' '] * 130 for _ in range(31)]
    x = y = 0
    footer = set()
    for match in re.finditer(r'\x1b\[[0-?]*[ -/]*[@-~]|[^\x1b]', frame):
        token = match.group()
        if token.startswith('\x1b['):
            if token.endswith('H'):
                parts = token[2:-1].split(';')
                y = int(parts[0] or 1) - 1
                x = int(parts[1] or 1) - 1 if len(parts) > 1 else 0
        elif token >= ' ' and 0 <= y < 31 and 0 <= x < 130:
            cells[y][x] = token
            if y == 30:
                footer.add(x)
            x += 1
    if len(footer) == 130:
        last_grid = [''.join(line) for line in cells]
    return last_grid


def wait(predicate, timeout=8):
    global data
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        screen = grid()
        if predicate(screen):
            return screen
        assert child.poll() is None, (child.returncode, data[-2000:])
        if select.select([master], [], [], 0.03)[0]:
            data += os.read(master, 262144)
    raise AssertionError(('TUI state timed out', grid()))


def send(keys):
    os.write(master, keys)


try:
    wait(lambda rows: '4 items' in rows[30] and 'A' in rows[2])
    start = time.monotonic()
    send(b'I')
    rows = wait(lambda rows: 'Detail column · Modified' in rows[30])
    assert all(row[61:77] == '2026-09-23 10:14' for row in rows[2:6]), rows[2:6]
    assert 'Name ▴' in rows[0]
    actual_data = data
    data += b'\x1b[H\x1b[1;1Hincomplete'
    assert 'Detail column' in grid()[30], 'a partial frame erased a proven receipt'
    data = actual_data
    wait(lambda rows: 'Detail column' not in rows[30])
    elapsed = time.monotonic() - start
    assert 3.7 <= elapsed <= 8, elapsed
    print(f'ok calendar rows and four-second receipt, observed {elapsed:.3f}s')
    send(b'I')
    rows = wait(lambda rows: 'Detail column · Mode' in rows[30])
    assert rows[2][73:77] == '0755' and rows[4][73:77] == '0644', rows[2:6]
    send(b'I')
    expected_owner = pwd.getpwuid(os.getuid()).pw_name
    wait(lambda rows: 'Detail column · Owner' in rows[30] and all(expected_owner in row[60:77] for row in rows[2:6]))
    send(b'I')
    wait(lambda rows: 'Detail column · Size' in rows[30] and '5 B' in rows[5])
    send(b's')
    rows = wait(lambda rows: 'Size ▴' in rows[0] and 'small.txt' in rows[4])
    assert rows[2][31:32] == 'A' and rows[3][31:32] == 'Z', rows[2:6]
    assert 'small.txt' in rows[4] and 'large.txt' in rows[5]
    send(b'S')
    rows = wait(lambda rows: 'Size ▾' in rows[0] and 'large.txt' in rows[4])
    assert rows[2][31:32] == 'A' and rows[3][31:32] == 'Z', rows[2:6]
    send(b'i')
    wait(lambda rows: any('Modified · 2026-09-23 10:14' in row for row in rows))
    send(b'\x1b')
    wait(lambda rows: not any('─ properties ' in row for row in rows))
    send(b'q')
    child.wait(timeout=8)
    while select.select([master], [], [], 0.03)[0]:
        try:
            data += os.read(master, 262144)
        except OSError:
            break
    assert child.returncode == 0, child.returncode
    assert termios.tcgetattr(slave) == saved, 'termios changed after quit'
    assert b'\x1b[?1049l' in data, 'alternate screen was not restored'
    print('PASS tui-detail: four modes, local dates, properties, folder-name size sorting, clean TTY quit')
except BaseException:
    print('Last complete TUI grid:', repr(last_grid), flush=True)
    raise
finally:
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=3)
    os.close(master)
    os.close(slave)
    (root / 'pty.log').write_bytes(data)
