"""Controlled OS boundaries for deterministic relay lifecycle regressions."""
from contextlib import ExitStack
import errno
import inspect
import json
import os
from pathlib import Path
import signal
import sys
from unittest.mock import patch

from . import cells, relay
from .evidence import MARKER

MASTER_FD = 7000
CHILD_PID = 9000


class Server:
    def __init__(self):
        self.closed = False

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def bind(self, *_):
        pass

    def listen(self, *_):
        pass

    def setblocking(self, *_):
        pass

    def close(self):
        self.closed = True


def run_relay(case, scenario):
    (case / MARKER).write_text("controlled relay case\n")
    spec = case / "relay-spec.json"
    spec.write_text(json.dumps({"argv": ["candidate"], "environment": {}}))
    if scenario == "start-failure":
        (case / "relay-start.json").write_text("exclusive-create collision\n")
    server, handlers, closed, logs, forwarded, restored, killed = Server(), {}, [], [], [], [], []
    output = [b"first", b"FINAL\x1b[?1049l\x1b[?25h"] if scenario == "drain" else ["議".encode()]
    fired = False
    original_close, original_open = os.close, Path.open
    source, first_line = inspect.getsourcelines(cells.Screen.feed)
    wide_write = first_line + next(i for i, line in enumerate(source) if "self.grid[self.y][self.x + 1]" in line)

    def trace(frame, event, arg):
        nonlocal fired
        if not fired and event == "line" and frame.f_code is cells.Screen.feed.__code__ and frame.f_lineno == wide_write:
            fired = True
            handlers[signal.SIGWINCH]()
        return trace

    def close(fd):
        closed.append(fd)
        if fd != MASTER_FD:
            return original_close(fd)
        if scenario == "resize-close" and callable(handlers.get(signal.SIGWINCH)):
            handlers[signal.SIGWINCH]()

    def ioctl(fd, *_):
        if fd == MASTER_FD and fd in closed:
            raise OSError(errno.EBADF, "Bad file descriptor")

    def install(sig, handler):
        previous = handlers.get(sig, signal.SIG_DFL)
        handlers[sig] = handler
        return previous

    def open_file(path, *args, **kwargs):
        if scenario == "log-failure" and path.name == "graphics.log" and args and args[0] == "x":
            raise OSError("graphics log creation refused")
        result = original_open(path, *args, **kwargs)
        if path.name in ("pty.log", "graphics.log") and args and args[0] == "x":
            logs.append(result)
        return result

    doubles = [(relay.socket, "socket", lambda *_: server), (relay.os, "chmod", lambda *_: None),
               (relay.pty, "fork", lambda: (CHILD_PID, MASTER_FD)), (relay.os, "close", close),
               (relay.os, "write", lambda fd, data: len(data)),
               (relay.os, "read", lambda *_: output.pop(0) if output else b""),
               (relay.os, "waitpid", lambda *_: (CHILD_PID, 0)),
               (relay.os, "killpg", lambda *args: killed.append(args)),
               (relay.termios, "tcgetattr", lambda *_: [1, 2, 3]),
               (relay.termios, "tcsetattr", lambda *args: restored.append(args)),
               (relay.tty, "setraw", lambda *_: None), (relay.signal, "signal", install),
               (relay.fcntl, "ioctl", ioctl),
               (relay, "winsize", lambda *_: [1, 1, 10, 10] if fired else [6, 20, 200, 60]),
               (relay, "write_all", lambda fd, data: forwarded.append(data)),
               (relay.select, "select", lambda *_: ([MASTER_FD] if output else [], [], [])),
               (Path, "open", open_file)]
    error, code = None, None
    with ExitStack() as stack:
        for target, name, double in doubles:
            stack.enter_context(patch.object(target, name, double))
        if scenario == "resize-feed":
            sys.settrace(trace)
        try:
            code = relay.serve(spec)
        except Exception as caught:
            error = caught
        finally:
            sys.settrace(None)
    result = dict(error=error, code=code, forwarded=b"".join(forwarded), unread=list(output),
                  restored=restored, killed=killed, server_closed=server.closed,
                  master_closed=MASTER_FD in closed, logs_closed=all(stream.closed for stream in logs),
                  handlers_restored=all(handler == signal.SIG_DFL for handler in handlers.values()), injected=fired)
    result["events"] = [json.loads(line) for line in (case / "pty.log").read_text().splitlines()]
    for stream in logs:
        stream.close()
    return result
