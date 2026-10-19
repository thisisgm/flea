"""Controlled OS boundaries for deterministic relay lifecycle regressions."""
from contextlib import ExitStack
import errno
import inspect
import json
import os
from pathlib import Path
import signal
import struct
import sys
import termios
from unittest.mock import patch

from . import cells, relay
from .evidence import MARKER

MASTER_FD = 7000
CHILD_PID = 9000
RESIZED = struct.pack("HHHH", 1, 1, 10, 10)  # the geometry the winsize double reports after the resize fires


class Client:
    def __init__(self):
        self.closed = False

    def setblocking(self, *_):
        pass

    def close(self):
        self.closed = True


class Server:
    def __init__(self, fail=False):
        self.closed = False
        self.fail = fail
        self.clients = []

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

    def accept(self):
        self.clients.append(Client())
        return self.clients[-1], None

    def close(self):
        self.closed = True
        if self.fail:
            raise OSError("server close refused")


def run_relay(case, scenario):
    (case / MARKER).write_text("controlled relay case\n")
    spec = case / "relay-spec.json"
    spec.write_text(json.dumps({"argv": ["candidate"], "environment": {}}))
    if scenario == "start-failure":
        (case / "relay-start.json").write_text("exclusive-create collision\n")
    server_fails = scenario in ("server-close-failure", "two-close-failure")
    server, handlers, closed, logs, forwarded, restored, killed = Server(server_fails), {}, [], [], [], [], []
    # The close scenarios accept two clients on the first poll, so the teardown has client sockets to close.
    accepts = [2] if scenario.endswith("close-failure") else []
    installed, geometry, reaps, winch_restored = set(), [], [], None
    # The teardown case ends on master EOF with the child still running, so the shutdown path must reap it.
    output = [b"first", b"FINAL\x1b[?1049l\x1b[?25h"] if scenario == "drain" else [b""] if scenario == "teardown" else ["議".encode()]
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
        nonlocal winch_restored
        closed.append(fd)
        if fd != MASTER_FD:
            return original_close(fd)
        # The state at the first master close: a resize handler still installed here is the ordering defect.
        if winch_restored is None:
            winch_restored = handlers.get(signal.SIGWINCH, signal.SIG_DFL) == signal.SIG_DFL
        if scenario == "resize-close" and callable(handlers.get(signal.SIGWINCH)):
            handlers[signal.SIGWINCH]()
        if scenario in ("master-close-failure", "two-close-failure"):
            raise OSError("master close refused")

    def ioctl(fd, request=None, data=None, *_):
        if fd == MASTER_FD and fd in closed:
            raise OSError(errno.EBADF, "Bad file descriptor")
        if fd == MASTER_FD and request == termios.TIOCSWINSZ:
            geometry.append(data)

    def install(sig, handler):
        previous = handlers.get(sig, signal.SIG_DFL)
        handlers[sig] = handler
        if handler != signal.SIG_DFL:
            installed.add(signal.Signals(sig).name)
        return previous

    def waitpid(*_):
        # Each call records whether the stop signals are still routed and how many resizes have reached the PTY.
        call = len(reaps)
        exited = not (scenario == "teardown" and call < 2 or scenario == "resize-feed" and call < 1)
        reaps.append({"torn_down": bool(killed), "stop_routed": all(callable(handlers.get(sig)) for sig in (
                          signal.SIGTERM, signal.SIGHUP, signal.SIGINT)),
                      "applied": geometry.count(RESIZED), "exited": exited})
        return (CHILD_PID, 0) if exited else (0, 0)

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
               (relay.os, "waitpid", waitpid),
               (relay.os, "killpg", lambda *args: killed.append(args)),
               (relay.termios, "tcgetattr", lambda *_: [1, 2, 3]),
               (relay.termios, "tcsetattr", lambda *args: restored.append(args)),
               (relay.tty, "setraw", lambda *_: None), (relay.signal, "signal", install),
               (relay.fcntl, "ioctl", ioctl),
               (relay, "winsize", lambda *_: [1, 1, 10, 10] if fired else [6, 20, 200, 60]),
               (relay, "write_all", lambda fd, data: forwarded.append(data)),
               (relay.select, "select", lambda *_: ([server] * (accepts.pop() if accepts else 0)
                                                    + ([MASTER_FD] if output else []), [], [])),
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
    # The loop's own reap of the exited child, before any teardown kill, must find every resize already applied.
    reaped = next((r for r in reaps if r["exited"]), None)
    applied = geometry.count(RESIZED)
    result = dict(error=error, code=code, forwarded=b"".join(forwarded), unread=list(output),
                  restored=restored, killed=killed, server_closed=server.closed,
                  master_closed=MASTER_FD in closed, logs_closed=all(stream.closed for stream in logs),
                  logs_opened=len(logs), clients_closed=[client.closed for client in server.clients],
                  handlers_restored=all(handler == signal.SIG_DFL for handler in handlers.values()), injected=fired,
                  handlers_installed=sorted(installed), winch_restored_at_master_close=winch_restored,
                  stop_routed_during_teardown=bool(reaps) and all(r["stop_routed"] for r in reaps if r["torn_down"])
                  and any(r["torn_down"] for r in reaps),
                  applied_before_reap=bool(reaped) and not reaped["torn_down"] and applied > 0
                  and reaped["applied"] == applied)
    result["events"] = [json.loads(line) for line in (case / "pty.log").read_text().splitlines()]
    for stream in logs:
        stream.close()
    return result
