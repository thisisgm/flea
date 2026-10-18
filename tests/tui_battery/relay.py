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
import struct
import sys
import termios
import time
import tty

from .cells import Screen
from .evidence import Owned, save
from .protocol import GraphicsNegative, query_ids

MAX_LOG = 64 * 1024 * 1024


def write_all(fd, data):
    while data:
        count = os.write(fd, data)
        if count <= 0:
            raise RuntimeError("terminal write made no progress")
        data = data[count:]


def winsize(fd):
    return list(struct.unpack("HHHH", fcntl.ioctl(fd, termios.TIOCGWINSZ, bytes(8))))


def serve(spec_path):
    spec = json.loads(Path(spec_path).read_text())
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
        os.read(start_read, 1)
        os.close(start_read)
        fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", *size))
        stderr = os.open(owner.child("candidate.stderr"), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        os.dup2(stderr, 2)
        os.close(stderr)
        argv = list(spec["argv"])
        if argv[0] == "bwrap":
            index = argv.index("--chdir")
            forwarded = []
            for name in ("TERM", "COLORTERM", "TERM_PROGRAM", "TMUX"):
                if name in os.environ:
                    forwarded += ["--setenv", name, os.environ[name]]
            argv[index:index] = forwarded
        os.execvpe(argv[0], argv, spec["environment"])
    os.close(start_read)
    status, stopping, total = None, False, 0
    negative = GraphicsNegative() if spec.get("graphics_negative") else None
    output_tail = b""
    original_child_mode = termios.tcgetattr(master)
    os.write(start_write, b"1")
    os.close(start_write)
    pending_since = None
    clients = {}
    log = owner.child("pty.log").open("x")
    graphics = owner.child("graphics.log").open("x")
    def event(direction, data=b"", **extra):
        nonlocal total
        total += len(data)
        if total > MAX_LOG:
            raise RuntimeError("PTY evidence exceeded 64 MiB budget")
        log.write(json.dumps(dict(ns=time.time_ns(), direction=direction,
                                  base64=base64.b64encode(data).decode(), **extra)) + "\n")
        log.flush()
    def resized(*_):
        current = winsize(0)
        fcntl.ioctl(master, termios.TIOCSWINSZ, struct.pack("HHHH", *current))
        screen.resize(current[1], current[0])
        event("resize", geometry=current)
    def stop(*_):
        nonlocal stopping
        stopping = True
    signal.signal(signal.SIGWINCH, resized)
    for sig in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT):
        signal.signal(sig, stop)
    tty.setraw(0)
    save(case / "relay-start.json", {"pid": pid, "pty": size, "stdin_tty": os.isatty(0), "stdout_tty": os.isatty(1)})
    try:
        while not stopping:
            ready, _, _ = select.select([0, master, server, *clients], [], [], .05)
            for fd in ready:
                if fd == server:
                    client, _ = server.accept()
                    client.setblocking(False)
                    clients[client] = b""
                elif fd == master:
                    try:
                        data = os.read(master, 65536)
                    except OSError:
                        data = b""
                    if not data:
                        stopping = True
                        break
                    event("output", data)
                    write_all(1, data)
                    if negative:
                        output_tail = (output_tail + data)[-65536:]
                        negative.ids.update(query_ids(output_tail))
                    screen.feed(data)
                    for chunk in screen.graphics:
                        graphics.write(json.dumps({"ns": time.time_ns(), "data": chunk}) + "\n")
                    screen.graphics.clear()
                    graphics.flush()
                elif fd == 0:
                    data = os.read(0, 65536)
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
                    data = fd.recv(1024 * 1024)
                    if not data:
                        fd.close()
                        del clients[fd]
                        continue
                    clients[fd] += data
                    if len(clients[fd]) > 1024 * 1024:
                        raise RuntimeError("relay command exceeded budget")
                    if b"\n" not in clients[fd]:
                        continue
                    raw, clients[fd] = clients[fd].split(b"\n", 1)
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
                    fd.settimeout(2)
                    fd.sendall(json.dumps(answer).encode() + b"\n")
                    fd.setblocking(False)
            if negative and pending_since is not None and time.monotonic() - pending_since > .15:
                tail = negative.flush()
                event("candidate-input", tail, incomplete_reply_flushed=True)
                write_all(master, tail)
                pending_since = None
            done, child_status = os.waitpid(pid, os.WNOHANG)
            if done:
                status = os.waitstatus_to_exitcode(child_status)
                stopping = True
    finally:
        if status is None:
            try:
                os.killpg(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            deadline = time.monotonic() + 3
            while time.monotonic() < deadline:
                done, child_status = os.waitpid(pid, os.WNOHANG)
                if done:
                    status = os.waitstatus_to_exitcode(child_status)
                    break
                select.select([], [], [], .02)
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
        os.close(master)
        log.close()
        graphics.close()
        server.close()
        for client in clients:
            client.close()
    return status if status >= 0 else 128 - status


def rpc(case, request, timeout=2):
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
            part = client.recv(65536)
            if not part:
                raise RuntimeError("relay closed without an answer")
            chunks.extend(part)
            if len(chunks) > 8 * 1024 * 1024:
                raise RuntimeError("relay answer exceeds budget")
        return json.loads(chunks)


if __name__ == "__main__":
    sys.exit(serve(sys.argv[1]))
