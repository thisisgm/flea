#!/usr/bin/env python3
"""Properties race regressions, inside properties-races.sh's marked root."""
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
assert root.is_absolute() and root.name.startswith("flea-properties-races-")
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
                str(Path(__file__).with_name("properties-races-block.c")), "-ldl"], check=True)
probe = owned(root / "statx-probe")
subprocess.run(["cc", "-o", str(probe),
                str(Path(__file__).with_name("properties-races-block-test.c"))], check=True)
subprocess.run([str(probe)], cwd=root, env=dict(os.environ, LD_PRELOAD=str(library),
               FLEA_REVIEW_STAT=str(root / "unused"), FLEA_REVIEW_ARM=str(root / "unarmed")),
               stdin=subprocess.DEVNULL, check=True)
for name in ("cache", "state", "runtime", "config"):
    folder(name)
owned(root / "runtime").chmod(0o700)


class Backend:
    def __init__(self, blocked=None, stat_target=None, mount_body=None):
        self.events = queue.Queue()
        self.transcript = []
        self.raw = []
        self.release = owned(root / ("release-" + str(time.monotonic_ns())))
        self.entered = owned(root / ("entered-" + str(time.monotonic_ns())))
        self.stats = owned(root / ("stats-" + str(time.monotonic_ns())))
        env = dict(os.environ, TZ="UTC", XDG_CACHE_HOME=str(root / "cache"),
                   XDG_STATE_HOME=str(root / "state"), XDG_CONFIG_HOME=str(root / "config"),
                   XDG_RUNTIME_DIR=str(root / "runtime"))
        if blocked is not None:
            env.update(LD_PRELOAD=str(library), FLEA_TEST_SIZE_PATH=str(blocked),
                       FLEA_TEST_SIZE_ENTERED=str(self.entered), FLEA_TEST_SIZE_RELEASE=str(self.release))
        if stat_target is not None or mount_body is not None:
            env["LD_PRELOAD"] = str(library)
        if stat_target is not None:
            env.update(FLEA_REVIEW_STAT=str(stat_target), FLEA_REVIEW_ARM=str(root / "arm"),
                       FLEA_REVIEW_ENTERED=str(self.entered), FLEA_REVIEW_RELEASE=str(self.release),
                       FLEA_REVIEW_STATS=str(self.stats))
        if mount_body is not None:
            env["FLEA_REVIEW_MOUNTS"] = str(mount_body)
        self.proc = subprocess.Popen([binary, "--backend"], env=env, text=True,
                                     stdin=subprocess.PIPE, stdout=subprocess.PIPE)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def read(self):
        try:
            for line in self.proc.stdout:
                message = json.loads(line)
                self.raw.append(line.rstrip("\n"))
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


failures = []
plain = file("plain", b"abc")

# Replace a listed child only after the walk chose to descend into it.
tree = folder("race/tree")
child = folder("race/tree/child")
file("race/tree/child/original", b"x")
outside = folder("outside")
file("outside/payload", b"x" * 1000000)
with Backend(child) as backend:
    backend.snapshot([tree, plain])
    assert not backend.action("properties")["done"]
    backend.barrier()
    owned(child).rename(owned(root / "saved-child"))
    owned(child).symlink_to(outside, target_is_directory=True)
    backend.release.touch()
    reply = backend.until(lambda r: r.get("op") == "properties" and (r.get("done") or not r["ok"]))
    print("SYMLINK_REPLACEMENT", json.dumps(reply), flush=True)
    if not (reply.get("ok") and reply.get("done") and reply.get("partial") and reply.get("bytes", 0) < 1000000):
        failures.append("a replaced child must be an unfollowed partial floor")

enclosing = folder("enclosing")
remote = folder("enclosing/remote")
file("enclosing/remote/data", b"abc")
mount_body = file("mountinfo", ("1 0 8:1 / / rw - ext4 /dev/a rw\n30 1 0:45 / " + str(enclosing) + " rw - nfs n:/s rw\n").encode())
arm = owned(root / "arm")
with Backend(stat_target=remote, mount_body=mount_body) as backend:
    backend.snapshot([remote, plain])
    arm.touch()
    started = time.monotonic()
    backend.send(c="menuaction", op="properties", id=7)
    backend.barrier()
    time.sleep(16)
    before_release = [r for r in backend.transcript if r.get("op") == "properties"]
    print("REMOTE_INITIAL_STAT elapsed=", round(time.monotonic() - started, 3), "properties=", before_release, flush=True)
    terminal = backend.until(lambda r: r.get("op") == "properties" and (r.get("done") or not r["ok"]))
    if not (terminal["ok"] and terminal.get("done") and terminal.get("partial") and terminal.get("bytes") == 0):
        failures.append("initial metadata timeout must finish with a zero partial floor")
    calls = backend.stats.read_text()
    repeated = backend.properties()
    if not (repeated["ok"] and repeated.get("done") and repeated.get("partial") and timings[-1] < 1000 and backend.stats.read_text() == calls):
        failures.append("an enclosing stuck mount must answer before another selected-item stat")
    print("REMOTE_ENCLOSING_STUCK repeated_ms=", round(timings[-1], 3), "stat_calls=", len(backend.stats.read_text()), "reply=", repeated, flush=True)
    assert backend.action("close")["ok"]
    backend.send(c="menuaction", op="snapshot", id=8, rows=[1])
    time.sleep(.5)
    ready = any(r.get("op") == "snapshot" and r.get("id") == 8 for r in backend.transcript)
    print("REMOTE_INITIAL_STAT snapshot_after_close=", ready, flush=True)
    owned(Path(str(backend.release) + ".stat")).touch()
    backend.until(lambda r: r.get("op") == "snapshot" and r.get("id") == 8)
    if not before_release or not ready:
        failures.append("initial remote metadata bypassed bulk deadline and held menu worker after close")
    unlink(arm)

with Backend(blocked=remote, stat_target=plain, mount_body=mount_body) as backend:
    backend.snapshot([remote, plain])
    assert not backend.action("properties")["done"]
    backend.barrier()
    # Switch from the opendir barrier to the final selected-file metadata barrier.
    unlink(backend.entered)
    arm.touch()
    backend.release.touch()
    # The C helper appends .stat so releasing opendir cannot release the metadata barrier.
    deadline = time.monotonic() + 5
    while not backend.entered.exists():
        assert time.monotonic() < deadline, "final validation did not reach metadata barrier"
        time.sleep(.005)
    time.sleep(16)
    finals = [r for r in backend.transcript if r.get("op") == "properties" and (r.get("done") or not r["ok"])]
    print("REMOTE_FINAL_STAT after_16s_terminal=", finals, flush=True)
    owned(Path(str(backend.release) + ".stat")).touch()
    backend.until(lambda r: r.get("op") == "properties" and (r.get("done") or not r["ok"]))
    expected_floor = sum(p.lstat().st_size for p in (remote, remote / "data", plain))
    if not (finals and finals[-1]["ok"] and finals[-1].get("partial") and finals[-1].get("done") and finals[-1].get("bytes") == expected_floor):
        failures.append("final identity timeout must retain the complete counted floor")
    unlink(arm)

with Backend(blocked=remote, mount_body=mount_body) as backend:
    backend.snapshot([remote, plain])
    assert not backend.action("properties")["done"]
    backend.barrier()
    time.sleep(16)
    terminal = backend.until(lambda r: r.get("op") == "properties" and (r.get("done") or not r["ok"]))
    if not (terminal["ok"] and terminal.get("done") and terminal.get("partial") and terminal.get("bytes", 0) >= plain.stat().st_size + remote.stat().st_size):
        failures.append("a remote folder deadline must finish with its counted partial floor")
    repeated = backend.properties()
    if not (repeated["ok"] and repeated.get("done") and repeated.get("partial") and timings[-1] < 1000):
        failures.append("a timed-out folder must mark its enclosing mount stuck")
    print("REMOTE_WALK_FLOOR", terminal, "stuck_repeat_ms=", round(timings[-1], 3), flush=True)

single_tree = folder("single-race/tree")
single_child = folder("single-race/tree/child")
file("single-race/tree/child/original", b"x")
with Backend(single_child) as backend:
    backend.snapshot([single_tree])
    backend.send(c="menuaction", op="contents", id=7)
    backend.barrier()
    owned(single_child).rename(owned(root / "single-saved-child"))
    owned(single_child).symlink_to(outside, target_is_directory=True)
    backend.release.touch()
    reply = backend.until(lambda r: r.get("op") == "contents" and r.get("done"))
    assert reply["ok"] and reply["partial"] and reply["bytes"] < 1000000, reply
    print("SINGLE_SYMLINK_REPLACEMENT", json.dumps(reply), flush=True)

stable = folder("stable")
stable_sub = folder("stable/sub")
stable_a = file("stable/a", b"abc")
stable_b = file("stable/sub/b", b"de")
stable_link = owned(root / "stable/link")
stable_link.symlink_to(outside, target_is_directory=True)
with Backend() as backend:
    backend.snapshot([stable])
    reply = backend.action("contents")
    expected = {"t": "menuaction", "id": 7, "op": "contents", "ok": True,
                "bytes": sum(p.lstat().st_size for p in (stable, stable_sub, stable_a, stable_b, stable_link)),
                "items": 4, "partial": False, "done": True}
    assert reply == expected, (reply, expected)
    assert json.dumps(expected, separators=(",", ":")) in backend.raw, backend.raw
    print("SINGLE_CONTENTS_BYTE_IDENTICAL", json.dumps(expected, separators=(",", ":")), flush=True)

for phase in ("initial", "final", "files"):
    initial = phase != "final"
    selected_remote = remote / "data" if phase == "files" else remote
    with Backend(blocked=remote if phase == "final" else None,
                 stat_target=selected_remote if initial else plain, mount_body=mount_body) as backend:
        backend.snapshot([selected_remote, plain])
        if initial:
            arm.touch()
            backend.send(c="menuaction", op="properties", id=7)
            backend.barrier()
        else:
            assert not backend.action("properties")["done"]
            backend.barrier()
            unlink(backend.entered)
            arm.touch()
            backend.release.touch()
            backend.barrier()
        started = time.monotonic()
        assert backend.action("close")["ok"]
        backend.send(c="menuaction", op="snapshot", id=8, rows=[1])
        snapshot = backend.until(lambda r: r.get("op") == "snapshot" and r.get("id") == 8)
        elapsed = time.monotonic() - started
        assert snapshot["ok"] and elapsed < 1, (snapshot, elapsed)
        assert not Path(str(backend.release) + ".stat").exists(), "metadata must remain blocked until snapshot 8 answered"
        cancelled = [r for r in backend.transcript if r.get("op") == "properties" and r.get("cancelled")]
        assert cancelled and cancelled[-1]["partial"] and not cancelled[-1]["done"], cancelled
        owned(Path(str(backend.release) + ".stat")).touch()
        backend.send(c="menuaction", op="properties", id=8)
        single = backend.until(lambda r: r.get("op") == "properties" and r.get("id") == 8)
        assert single["ok"] and single["path"] == str(plain), single
        assert not any(r.get("op") == "properties" and r.get("id") == 7 and r.get("done") for r in backend.transcript)
        unlink(arm)
        print("REMOTE_CANCEL", phase, "snapshot_after_close_ms=", round(elapsed * 1000, 3), flush=True)

for failure in failures:
    print("FAIL properties-races:", failure, flush=True)
assert not failures, failures
