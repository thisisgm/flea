#!/usr/bin/env python3
"""Multi-item Properties protocol acceptance, inside properties-multi.sh's marked root."""
import datetime
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
assert root.is_absolute() and root.name.startswith("flea-properties-multi-")
assert (root / ".flea-test-sandbox").is_file()
assert not root.is_relative_to(Path.home().resolve())
assert os.geteuid() != 0, "unreadable-folder proof must run without root privileges"
timings = []


def owned(path):
    path = Path(os.path.abspath(path))
    assert str(path) and path.is_absolute() and path.is_relative_to(root) and path != root
    assert path.parent.resolve().is_relative_to(root)
    assert (root / ".flea-test-sandbox").is_file()
    return path


def folder(name):
    path = owned(root / name)
    path.mkdir(parents=True)
    return path


def file(name, data=b"abc"):
    path = owned(root / name)
    assert not path.is_symlink()
    path.write_bytes(data)
    return path


def unlink(path):
    owned(path).unlink()


library = owned(root / "block.so")
subprocess.run(["cc", "-shared", "-fPIC", "-o", str(library),
                str(Path(__file__).with_name("dirsize-block.c")), "-ldl"], check=True)
for name in ("cache", "state", "runtime", "config"):
    folder(name)
owned(root / "runtime").chmod(0o700)


class Backend:
    def __init__(self, blocked=None):
        self.events = queue.Queue()
        self.transcript = []
        self.release = owned(root / ("release-" + str(time.monotonic_ns())))
        self.entered = owned(root / ("entered-" + str(time.monotonic_ns())))
        env = dict(os.environ, TZ="UTC", XDG_CACHE_HOME=str(root / "cache"),
                   XDG_STATE_HOME=str(root / "state"), XDG_CONFIG_HOME=str(root / "config"),
                   XDG_RUNTIME_DIR=str(root / "runtime"))
        if blocked is not None:
            env.update(LD_PRELOAD=str(library), FLEA_TEST_SIZE_PATH=str(blocked),
                       FLEA_TEST_SIZE_ENTERED=str(self.entered), FLEA_TEST_SIZE_RELEASE=str(self.release))
        self.proc = subprocess.Popen([binary, "--backend"], env=env, text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        try:
            for line in self.proc.stdout:
                message = json.loads(line)
                self.transcript.append(message)
                self.events.put(message)
        finally:
            self.events.put({"t": "eof"})

    def send(self, **request):
        self.proc.stdin.write(json.dumps(request) + "\n")
        self.proc.stdin.flush()

    def until(self, predicate):
        deadline = time.monotonic() + 5
        while True:
            try:
                reply = self.events.get(timeout=max(.001, deadline - time.monotonic()))
            except queue.Empty:
                raise AssertionError("backend reply timed out: " + repr(self.transcript)) from None
            assert reply["t"] not in ("eof", "error"), reply
            if predicate(reply):
                return reply

    def action(self, op, **extra):
        self.send(c="menuaction", op=op, id=7, **extra)
        return self.until(lambda r: r.get("op") == op and r.get("id") == 7)

    def snapshot(self, paths):
        self.send(c="listpaths", paths=[str(p) for p in paths], first=len(paths))
        rows = self.until(lambda r: r["t"] == "rows")
        assert len(rows["rows"]) == len(paths), rows
        captured = self.action("snapshot", rows=list(range(len(paths))))
        assert captured["ok"] and captured["count"] == len(paths), captured

    def properties(self):
        started = time.monotonic()
        reply = self.action("properties")
        while reply["ok"] and not reply.get("done") and not reply.get("cancelled"):
            reply = self.until(lambda r: r.get("op") == "properties" and r.get("id") == 7)
        timings.append((time.monotonic() - started) * 1000)
        return reply

    def barrier(self):
        deadline = time.monotonic() + 5
        while not self.entered.exists():
            assert self.proc.poll() is None
            assert time.monotonic() < deadline, "walk did not reach the opendir barrier"
            time.sleep(.005)

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.release.touch()
        if self.proc.poll() is None:
            self.send(c="quit")
            try:
                assert self.proc.wait(timeout=5) == 0
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
                raise AssertionError("backend did not quit") from None
        self.reader.join(timeout=1)
        self.proc.stdin.close()
        self.proc.stdout.close()


def complete(reply):
    assert reply["ok"] and reply["done"] and not reply["partial"], reply
    assert not reply.get("cancelled"), reply
    assert set(reply) == {"t", "id", "op", "ok", "kinds", "bytes", "newest", "oldest", "partial", "done"}, reply


images = [file("a.jpg"), file("b.png", b"12345")]
video = file("c.mp4", b"1234567")
for path, stamp in zip([*images, video], [1787313720, 1790158440, 1790000000]):
    os.utime(owned(path), (stamp, stamp))
with Backend() as backend:
    backend.snapshot([*images, video])
    reply = backend.properties()
    complete(reply)
    assert reply["kinds"] == "2 images, 1 video", reply
    assert reply["bytes"] == 15, reply
    assert reply["oldest"] == "2026-08-21 12:02", reply
    assert reply["newest"] == "2026-09-23 10:14", reply
    print("ok properties multi: specimen kinds, bytes and minute dates", flush=True)

tree = folder("mixed/tree")
sub = folder("mixed/tree/sub")
payload = file("mixed/tree/sub/data", b"123456789")
plain = file("mixed/plain", b"1234")
target = folder("target")
file("target/large", b"x" * 100000)
link = owned(root / "mixed/picture.jpg")
link.symlink_to(target, target_is_directory=True)
inside_link = owned(tree / "link")
inside_link.symlink_to(target, target_is_directory=True)
os.utime(owned(payload), (2000000000, 2000000000))
with Backend() as backend:
    backend.snapshot([tree, plain, link])
    reply = backend.properties()
    complete(reply)
    want = sum(p.lstat().st_size for p in [tree, sub, payload, plain, link, inside_link])
    assert reply["bytes"] == want, (reply, want)
    assert reply["kinds"] == "1 folder, 1 symbolic link, 1 file", reply
    assert reply["newest"] != datetime.datetime.fromtimestamp(2000000000, datetime.timezone.utc).strftime("%Y-%m-%d %H:%M"), reply
    print("ok properties multi: mixed tree, links never followed, selected dates only", flush=True)

denied = folder("denied")
secret = file("denied/secret", b"not-counted")
owned(denied).chmod(0)
try:
    with Backend() as backend:
        backend.snapshot([denied, plain])
        reply = backend.properties()
        assert reply["ok"] and reply["done"] and reply["partial"], reply
        assert reply["bytes"] == denied.lstat().st_size + plain.lstat().st_size, reply
        print("ok properties multi: unreadable folder reports a partial floor", flush=True)
finally:
    owned(denied).chmod(0o700)

for scenario in ("vanished", "replaced"):
    gone = file(scenario + ".txt")
    with Backend() as backend:
        backend.snapshot([gone, plain])
        if scenario == "vanished":
            unlink(gone)
        else:
            owned(gone).rename(owned(root / "held-original.txt"))
            file(scenario + ".txt", b"replacement")
        reply = backend.properties()
        assert not reply["ok"] and "reopen the menu" in reply["error"], reply
        assert "bytes" not in reply, reply
        print("ok properties multi: refuses " + scenario + " selected identity", flush=True)

with Backend() as backend:
    backend.snapshot(images)
    backend.send(c="list", path=str(target), first=10)
    backend.until(lambda r: r["t"] == "rows")
    reply = backend.properties()
    complete(reply)
    assert reply["bytes"] == 8 and reply["kinds"] == "2 images", reply
    assert backend.action("close")["ok"]
    assert not backend.action("properties")["ok"]
    print("ok properties multi: holds paths across navigation and expires on close", flush=True)

blocked = folder("blocked")
file("blocked/data", b"abc")
with Backend(blocked) as backend:
    backend.snapshot([blocked, plain])
    first = backend.action("properties")
    assert first["ok"] and not first["done"], first
    backend.barrier()
    started = time.monotonic()
    assert backend.action("close")["ok"]
    backend.release.touch()
    cancelled = backend.until(lambda r: r.get("op") == "properties" and r.get("cancelled"))
    assert cancelled["ok"] and cancelled["partial"] and not cancelled["done"], cancelled
    assert time.monotonic() - started < 2
    assert not any(r.get("op") == "properties" and r.get("done") for r in backend.transcript)
    print("ok properties multi: close cancels an active walk without an exact ending", flush=True)

vanish_during = file("vanish-during.txt")
with Backend(blocked) as backend:
    backend.snapshot([blocked, vanish_during])
    assert not backend.action("properties")["done"]
    backend.barrier()
    unlink(vanish_during)
    backend.release.touch()
    reply = backend.until(lambda r: r.get("op") == "properties" and not r["ok"])
    assert "disappeared" in reply["error"], reply
    print("ok properties multi: revalidates every selected identity after a walk", flush=True)

with Backend(blocked) as backend:
    backend.snapshot([blocked, plain])
    assert not backend.action("properties")["done"]
    backend.barrier()
    backend.send(c="menuaction", op="snapshot", id=8, rows=[1])
    # Actions are answered on the input loop, after snapshot has advanced cancellation, while its walk is still held.
    backend.send(c="actions", op="list", id=9001, paths=[])
    backend.until(lambda r: r["t"] == "actions" and r["id"] == 9001)
    backend.release.touch()
    cancelled = backend.until(lambda r: r.get("op") == "properties" and r.get("cancelled"))
    assert not cancelled["done"] and cancelled["partial"], cancelled
    snapshot = backend.until(lambda r: r.get("op") == "snapshot" and r["id"] == 8)
    assert snapshot["ok"] and snapshot["count"] == 1, snapshot
    backend.send(c="menuaction", op="properties", id=8)
    single = backend.until(lambda r: r.get("op") == "properties" and r["id"] == 8)
    assert single["ok"] and single["path"] == str(plain), single
    print("ok properties multi: a newer snapshot cancels and keeps its own selection", flush=True)

with Backend(blocked) as backend:
    backend.snapshot([blocked, plain])
    assert not backend.action("properties")["done"]
    backend.barrier()
    backend.release.touch()
    finished = backend.until(lambda r: r.get("op") == "properties" and r.get("done"))
    complete(finished)
    backend.send(c="menuaction", op="snapshot", id=8, rows=[1])
    snapshot = backend.until(lambda r: r.get("op") == "snapshot" and r["id"] == 8)
    assert snapshot["ok"] and snapshot["count"] == 1, snapshot
    assert not any(r.get("op") == "properties" and r.get("cancelled") for r in backend.transcript)
    print("ok properties multi: completion before a newer snapshot remains exact", flush=True)

print("properties-multi: 9 protocol scenarios passed (both snapshot orderings)", flush=True)
print(f"properties-multi: request completion range {min(timings):.3f}..{max(timings):.3f} ms ({len(timings)} requests)", flush=True)
