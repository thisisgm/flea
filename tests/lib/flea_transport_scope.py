"""Bind a graphical scope's endpoint to the exact candidate process it launched."""
import json
import os
from pathlib import Path
import stat
import sys

from flea_transport_lifecycle import identity


def scoped(wrap):
    return bool(wrap and Path(wrap[0]).name == 'systemd-run' and '--scope' in wrap)


def launch(root, wrap, command, environment):
    """Contact the real user manager while preserving the child's isolated session."""
    bus = Path(f'/run/user/{os.getuid()}/bus')
    info = bus.lstat()
    if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid():
        raise ValueError('graphical scope requires the owned user bus')
    child_environment = [f'--setenv={key}={environment[key]}'
                         for key in ('XDG_RUNTIME_DIR', 'DBUS_SESSION_BUS_ADDRESS') if key in environment]
    manager_environment = dict(environment, XDG_RUNTIME_DIR=str(bus.parent),
                               DBUS_SESSION_BUS_ADDRESS=f'unix:path={bus}')
    helper = str(Path(__file__).resolve())
    return [*wrap, *child_environment, sys.executable, '-B', helper, str(root), *command], manager_environment


def admit(pid, record):
    """A sibling or descendant is never a substitute for the receipted executable."""
    path = Path(record['root']) / 'scope-launch.json'
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('unsafe graphical scope launch receipt')
    receipt = json.loads(path.read_text())
    current = identity(pid)
    if (receipt['pid'], receipt['start'], receipt['binary']) != (pid, current['start'], record['binary']):
        raise ValueError('graphical scope peer does not match the candidate launch receipt')
    executable = Path(f'/proc/{pid}/exe').resolve(strict=True)
    if executable not in (Path(record['binary']).resolve(), Path(record['gui']).resolve()):
        raise ValueError('graphical scope peer is not the candidate executable')
    return current


def main():
    root = Path(sys.argv[1])
    command = sys.argv[2:]
    info = root.lstat()
    if root.is_symlink() or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise ValueError('unsafe graphical scope root')
    if not (root / '.flea-test-sandbox').is_file() or str(root) != os.environ.get('FLEA_TEST_WINDOW'):
        raise ValueError('graphical scope root is not this launch')
    options = json.loads((root / 'options.json').read_text())
    if not command or command[0] != options['binary']:
        raise ValueError('graphical scope command is not the admitted candidate')
    current = identity(os.getpid())
    previous = os.umask(0o077)
    try:
        with (root / 'scope-launch.json').open('x') as handle:
            json.dump(dict(pid=os.getpid(), start=current['start'], binary=command[0]), handle)
    finally:
        os.umask(previous)
    os.execv(command[0], command)


if __name__ == '__main__':
    main()
