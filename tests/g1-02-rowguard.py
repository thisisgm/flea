#!/usr/bin/env python3
"""Drive captured row generations through the real backend in a marked fixture."""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading

root = Path(sys.argv[1])
if not root.is_absolute() or not (root / ".flea-test-sandbox").is_file():
    raise SystemExit("g1-02 rowguard requires an absolute marked fixture")
for folder in ("a", "b"):
    (root / folder).mkdir()
    with (root / folder / "keep.txt").open("x") as stream:
        stream.write(folder)
child = subprocess.Popen([os.environ["FLEA_BIN"], "--backend"], stdin=subprocess.PIPE,
                         stdout=subprocess.PIPE, text=True)
replies = queue.Queue()


def read():
    for line in child.stdout:
        replies.put(json.loads(line))


threading.Thread(target=read, daemon=True).start()


def send(**request):
    child.stdin.write(json.dumps(request) + "\n")
    child.stdin.flush()


def await_kind(kind):
    for _ in range(128):
        reply = replies.get(timeout=10)
        if reply.get("t") == kind:
            return reply
    raise AssertionError("backend did not answer " + kind)


def listing(folder):
    send(c="list", path=str(root / folder), first=320, hidden=False)
    header = await_kind("listed")
    rows = await_kind("rows")
    assert header["n"] == 1 and rows["rows"][0]["n"] == "keep.txt"
    return rows["listing"]


try:
    first = listing("a")
    send(c="paths", rows=[0], listing=first)
    assert await_kind("paths")["paths"] == [str(root / "a/keep.txt")]
    second = listing("b")
    assert second > first
    for command in ("paths", "trash"):
        send(c=command, rows=[0], listing=first)
        assert await_kind("error")["where"] == "stale"
        assert (root / "a/keep.txt").read_text() == "a"
        assert (root / "b/keep.txt").read_text() == "b"
    third = listing("b")
    assert third > second
    send(c="paths", rows=[0], listing=second)
    assert await_kind("error")["where"] == "stale"
    send(c="paths", rows=[0], listing=third)
    assert await_kind("paths")["paths"] == [str(root / "b/keep.txt")]
    send(c="quit")
    child.stdin.close()
    assert child.wait(timeout=10) == 0
finally:
    if child.poll() is None:
        child.kill()
        child.wait(timeout=10)
print("G1_02 ROWGUARD PASS captured paths and trash refused after navigation and same-path relist")
