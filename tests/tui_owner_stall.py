"""A held Owner stat must time out without wedging later rows; early release is the control."""
import os
from pathlib import Path
import pwd
import subprocess
import sys
import time
from tui_pty import Session, fixture

binary, root = Path(sys.argv[1]).resolve(), Path(sys.argv[2]).resolve()
phase = sys.argv[3]
assert phase in ('control', 'deadline', 'recovery')
work, env = fixture(root)
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

with Session(binary, root, work, env) as terminal:
    wait, send, grid = terminal.wait, terminal.send, terminal.grid
    try:
        wait(lambda rows: '4 items' in rows[30] and 'A' in rows[2])
        settle = time.monotonic()
        wait(lambda rows: time.monotonic() - settle >= 0.5)
        arm.touch()
        send(b'III')
        wait(lambda rows: entered.exists())
        print('Owner worker entered the held stat on the simulated NFS mount', flush=True)
        if phase == 'recovery':
            started = time.monotonic()
            expected = pwd.getpwuid(os.getuid()).pw_name
            wait(lambda rows: time.monotonic() - started >= 6)
            rows = grid()
            assert all(expected in row[60:77] for row in rows[2:4]), rows[2:6]
            assert all(not row[60:77].strip() for row in rows[4:6]), rows[2:6]
            Path(str(release) + '.stat').touch()
            wait(lambda rows: Path(str(release) + '.returned').exists())
            wait(lambda rows: all(expected in row[60:77] for row in rows[2:6]), timeout=40)
            print(f'PASS recovery: held rows recovered after {time.monotonic() - started:.3f}s without navigation or relisting', flush=True)
        else:
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
        terminal.finish()
        print(f'PASS {phase}: late stat cannot replace healthy Owner, clean terminal restoration', flush=True)
    finally:
        Path(str(release) + '.stat').touch()
