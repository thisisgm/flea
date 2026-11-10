"""Bounded process cleanup using retained identities and kernel process handles."""
import ctypes
import os
from pathlib import Path
import signal
import time


def identity(pid):
    fields = Path(f'/proc/{int(pid)}/stat').read_text().rsplit(') ', 1)[1].split()
    return dict(state=fields[0], parent=int(fields[1]), group=int(fields[2]), session=int(fields[3]), start=fields[19])


def exited(pid, start=None):
    # A start names one process: a different start means the pid was reused, so the one asked about is gone.
    try:
        current = identity(pid)
    except (FileNotFoundError, ProcessLookupError):
        return True
    return current['state'] == 'Z' or (start is not None and current['start'] != start)


def signal_if_matches(pid, start, signum, *, group=None, verify=None):
    try:
        fd = os.pidfd_open(pid)
    except ProcessLookupError:
        return False
    try:
        try:
            current = identity(pid)
        except (FileNotFoundError, ProcessLookupError):
            return False
        if current['start'] != str(start) or current['state'] == 'Z':
            return False
        if group is not None and (current['group'] != group or current['session'] != group):
            return False
        if verify is not None:
            try:
                checked = verify(pid)
            except (FileNotFoundError, ProcessLookupError):
                return False
            if checked['start'] != current['start'] or checked['state'] == 'Z':
                return False
        try:
            signal.pidfd_send_signal(fd, signum)
            return True
        except ProcessLookupError:
            return False
    finally:
        os.close(fd)


def subreaper():
    libc = ctypes.CDLL(None, use_errno=True)
    if libc.prctl(36, 1, 0, 0, 0) != 0:
        error = ctypes.get_errno()
        raise OSError(error, os.strerror(error))


def reap_descendants(record):
    for process in Path('/proc').iterdir():
        if not process.name.isdecimal() or int(process.name) == record['group']:
            continue
        try:
            current = identity(process.name)
            if current['group'] == record['group'] and current['parent'] == os.getpid() and current['state'] == 'Z':
                os.waitpid(int(process.name), os.WNOHANG)
        except (FileNotFoundError, ProcessLookupError, ChildProcessError):
            continue


def drain(child, record, members, verify, *, terminate, grace, kill_grace=5, on_forcing=None, whole_group=False):
    forced = False
    def send(pid, signum):
        nonlocal forced
        current = verify(pid)
        forced |= signal_if_matches(pid, current['start'], signum, group=record['group'], verify=verify)
    if terminate:
        pending = members()
        targets = pending if whole_group else [pid for pid in pending if pid == record['pid']]
        for pid in targets:
            try:
                send(pid, signal.SIGTERM)
            # ESRCH from the ownership read means the target exited between the two reads.
            except (FileNotFoundError, ProcessLookupError):
                continue
    deadline = time.monotonic() + grace
    phase = "term" if terminate else "natural"
    while True:
        reap_descendants(record)
        pending = members()
        if not pending:
            reap_descendants(record)
            return dict(returncode=child.wait(timeout=kill_grace), forced=forced)
        if time.monotonic() >= deadline:
            if phase == 'kill':
                raise TimeoutError('owned process group survived SIGKILL deadline')
            if phase == 'natural' and on_forcing is not None:
                on_forcing()
            signum = signal.SIGTERM if phase == 'natural' else signal.SIGKILL
            for pid in pending:
                try:
                    send(pid, signum)
                except (FileNotFoundError, ProcessLookupError):
                    continue
            phase = 'term' if phase == 'natural' else 'kill'
            deadline = time.monotonic() + (grace if phase == 'term' else kill_grace)
        time.sleep(0.02)
