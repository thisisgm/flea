"""Teardown must end every owned window on record, including one whose last process exits mid-check."""
import errno
import json
import os
from pathlib import Path
import signal
import threading
import time
from unittest import mock

import flea_transport
import flea_transport_lifecycle
from flea_transport import MARKER, identity


def check_teardown(root, check):
    # A process that exits between its stat read and its environ read gives ESRCH, not ENOENT: it is gone, not foreign.
    # Sample: [Errno 3] No such process: '/proc/16961/environ', from a native run whose window then never ended.
    group = os.getpgrp()
    real_read_bytes = Path.read_bytes

    def exiting(path):
        if path.name == "environ":
            raise ProcessLookupError(errno.ESRCH, os.strerror(errno.ESRCH), str(path))
        return real_read_bytes(path)

    record = dict(group=group, pid=os.getpid(), root=str(root))
    member = dict(state="S", parent=1, group=group, session=group, start="1")
    with mock.patch.object(flea_transport, "identity", return_value=member), mock.patch.object(Path, "read_bytes", exiting):
        try:
            members = flea_transport.group_members(record)
        except ProcessLookupError as error:
            members = error
    check(members == [], "teardown-member-exiting-mid-check-is-gone")

    sent = []
    def verify(pid):
        raise ProcessLookupError(errno.ESRCH, os.strerror(errno.ESRCH), f"/proc/{pid}/environ")
    child = mock.Mock(wait=mock.Mock(return_value=0))
    pending = iter([[4242], []])
    with mock.patch.object(flea_transport_lifecycle, "signal_if_matches", lambda *a, **k: sent.append(a) or True):
        try:
            outcome = flea_transport_lifecycle.drain(child, dict(group=group, pid=4242), lambda: next(pending), verify,
                                                    terminate=True, grace=1)
        except ProcessLookupError as error:
            outcome = error
    check(isinstance(outcome, dict) and sent == [], "teardown-signal-target-exiting-mid-check-is-skipped")

    # Whatever stops a drain, the supervisor still records its end; a window with no stopped file stays on the books
    # after its processes are gone, and every later enumeration then fails reading its /proc entry.
    window = Path(root) / "teardown-supervisor"
    window.mkdir(mode=0o700)
    (window / MARKER).write_text("owned Flea window\n")
    options = dict(root=str(window), path=str(root), select=None, binary=str(Path(root) / "target/debug/flea"),
                   gui=str(Path(root) / "target/debug/flea-gui"), frontend="gpui", picker=False,
                   ui=str(flea_transport.REPO / "ui"), timeout=5.0, log=str(window / "window.log"), command=None, wrap=[],
                   backend=str(Path(root) / "target/debug/flea"), owner=os.getpid(), drain_timeout=1.0,
                   owner_start=identity(os.getpid())["start"])
    (window / "options.json").write_text(json.dumps(options))
    failure = ProcessLookupError(errno.ESRCH, os.strerror(errno.ESRCH), "/proc/1/environ")
    with mock.patch.object(flea_transport, "drain", side_effect=failure):
        supervisor = threading.Thread(target=flea_transport.serve, args=(window,), daemon=True)
        supervisor.start()
        deadline = time.monotonic() + 10
        while not (window / "window.json").exists() and supervisor.is_alive() and time.monotonic() < deadline:
            time.sleep(0.02)
        (window / "stop").touch()
        supervisor.join(15)
    launched = json.loads((window / "launch.json").read_text()) if (window / "launch.json").exists() else None
    if launched:
        try:
            if identity(launched["pid"])["start"] == launched["start"]:
                os.killpg(launched["group"], signal.SIGKILL)
        except (FileNotFoundError, ProcessLookupError):
            pass
    outcome = json.loads((window / "outcome.json").read_text()) if (window / "outcome.json").exists() else {}
    check(not supervisor.is_alive() and (window / "stopped").exists() and "No such process" in (outcome.get("error") or ""),
          "teardown-supervisor-records-a-failed-drain")


def check_teardown_past_an_unwritable_error(root, check):
    # A failed drain whose error file cannot be written still kills, waits for and reports the window.
    window = Path(root) / "teardown-unwritable"
    window.mkdir(mode=0o700)
    (window / MARKER).write_text("owned Flea window\n")
    options = dict(root=str(window), path=str(root), select=None, binary="/bin/sleep", gui="/bin/sleep",
                   frontend="gpui", picker=False, ui=str(flea_transport.REPO / "ui"), timeout=0.3,
                   log=str(window / "window.log"), command=["/bin/sleep", "60"], wrap=[], backend="/bin/sleep",
                   owner=os.getpid(), drain_timeout=1.0, owner_start=identity(os.getpid())["start"])
    (window / "options.json").write_text(json.dumps(options))
    real_write = Path.write_text
    failing = threading.Event()
    refused = []

    def drain(*_, **__):
        failing.set()
        raise ProcessLookupError(errno.ESRCH, os.strerror(errno.ESRCH), "/proc/1/environ")

    def write(path, *args, **kwargs):
        if failing.is_set() and path.name == "error" and not refused:
            refused.append(path)
            raise OSError(errno.ENOSPC, os.strerror(errno.ENOSPC), str(path))
        return real_write(path, *args, **kwargs)

    killed = []
    real_killpg = os.killpg
    with mock.patch.object(flea_transport, "drain", side_effect=drain), mock.patch.object(Path, "write_text", write), \
            mock.patch.object(flea_transport.os, "killpg", lambda group, sig: killed.append(group) or real_killpg(group, sig)), \
            mock.patch.object(threading, "excepthook", lambda _: None):
        supervisor = threading.Thread(target=flea_transport.serve, args=(window,), daemon=True)
        supervisor.start()
        supervisor.join(15)
    launched = json.loads((window / "launch.json").read_text()) if (window / "launch.json").exists() else None
    if launched and launched["group"] not in killed:
        try:
            real_killpg(launched["group"], signal.SIGKILL)
        except ProcessLookupError:
            pass
    outcome = json.loads((window / "outcome.json").read_text()) if (window / "outcome.json").exists() else {}
    check(bool(launched) and launched["group"] in killed and bool(refused), "teardown-kills-the-group-past-an-unwritable-error")
    check((window / "stopped").exists() and outcome.get("returncode") is not None and (window / "error").exists(),
          "teardown-waits-and-reports-past-an-unwritable-error")
