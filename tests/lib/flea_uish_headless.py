"""Drive the mounted GPUI test window; authenticate reads through the production IPC transport."""
import argparse
import contextlib
import hashlib
import json
import math
import os
from pathlib import Path
import selectors
import shutil
import signal
import subprocess
import sys
import tempfile
import time

from flea_transport import MARKER, REPO, TransportError, drain_timeout, exchange, group_members, owned, private
from flea_transport_lifecycle import drain, identity, subreaper

MAX_TIMEOUT_S = 30
READ_BYTES = 65536
MAX_REPLY_BYTES = 1024 * 1024
MAX_COMMAND_BYTES = 65536
TEARDOWN_GRACE_S = 3
LOG_TAIL_CHARS = 12000
POLL_S = 0.005
# A listing that settles just after ready moves its generation once or twice; five reads outlast it.
OBSERVE_ATTEMPTS = 5

def deadline_seconds(value):
    value = float(value)
    if not math.isfinite(value) or not 0 < value <= MAX_TIMEOUT_S:
        raise TransportError(f"timeout must be finite and in (0, {MAX_TIMEOUT_S}]")
    return value


class Host:
    def __init__(self, driver, *, timeout=10):
        self.timeout = deadline_seconds(timeout)
        # Read before the spawn, so a bad FLEA_TEST_DRAIN_TIMEOUT fails with no child to reap.
        self.drain_s = drain_timeout()
        driver = Path(driver).resolve(strict=True)
        if not os.access(driver, os.X_OK):
            raise TransportError("headless driver is not executable")
        # Hashed before the spawn, so an execute-only driver fails with no child to reap.
        driver_sha256 = hashlib.sha256(driver.read_bytes()).hexdigest()
        parent = REPO / ".superpowers/m9-uish-02"
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="host.", dir=parent)).resolve()
        (self.root / MARKER).write_text("owned GPUI headless host\n")
        try:
            self.prepare(driver, driver_sha256)
        except BaseException as error:
            self.remove_root(error)
            raise

    def prepare(self, driver, driver_sha256):
        private(self.root)
        environment = dict(PATH=os.environ.get("PATH", "/usr/bin:/bin"),
                           FLEA_UISH_ROOT=str(self.root), FLEA_TEST_WINDOW=str(self.root),
                           RUST_BACKTRACE="1")
        for key, name in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"),
                          ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache"),
                          ("XDG_DATA_HOME", "data"), ("XDG_RUNTIME_DIR", "runtime")):
            directory = self.root / name
            directory.mkdir(mode=0o700)
            environment[key] = str(directory)
        target = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target")).resolve()
        binary = Path(os.environ.get("FLEA_BIN", target / "debug/flea")).resolve(strict=True)
        if not os.access(binary, os.X_OK):
            raise TransportError("build the production flea binary first")
        environment["FLEA_BIN"] = str(binary)
        self.windows = {}
        self.sequence = 0
        self.buffer = bytearray()
        self.source = dict(driver_sha256=driver_sha256, driver=str(driver), backend=str(binary))
        self.log = (self.root / "driver.log").open("wb")
        self.selector = selectors.DefaultSelector()
        try:
            subreaper()
            self.process = subprocess.Popen([str(driver), "--ignored", "--exact", "uish_driver_replay",
                                             "--nocapture", "--test-threads=1"],
                                            env=environment, cwd=REPO, stdin=subprocess.PIPE,
                                            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                            start_new_session=True)
        except BaseException as error:
            self.selector.close()
            self.log.close()
            self.remove_root(error)
            raise
        try:
            current = identity(self.process.pid)
            self.record = dict(root=str(self.root), pid=self.process.pid, group=self.process.pid,
                               start=current["start"], leader_start=current["start"])
            self.selector.register(self.process.stdout, selectors.EVENT_READ)
            hello = self.receive(self.timeout)
            if hello["sequence"] is not None or hello["result"].get("value", {}).get("protocol") != "flea-uish-headless-v1":
                raise TransportError("not a mounted GPUI headless driver")
        except BaseException as error:
            if hasattr(self, "record"):
                self.stop(terminate=True, error=error)
            else:
                self.abandon(error)
            raise

    def abandon(self, error):
        with contextlib.suppress(ProcessLookupError):
            os.killpg(self.process.pid, signal.SIGKILL)
        self.process.wait()
        for stream in (self.process.stdin, self.process.stdout):
            stream.close()
        self.selector.close()
        self.log.close()
        self.remove_root(error)

    def keep_log(self, error):
        # The root and its driver.log go next, so the tail is printed once and travels in the error.
        path = self.root / "driver.log"
        tail = path.read_text(errors="replace")[-LOG_TAIL_CHARS:] if path.is_file() else ""
        if not tail:
            return
        if not getattr(self, "tail_printed", False):
            print(tail, file=sys.stderr)
            self.tail_printed = True
        note = "driver.log tail:\n" + tail
        if error is not None and note not in getattr(error, "__notes__", ()):
            error.add_note(note)

    def remove_root(self, error=None):
        # Only the marked host.* root this host made; the error its caller is handling keeps the log tail first.
        root, parent = self.root, (REPO / ".superpowers/m9-uish-02").resolve()
        if not root.exists():
            return
        if error is not None:
            self.keep_log(error)
        if not (root.is_absolute() and root.parent == parent and root.name.startswith("host.")
                and (root / MARKER).is_file()):
            raise TransportError(f"refusing to remove an unmarked host root: {root}")
        shutil.rmtree(root)

    def verify(self):
        if self.process.poll() is not None:
            raise TransportError("headless driver exited")
        if identity(self.process.pid)["start"] != self.record["start"]:
            raise TransportError("headless driver PID was reused")
        owned(self.process.pid, self.record)

    def receive(self, timeout):
        deadline = time.monotonic() + deadline_seconds(timeout)
        while True:
            # Sample input: b'driver noise UISH\t{"sequence": 1, "result": {"ok": true, "value": {}}}'
            while b"\n" in self.buffer:
                line, _, self.buffer = self.buffer.partition(b"\n")
                if b"UISH\t" in line:
                    return json.loads(line.split(b"UISH\t", 1)[1])
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"headless control timeout; log: {self.root / 'driver.log'}")
            if not self.selector.select(remaining):
                continue
            chunk = os.read(self.process.stdout.fileno(), READ_BYTES)
            if not chunk:
                raise TransportError(f"headless driver exited before reply; log: {self.root / 'driver.log'}")
            self.log.write(chunk)
            self.log.flush()
            self.buffer.extend(chunk)
            if len(self.buffer) > MAX_REPLY_BYTES:
                raise TransportError("oversized headless control reply")

    def send(self, command):
        self.verify()
        self.sequence += 1
        data = (json.dumps(dict(command, sequence=self.sequence)) + "\n").encode()
        if len(data) > MAX_COMMAND_BYTES:
            raise TransportError("oversized headless control command")
        self.process.stdin.write(data)
        self.process.stdin.flush()
        envelope = self.receive(self.timeout)
        if envelope["sequence"] != self.sequence:
            raise TransportError("stale headless control reply")
        if not envelope["result"]["ok"]:
            raise TransportError(envelope["result"]["error"])
        self.last_envelope = envelope
        return envelope["result"]["value"]

    def open(self, path):
        descriptor = self.send(dict(op="open", path=str(Path(path).resolve())))
        if descriptor.get("root_type") != "flea_gui::window::root::Root" or descriptor.get("pid") != self.process.pid:
            raise TransportError("not the owned production Root")
        window = Window(self, descriptor)
        self.windows[window.id] = window
        return window

    def stop(self, *, terminate=False, error=None):
        if self.log.closed:
            return
        if self.process.stdin and not self.process.stdin.closed:
            self.process.stdin.close()
        try:
            # An owned stop waits out the driver draining its windows; a forced stop keeps the short grace.
            result = drain(self.process, self.record, lambda: group_members(self.record),
                           lambda pid: owned(pid, self.record), terminate=terminate,
                           grace=TEARDOWN_GRACE_S if terminate else self.drain_s,
                           whole_group=terminate)
            if not terminate and (result["forced"] or result["returncode"] != 0):
                raise TransportError(f"headless host failed owned teardown: {result}")
        except BaseException as own:
            error = own
            raise
        finally:
            self.selector.close()
            self.process.stdout.close()
            self.log.close()
            self.remove_root(error)

    def __enter__(self):
        return self

    def __exit__(self, kind, value, traceback):
        if kind is not None:
            self.keep_log(value)
        self.stop(terminate=kind is not None, error=value)


class Window:
    def __init__(self, host, descriptor):
        self.host = host
        self.descriptor = descriptor
        self.id = descriptor["window"]
        self.closed = False

    def verify(self, window=None):
        self.host.verify()
        if self.closed or self.host.windows.get(self.id) is not self:
            raise TransportError("closed or foreign headless window")
        if window is not None and window != self.id:
            raise TransportError("foreign headless window identity")

    def input(self, op, *, window=None, **arguments):
        self.verify(window)
        return self.host.send(dict(arguments, op=op, window=self.id))

    def call(self, function, *arguments, target="flea", window=None, timeout=None):
        self.verify(window)
        if function in ("quit", "scene"):
            raise TransportError("production IPC is read-only in the headless bridge")
        record = dict(self.host.record, **self.descriptor)
        record["root"] = str(self.host.root)
        return exchange(record, ["call", target, function, *map(str, arguments)],
                        self.host.timeout if timeout is None else deadline_seconds(timeout))

    def value(self, function, *arguments):
        reply = self.call(function, *arguments)
        if reply.returncode:
            raise TransportError(f"reader {function} refused ({reply.returncode}): {reply.stderr.decode().strip()}")
        return reply.stdout.decode().removesuffix("\n")

    def observe(self, function, *arguments):
        # A read that straddles a generation change is discarded and retried; one that never settles is refused.
        for _attempt in range(OBSERVE_ATTEMPTS):
            started = time.monotonic_ns()
            before = (int(self.value("listRequests")), json.loads(self.value("searchStamps"))["gen"])
            reply = self.call(function, *arguments)
            after = (int(self.value("listRequests")), json.loads(self.value("searchStamps"))["gen"])
            if before == after:
                break
        else:
            raise TransportError("stale observation: production row generation changed during read")
        return dict(window=self.id, source=self.host.source, started_ns=started,
                    received_ns=time.monotonic_ns(), clock="python-monotonic",
                    row_generation=dict(list_requests=after[0], search=after[1],
                                        semantics="production listing request and search generations; no presentation claim"),
                    code=reply.returncode, stdout=reply.stdout.decode(), stderr=reply.stderr.decode())

    def wait(self, function, expected, *, timeout=None):
        deadline = time.monotonic() + (self.host.timeout if timeout is None else deadline_seconds(timeout))
        actual = None
        while time.monotonic() < deadline:
            actual = self.value(function)
            if actual == expected:
                return actual
            time.sleep(POLL_S)
        raise TimeoutError(f"{function}: expected {expected!r}, observed {actual!r}")

    def close(self, *, window=None):
        self.verify(window)
        result = self.input("close")
        if result != {"closed": True} or Path(self.descriptor["socket"]).exists():
            raise TransportError("owned socket survived window close")
        self.closed = True
        del self.host.windows[self.id]


def replay(driver, source):
    with Host(driver) as host:
        aliases = {}
        for line in source:
            command = json.loads(line)
            op = command.pop("op")
            if op == "open":
                window = host.open(host.root / command["path"])
                aliases[command.get("as", "main")] = window
                result = window.descriptor
            else:
                window = aliases[command.pop("window", "main")]
                if op == "read":
                    result = window.observe(command["function"], *command.get("arguments", []))
                elif op == "wait":
                    result = window.wait(command["function"], command["expected"])
                elif op == "close":
                    window.close()
                    result = {"closed": True}
                else:
                    result = window.input(op, **command)
            print(json.dumps(result), flush=True)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--driver", required=True)
    parser.add_argument("--replay", type=Path, required=True)
    args = parser.parse_args()
    with args.replay.open() as source:
        replay(args.driver, source)
