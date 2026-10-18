"""A held Owner stat must time out without wedging later rows; early release is the control."""
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
phase = sys.argv[3]
assert phase in ('control', 'deadline')
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
healthy = root / 'healthy'
healthy.mkdir()
(healthy / 'healthy.txt').write_text('healthy')
library = root / 'owner-barrier.so'
subprocess.run(['cc', '-shared', '-fPIC', '-o', str(library), 'tests/tui_owner_stall.c', '-ldl'], check=True)
arm, entered, release = root / 'arm', root / 'entered', root / 'release'
mounts = root / 'mounts'
mounts.write_text(f'1 0 8:1 / / rw - ext4 /dev/review rw\n30 1 0:45 / {work} rw - nfs review:/s rw\n')
env.update(LD_PRELOAD=str(library), FLEA_OWNER_TEST_STAT=str(work / 'large.txt'),
           FLEA_OWNER_TEST_ARM=str(arm), FLEA_OWNER_TEST_ENTERED=str(entered),
           FLEA_OWNER_TEST_RELEASE=str(release), FLEA_OWNER_TEST_MOUNTS=str(mounts))
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
    settle = time.monotonic()
    wait(lambda rows: time.monotonic() - settle >= 0.5)
    arm.touch()
    send(b'III')
    wait(lambda rows: entered.exists())
    print('Owner worker entered the held stat on the simulated NFS mount', flush=True)
    send(b':' + str(healthy).encode() + b'\r')
    rows = wait(lambda rows: '1 item' in rows[30] and 'healthy.txt' in rows[2])
    assert not rows[2][60:77].strip(), rows[2]
    started = time.monotonic()
    expected = pwd.getpwuid(os.getuid()).pw_name
    if phase == 'control':
        wait(lambda rows: time.monotonic() - started >= 1.0)
        assert not grid()[2][60:77].strip(), 'held Owner resolved before release control'
        Path(str(release) + '.stat').touch()
        wait(lambda rows: expected in rows[2][60:77])
        print('CONTROL: releasing only the old stat restores the healthy file Owner', flush=True)
    else:
        wait(lambda rows: expected in rows[2][60:77], timeout=6.3)
        assert not Path(str(release) + '.stat').exists(), 'stat was released before deadline proof'
        print(f'PASS deadline: healthy local Owner recovered after {time.monotonic() - started:.3f}s while old stat stayed held', flush=True)
        send(b'I')
        wait(lambda rows: 'Detail column · Size' in rows[30])
        send(b'III')
        wait(lambda rows: 'Detail column · Owner' in rows[30] and expected in rows[2][60:77])
        Path(str(release) + '.stat').touch()
    wait(lambda rows: Path(str(release) + '.returned').exists())
    send(b'I')
    wait(lambda rows: 'Detail column · Size' in rows[30])
    send(b'III')
    wait(lambda rows: 'Detail column · Owner' in rows[30] and expected in rows[2][60:77])
    send(b'q')
    child.wait(timeout=8)
    assert child.returncode == 0
    assert termios.tcgetattr(slave) == saved
    print(f'PASS {phase}: late stat cannot replace healthy Owner, clean terminal restoration', flush=True)
except BaseException:
    print('Last complete TUI grid:', repr(last_grid), flush=True)
    raise
finally:
    Path(str(release) + '.stat').touch()
    if child.poll() is None:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait(timeout=3)
    os.close(master)
    os.close(slave)
    (root / 'pty.log').write_bytes(data)
