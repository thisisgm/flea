"""Real backend protocol regression for identity remapping after recursive watch deltas."""
import json
import os
from pathlib import Path
import selectors
import shutil
import subprocess
import sys
import tempfile
import time

root = Path(tempfile.mkdtemp(prefix="flea-search-remap."))
(root / ".flea-test-sandbox").write_text("Search remap protocol\n")
scope = root / "files"
scope.mkdir()
for folder in ("a", "b", "c"):
    (scope / folder).mkdir()
    (scope / folder / "same.txt").write_text(folder)
process = subprocess.Popen([sys.argv[1], "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                           env=dict(os.environ, FLEA_UNDO_DIR=str(root / "undo")), bufsize=0)
selector = selectors.DefaultSelector()
selector.register(process.stdout, selectors.EVENT_READ)
pending = b""


def send(**request):
    process.stdin.write((json.dumps(request) + "\n").encode())
    process.stdin.flush()


def receive(kind):
    global pending
    end = time.monotonic() + 15
    while time.monotonic() < end:
        while b"\n" in pending:
            line, pending = pending.split(b"\n", 1)
            doc = json.loads(line)
            if doc["t"] == "error":
                raise AssertionError(doc)
            if doc["t"] == kind:
                return doc
        for key, _ in selector.select(max(0, end - time.monotonic())):
            data = os.read(key.fd, 65536)
            assert data, "backend closed"
            pending += data
    raise AssertionError(f"no {kind} reply")


def landed():
    rows = receive("rows")
    rows["walking"] = receive("searched").get("walking", False)
    return rows


def settled():
    global pending
    end = time.monotonic() + 15
    # One file change can raise several watch deltas, so replacements are drained until the backend stays quiet.
    while selector.select(1):
        data = os.read(process.stdout.fileno(), 65536)
        assert data and time.monotonic() < end, "backend closed or never settled"
        pending += data
    *lines, pending = pending.split(b"\n")
    errors = [line for line in lines if json.loads(line)["t"] == "error"]
    assert not errors, errors
    send(c="window", start=0, count=100)
    return receive("rows")


try:
    send(c="searchopen", path=str(scope), folder=str(scope))
    receive("searchopened")
    send(c="searchquery", query="same.txt", gen=7, count=1)
    initial = landed()
    while initial.get("walking", False):
        initial = landed()
    names = ["a/same.txt", "b/same.txt", "c/same.txt"]
    assert [row["n"] for row in initial["rows"]] == names[:1]
    source = initial["listing"]
    cursor = names.index("b/same.txt")
    os.chmod(scope / "a/same.txt", 0o640)
    landed()
    (scope / "same.txt").write_text("new first result")
    (scope / "a/same.txt").unlink()
    current = settled()
    expected = sorted([row["n"] for row in current["rows"]])
    assert expected == ["b/same.txt", "c/same.txt", "same.txt"], expected
    send(c="searchremap", gen=7, **{"from": source}, rows=list(range(3)), cursor=cursor)
    result = receive("searchremap")
    assert result["ok"] and result["listing"] == current["listing"], result
    assert result["rows"] == sorted(at for at, row in enumerate(current["rows"]) if row["n"] in names[1:]), result
    assert current["rows"][result["cursor"]]["n"] == "b/same.txt", result
    send(c="searchremap", gen=7, **{"from": source}, rows=[0], cursor=0)
    assert not receive("searchremap")["ok"], "consumed source stamp accepted"
    send(c="searchremap", gen=7, **{"from": result["listing"]}, rows=[0], cursor=0)
    assert not receive("searchremap")["ok"], "answered listing kept as a source before any index change"
    (scope / "b/same.txt").unlink()
    settled()
    send(c="searchremap", gen=7, **{"from": result["listing"]}, rows=result["rows"], cursor=result["cursor"])
    missing = receive("searchremap")
    assert missing["ok"] and missing["cursor"] is None and len(missing["rows"]) == 1, missing
    os.chmod(scope / "c/same.txt", 0o600)
    held = settled()
    send(c="searchremap", gen=6, **{"from": missing["listing"]}, rows=[0], cursor=0)
    assert not receive("searchremap")["ok"], "older query generation accepted"
    send(c="searchremap", gen=7, **{"from": missing["listing"]}, rows=[0], cursor=0)
    assert receive("searchremap")["ok"], "current source refused"
    os.chmod(scope / "c/same.txt", 0o644)
    settled()
    send(c="searchquery", query="same", gen=8, count=10)
    landed()
    queried = settled()
    send(c="searchremap", gen=8, **{"from": held["listing"]}, rows=[0], cursor=0)
    assert not receive("searchremap")["ok"], "new query kept the old snapshot"
    os.chmod(scope / "c/same.txt", 0o600)
    moved = settled()
    send(c="searchremap", gen=8, **{"from": queried["listing"]}, rows=[0], cursor=0)
    assert receive("searchremap")["ok"], "new query did not snapshot its own listing"
    os.chmod(scope / "c/same.txt", 0o644)
    settled()
    send(c="list", path=str(scope), first=10)
    receive("rows")
    send(c="searchopen", path=str(scope), folder=str(scope))
    receive("searchopened")
    send(c="searchquery", query="same", gen=8, count=10)
    landed()
    settled()
    send(c="searchremap", gen=8, **{"from": moved["listing"]}, rows=[0], cursor=0)
    assert not receive("searchremap")["ok"], "navigation kept the snapshot"
    print("ok   searchremap: repeated replacements, full paths, missing files, off-window cursor, stale stamps and navigation")
finally:
    try:
        send(c="quit")
    except OSError:
        pass
    try:
        process.wait(timeout=30)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()
    assert root.is_absolute() and root.parent == Path(tempfile.gettempdir()) and root.resolve() == root
    assert (root / ".flea-test-sandbox").is_file()
    shutil.rmtree(root)
