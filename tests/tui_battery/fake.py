"""Display-free adapter diagnostics; never emits native screenshot evidence."""
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import shlex
import signal
import struct
import sys
import termios
import time
import tty

from .adapters import Adapter, OPTIONS, OWNED_CLASS_PREFIX
from .cells import predicate
from .evidence import Blocked, MARKER, save
from .geometry import COLUMNS, ROWS
from . import relay

FAKE_EOF_EXIT = 92
EXEC_FAILURE_EXIT = 127
DIAGNOSTIC_OUTPUT_BYTES = 65536
OUTPUT_TAIL_CHARS = 300
FAKE_CELL_WIDTH, FAKE_CELL_HEIGHT = 8, 22


def terminal(name, args):
    if args in (["--help"], ["--version"], ["+version"], ["-V"]):
        print("fake terminal diagnostic " + " ".join(OPTIONS[name]))
        return 0
    # A config check answers like a terminal that accepts the config; the battery's tests cover a rejection.
    if args[:1] in (["--check-config"], ["+validate-config"], ["+runpy"]):
        return 0
    if name == "alacritty":
        command = args[args.index("-e") + 1:]
    elif name == "ghostty":
        if "--gtk-single-instance=false" not in args:
            raise RuntimeError("ghostty isolation flag missing")
        command = args[args.index("-e") + 1:]
    elif name in ("foot", "kitty"):
        index = 0
        while index < len(args) and args[index].startswith("--"):
            index += 1 if "=" in args[index] else 2
        command = args[index:]
    else:
        command = args[args.index("new-session") + 3:]
    if not command:
        raise RuntimeError("adapter did not forward its candidate command")
    os.execvpe(command[0], command, os.environ)


def candidate():
    if not os.isatty(0) or not os.isatty(1):
        return 90
    saved = termios.tcgetattr(0)
    tty.setraw(0)
    try:
        os.write(1, b"\x1b[?1049h\x1b[?25l\x1b[HREADY\x1b_Gi=31,a=q;AAAA\x1b\\\x1b[31;1Hfake terminal diagnostic")
        reply = bytearray()
        while not reply.endswith(b"\x1b\\"):
            data = os.read(0, 1)
            if not data:
                return FAKE_EOF_EXIT
            reply.extend(data)
        if reply != b"\x1b_Gi=31;OK\x1b\\":
            return 91
        os.write(1, b"\x1b[2;1HQUERY REPLY RECEIVED")
        while True:
            key = os.read(0, 1)
            if not key:
                return FAKE_EOF_EXIT
            if key == b"?":
                os.write(1, b"\x1b[HKEY RECEIVED")
            elif key == b"q":
                break
    finally:
        os.write(1, b"\x1b[?25h\x1b[?1049l")
        termios.tcsetattr(0, termios.TCSANOW, saved)
    return 0


def exit_blocked(name, status, output):
    return Blocked(f"fake {name} exited {status}: " + output.decode(errors="replace")[-OUTPUT_TAIL_CHARS:])


def adapter_diagnostic(name, case, commands, palette):
    started = time.monotonic()
    (case / MARKER).write_text("fake terminal diagnostic\n")
    fakebin = case / "bin"
    fakebin.mkdir()
    root = str(Path(__file__).resolve().parents[1])
    for executable in {name, "foot", "tmux"}:
        wrapper = fakebin / executable
        wrapper.write_text("#!/bin/sh\nexec " + shlex.join([sys.executable, "-m", "tui_battery.fake", "--terminal", executable]) + ' "$@"\n')
        wrapper.chmod(0o700)
    adapter = Adapter(name, case, OWNED_CLASS_PREFIX + "fake", palette)
    adapter.configure()
    environment = dict(adapter.environment(), PATH=str(fakebin) + ":/usr/bin:/bin", PYTHONPATH=root, PYTHONDONTWRITEBYTECODE="1")
    preflight = adapter.preflight(commands, str(fakebin), environment)
    config_check = adapter.validate(commands, str(fakebin), environment)
    save(case / "relay-spec.json", {"argv": [sys.executable, "-m", "tui_battery.fake", "--candidate"], "environment": environment})
    argv = adapter.argv([sys.executable, "-m", "tui_battery.relay", str(case / "relay-spec.json")])
    pid, master = pty.fork()
    if pid == 0:
        try:
            fcntl.ioctl(0, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLUMNS,
                        COLUMNS * FAKE_CELL_WIDTH, ROWS * FAKE_CELL_HEIGHT))
            os.execvpe(argv[0], argv, environment)
        except OSError as error:
            os.write(2, f"fake {name} could not execute {argv[0]}: {error}\n".encode())
            os._exit(EXEC_FAILURE_EXIT)
    deadline, status = time.monotonic() + 15, None
    output = bytearray()
    found_ready = found_key = sent_reply = sent_key = False
    try:
        while time.monotonic() < deadline:
            ready, _, _ = select.select([master], [], [], .03)
            if ready:
                try:
                    output.extend(os.read(master, DIAGNOSTIC_OUTPUT_BYTES))
                    output = output[-DIAGNOSTIC_OUTPUT_BYTES:]
                except OSError:
                    break
            if not found_key and (case / "relay.sock").exists() and not (case / "relay-exit.json").exists():
                snapshot = relay.rpc(case, {"op": "snapshot"})
                if not found_ready and predicate(snapshot, {"contains": ["READY"]}):
                    found_ready = True
                    os.write(master, b"\x1b_Gi=31;")
                elif found_ready and not sent_reply:
                    os.write(master, b"OK\x1b\\")
                    sent_reply = True
                elif sent_reply and not sent_key and predicate(snapshot, {"contains": ["QUERY REPLY RECEIVED"]}):
                    sent_key = True
                    os.write(master, b"?")
                elif found_ready and not found_key and predicate(snapshot, {"contains": ["KEY RECEIVED"]}):
                    found_key = True
                    save(case / "cells.json", snapshot)
                    os.write(master, b"q")
            done, child_status = os.waitpid(pid, os.WNOHANG)
            if done:
                status = os.waitstatus_to_exitcode(child_status)
                break
        while status is None and time.monotonic() < deadline:
            done, child_status = os.waitpid(pid, os.WNOHANG)
            if done:
                status = os.waitstatus_to_exitcode(child_status)
            else:
                time.sleep(.02)
        if status is None:
            raise Blocked(f"fake {name} did not quit at its deadline")
        if status:
            raise exit_blocked(name, status, output)
        receipt = json.loads((case / "relay-exit.json").read_text())
        if status or not found_key or not receipt["termios_restored"] or not receipt["relay_termios_restored"]:
            raise AssertionError(f"fake {name} failed PTY input/cleanup proof")
        receipt.update(adapter=name, adapter_argv=argv, preflight=preflight, config_check=config_check, fake=True, native_acceptance=False,
                       exit_code=status, elapsed_ms=round((time.monotonic() - started) * 1000, 3))
        return receipt
    finally:
        if status is None:
            try:
                os.killpg(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
        os.close(master)


if __name__ == "__main__":
    if sys.argv[1] == "--terminal":
        sys.exit(terminal(sys.argv[2], sys.argv[3:]))
    if sys.argv[1] == "--candidate":
        sys.exit(candidate())
    raise SystemExit("unknown fake terminal operation")
