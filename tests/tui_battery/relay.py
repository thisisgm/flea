"""Native terminal relay: a controlling PTY with byte-exact response forwarding."""
import base64
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import socket
import stat
import struct
import sys
import termios
import time
import tty

from .cells import Screen
from .evidence import Owned, save
from .protocol import GraphicsNegative, query_ids

MAX_LOG = 64 * 1024 * 1024
READ_BYTES = 64 * 1024
TAIL_BYTES = 64 * 1024
COMMAND_BYTES = 1024 * 1024
RPC_ANSWER_BYTES = 8 * 1024 * 1024
POLL_SECONDS = .05
FLUSH_SECONDS = .15
ANSWER_SECONDS = 2
SHUTDOWN_SECONDS = 3
SHUTDOWN_POLL_SECONDS = .02
# The terminal facts a candidate reads from its environment; TMUX also names the one socket its tmux client dials.
FORWARDED = ("TERM", "COLORTERM", "TERM_PROGRAM", "TMUX")


def write_all(fd, data):
    while data:
        count = os.write(fd, data)
        if count <= 0:
            raise RuntimeError("terminal write made no progress")
        data = data[count:]


def winsize(fd):
    return list(struct.unpack("HHHH", fcntl.ioctl(fd, termios.TIOCGWINSZ, bytes(8))))


def sandboxed(argv, environ):
    """The candidate argv; a bwrap one gains this terminal's variables and, under tmux, its server socket bound read-only."""
    argv = list(argv)
    if argv[0] != "bwrap":
        return argv
    forwarded = []
    for name in FORWARDED:
        if name in environ:
            forwarded += ["--setenv", name, environ[name]]
    # Sample input: TMUX=/run/user/1000/tmux-1000/default,4242,0, the server's socket, its pid and the session index.
    path = environ.get("TMUX", "").split(",")[0]
    if path:
        try:
            listening = os.path.isabs(path) and stat.S_ISSOCK(os.stat(path).st_mode)
        except OSError:
            listening = False
        if not listening:
            raise RuntimeError("TMUX names no tmux server socket: " + path)
        forwarded += ["--ro-bind", path, path]
    index = argv.index("--chdir")
    return argv[:index] + forwarded + argv[index:]


def serve(spec_path):
    spec = json.loads(Path(spec_path).read_text())
    argv = sandboxed(spec["argv"], os.environ)
    case = Path(spec_path).parent
    owner = Owned(case)
    control_path = owner.child("relay.sock")
    server = socket.socket(socket.AF_UNIX)
    directory_fd = os.open(case, os.O_RDONLY | os.O_DIRECTORY)
    try:
        server.bind(f"/proc/self/fd/{directory_fd}/relay.sock")
    finally:
        os.close(directory_fd)
    os.chmod(control_path, 0o600)
    server.listen(4)
    server.setblocking(False)
    saved = termios.tcgetattr(0)
    size = winsize(0)
    rows, columns = size[:2]
    screen = Screen(columns, rows)
    start_read, start_write = os.pipe()
    pid, master = pty.fork()
    if pid == 0:
        os.close(start_write)
        if os.read(start_read, 1) != b"1":
            os._exit(1)
        os.close(start_read)
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", *size))
        stderr = os.open(owner.child("candidate.stderr"), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.dup2(stderr, 2)
        os.close(stderr)
        os.execvpe(argv[0], argv, spec["environment"])
    os.close(start_read)
    status, stopping, total = None, False, 0
    negative = GraphicsNegative() if spec.get("graphics_negative") else None
    output_tail = b""
    original_child_mode = None
    pending_since = None
    clients = {}
    resize_pending = False
    log = graphics = None
    start_open = True
    previous_handlers = {}

    def event(direction, data=b"", **extra):
        nonlocal total
        total += len(data)
        if total > MAX_LOG:
            raise RuntimeError("PTY evidence exceeded 64 MiB budget")
        log.write(json.dumps(dict(ns=time.time_ns(), direction=direction,
                                  base64=base64.b64encode(data).decode(), **extra)) + "\n")
        log.flush()

    def relay_output(data):
        nonlocal output_tail
        event("output", data)
        write_all(1, data)
        if negative:
            output_tail = (output_tail + data)[-TAIL_BYTES:]
            negative.ids.update(query_ids(output_tail))
        screen.feed(data)
        for chunk in screen.graphics:
            graphics.write(json.dumps({"ns": time.time_ns(), "data": chunk}) + "\n")
        screen.graphics.clear()
        graphics.flush()

    def apply_resize():
        nonlocal resize_pending
        resize_pending = False
        current = winsize(0)
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", *current))
        screen.resize(current[1], current[0])
        event("resize", geometry=current)

    def resized(*_):
        # Signal context only sets a flag; the loop applies the geometry between feeds.
        nonlocal resize_pending
        resize_pending = True

    def stop(*_):
        nonlocal stopping
        stopping = True

    try:
        original_child_mode = termios.tcgetattr(master)
        log = owner.child("pty.log").open("x")
        graphics = owner.child("graphics.log").open("x")
        previous_handlers[signal.SIGWINCH] = signal.signal(signal.SIGWINCH, resized)
        for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
            previous_handlers[sig] = signal.signal(sig, stop)
        tty.setraw(0)
        os.write(start_write, b"1")
        os.close(start_write)
        start_open = False
        save(case / "relay-start.json", {"pid": pid, "pty": size, "stdin_tty": os.isatty(0), "stdout_tty": os.isatty(1)})
        while not stopping:
            if resize_pending:
                apply_resize()
            ready, _, _ = select.select([0, master, server, *clients], [], [], POLL_SECONDS)
            for fd in ready:
                if fd == server:
                    client, _ = server.accept()
                    client.setblocking(False)
                    clients[client] = b""
                elif fd == master:
                    try:
                        data = os.read(master, READ_BYTES)
                    except OSError:
                        data = b""
                    if not data:
                        stopping = True
                        break
                    relay_output(data)
                elif fd == 0:
                    data = os.read(0, READ_BYTES)
                    if not data:
                        stopping = True
                        break
                    event("terminal-input", data)
                    if negative:
                        data = negative.feed(data)
                        pending_since = time.monotonic() if negative.pending else None
                    if data:
                        event("candidate-input", data)
                        write_all(master, data)
                else:
                    data = fd.recv(COMMAND_BYTES)
                    if not data:
                        fd.close()
                        del clients[fd]
                        continue
                    clients[fd] += data
                    if len(clients[fd]) > COMMAND_BYTES:
                        raise RuntimeError("relay command exceeded budget")
                    if b"\n" not in clients[fd]:
                        continue
                    raw, clients[fd] = clients[fd].split(b"\n", 1)
                    # Sample input: {"op": "input", "base64": "YWI="} as one JSON line.
                    request = json.loads(raw)
                    if request["op"] == "input":
                        incoming = base64.b64decode(request["base64"], validate=True)
                        event("setup-input", incoming)
                        write_all(master, incoming)
                    elif request["op"] == "stop":
                        os.killpg(pid, signal.SIGTERM)
                    elif request["op"] != "snapshot":
                        raise RuntimeError("unknown relay command")
                    answer = dict(screen.snapshot(), pty=winsize(master))
                    fd.settimeout(ANSWER_SECONDS)
                    fd.sendall(json.dumps(answer).encode() + b"\n")
                    fd.setblocking(False)
            if negative and pending_since is not None and time.monotonic() - pending_since > FLUSH_SECONDS:
                tail = negative.flush()
                event("candidate-input", tail, incomplete_reply_flushed=True)
                write_all(master, tail)
                pending_since = None
            done, child_status = os.waitpid(pid, os.WNOHANG)
            if done:
                status = os.waitstatus_to_exitcode(child_status)
                stopping = True
        if status is not None:
            # Output queued before the child exited still reaches the log, the screen and the terminal.
            deadline = time.monotonic() + SHUTDOWN_SECONDS
            while time.monotonic() < deadline and select.select([master], [], [], 0)[0]:
                try:
                    data = os.read(master, READ_BYTES)
                except OSError:
                    break
                if not data:
                    break
                relay_output(data)
        if resize_pending:
            # A resize that arrived during the last feed still reaches the PTY and the log.
            apply_resize()
    finally:
        if start_open:
            os.close(start_write)
        try:
            if status is None:
                try:
                    os.killpg(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                deadline = time.monotonic() + SHUTDOWN_SECONDS
                while time.monotonic() < deadline:
                    done, child_status = os.waitpid(pid, os.WNOHANG)
                    if done:
                        status = os.waitstatus_to_exitcode(child_status)
                        break
                    select.select([], [], [], SHUTDOWN_POLL_SECONDS)
                if status is None:
                    os.killpg(pid, signal.SIGKILL)
                    _, child_status = os.waitpid(pid, 0)
                    status = os.waitstatus_to_exitcode(child_status)
            restored = termios.tcgetattr(master) == original_child_mode
            termios.tcsetattr(0, termios.TCSANOW, saved)
            save(case / "relay-exit.json", {"candidate_exit": status, "termios_restored": restored,
                 "relay_termios_restored": termios.tcgetattr(0) == saved, "screen": screen.snapshot()})
            if negative:
                save(case / "dropped-responses.json", {"forced_fallback": True, "base64": negative.dropped})
        finally:
            # SIGWINCH goes back before the master closes; stop signals stay on stop() past the reap, restore and receipts.
            if signal.SIGWINCH in previous_handlers:
                signal.signal(signal.SIGWINCH, previous_handlers.pop(signal.SIGWINCH))
            failure = None
            try:
                for close in (lambda: os.close(master), *(stream.close for stream in (log, graphics) if stream),
                              server.close, *(client.close for client in clients)):
                    # Every close runs even after one raises; the first error is raised once the handlers are back.
                    try:
                        close()
                    except Exception as error:
                        failure = failure or error
            finally:
                for sig, handler in previous_handlers.items():
                    signal.signal(sig, handler)
            if failure is not None:
                raise failure
    return status if status >= 0 else 128 - status


def rpc(case, request, timeout=ANSWER_SECONDS):
    with socket.socket(socket.AF_UNIX) as client:
        client.settimeout(timeout)
        directory_fd = os.open(case, os.O_RDONLY | os.O_DIRECTORY)
        try:
            client.connect(f"/proc/self/fd/{directory_fd}/relay.sock")
        finally:
            os.close(directory_fd)
        client.sendall(json.dumps(request).encode() + b"\n")
        chunks = bytearray()
        while b"\n" not in chunks:
            part = client.recv(READ_BYTES)
            if not part:
                raise RuntimeError("relay closed without an answer")
            chunks.extend(part)
            if len(chunks) > RPC_ANSWER_BYTES:
                raise RuntimeError("relay answer exceeds budget")
        # Sample input: {"columns": 2, "rows": 1, "grid": [[{"ch": "A"}, {"ch": " "}]], "pty": [24, 80]} as one JSON line.
        return json.loads(chunks)


if __name__ == "__main__":
    sys.exit(serve(sys.argv[1]))
