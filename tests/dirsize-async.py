#!/usr/bin/env python3
"""Protocol regression with a pinned opendir; called inside protocol.sh's guarded sandbox."""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time

root = Path(sys.argv[1]).resolve()
binary = str(Path(sys.argv[2]).resolve())
assert root.name.startswith("flea-dirsize-async-")
assert (root / ".flea-test-sandbox").is_file()
assert not root.is_relative_to(Path.home().resolve())
library = root / "block.so"
subprocess.run(["cc", "-shared", "-fPIC", "-o", str(library),
                str(Path(__file__).with_name("dirsize-block.c")), "-ldl"], check=True)


class Backend:
    def __init__(self, directory):
        self.directory = directory
        self.entered = directory / "entered"
        self.release = directory / "release"
        self.slow = directory / "tree/aaa"
        self.fast = directory / "tree/zzz"
        self.destination = directory / "destination/only"
        for path in (self.slow, self.fast, self.destination):
            path.mkdir(parents=True)
        (self.slow / "data").write_bytes(b"slow")
        (self.fast / "data").write_bytes(b"fast")
        (self.destination / "data").write_bytes(b"destination")
        env = dict(os.environ, LD_PRELOAD=str(library),
                   FLEA_TEST_SIZE_PATH=str(self.slow),
                   FLEA_TEST_SIZE_ENTERED=str(self.entered),
                   FLEA_TEST_SIZE_RELEASE=str(self.release))
        self.proc = subprocess.Popen([binary, "--backend"], env=env, text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                     stderr=None)
        self.replies = queue.Queue()
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()
        try:
            self.start_wait()
        except BaseException:
            self.close()
            raise

    def start_wait(self):
        self.send(c="list", path=str(self.directory / "tree"), first=10)
        self.until("rows")
        self.send(c="dirsize", rows=[0])
        deadline = time.monotonic() + 5
        while not self.entered.exists():
            assert self.proc.poll() is None, "backend exited before size walk"
            assert time.monotonic() < deadline, "size worker did not reach opendir barrier"
            time.sleep(.005)

    def read(self):
        for line in self.proc.stdout:
            self.replies.put(json.loads(line))
        self.replies.put({"t": "eof"})

    def send(self, **request):
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()

    def until(self, kind, allow_size=False):
        deadline = time.monotonic() + 3
        while True:
            try:
                message = self.replies.get(timeout=max(.001, deadline - time.monotonic()))
            except queue.Empty:
                raise AssertionError("backend did not answer " + kind + " while size work was pending") from None
            assert message["t"] not in ("error", "eof"), message
            if message["t"] == kind:
                return message
            assert allow_size or message["t"] != "dirsized", "unexpected stale size reply"

    def fresh(self, path):
        self.send(c="dirsize", rows=[0])
        self.release.touch()
        reply = self.until("dirsized")
        expected = path.stat().st_size + sum(p.lstat().st_size for p in path.iterdir())
        assert reply["row"] == 0 and reply["bytes"] == expected, reply
        assert reply["partial"] is False, "cancelled result leaked"
        self.send(c="fsinfo")
        self.until("fsinfo")  # A second size reply would be an old-generation leak.

    def close(self):
        self.release.touch()
        if self.proc.poll() is None:
            self.send(c="quit")
            try:
                self.proc.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        self.reader.join(timeout=1)
        self.proc.stdin.close()
        self.proc.stdout.close()


for scenario in ("cancel", "list", "sort", "quit"):
    backend = None
    try:
        backend = Backend(root / scenario)
        if scenario == "quit":
            backend.send(c="quit")
            assert backend.proc.wait(timeout=3) == 0
            assert not backend.release.exists(), "quit needed the filesystem barrier released"
        elif scenario == "cancel":
            backend.send(c="dirsizecancel")
            backend.send(c="fsinfo")
            backend.until("fsinfo")  # Must answer while the filesystem call remains blocked.
            backend.fresh(backend.slow)
        elif scenario == "list":
            backend.send(c="list", path=str(backend.directory / "destination"), first=10)
            rows = backend.until("rows")
            assert rows["rows"][0]["n"] == "only"
            backend.fresh(backend.destination)
        else:
            backend.send(c="sort", by="name", desc=True)
            backend.until("listed")
            backend.fresh(backend.fast)
        print("ok   running size walk: " + scenario, flush=True)
    finally:
        if backend is not None:
            backend.close()
