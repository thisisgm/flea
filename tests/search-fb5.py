"""Verify production search triplets, paging, mutable row facts and closed-session silence."""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import threading

parent = (Path.cwd() / ".superpowers" / "fb5").resolve()
parent.mkdir(parents=True, exist_ok=True)
root = Path(subprocess.check_output(["mktemp", "-d", str(parent / "search-wire.XXXXXX")], text=True).strip())
marker = root / ".flea-test-sandbox"
marker.write_text("flea test sandbox\n")
process = None
try:
    scope = root / "scope"
    scope.mkdir()
    for i in range(800):
        (scope / f"alpha{i:04}.txt").write_bytes(b"a")
    other = root / "other"
    other.mkdir()
    alias = other / "alias.txt"
    os.link(scope / "alpha0000.txt", alias)
    env = dict(os.environ)
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR"]:
        directory = root / key.lower()
        directory.mkdir(mode=0o700)
        env[key] = str(directory)
    process = subprocess.Popen([sys.argv[1], "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, env=env)
    lines = queue.Queue()

    def read():
        for line in process.stdout:
            lines.put(json.loads(line))
        lines.put(None)

    threading.Thread(target=read, daemon=True).start()

    def send(**line):
        process.stdin.write(json.dumps(line) + "\n")
        process.stdin.flush()

    def receive():
        line = lines.get(timeout=30)
        assert line is not None, "backend ended"
        assert line.get("t") != "error", line
        return line

    def answer(gen):
        result = []
        while True:
            line = receive()
            result.append(line)
            if line.get("t") == "searched" and line.get("gen") == gen and not line["walking"]:
                return result

    send(c="searchopen", path=str(scope), folder=str(scope), hidden=False)
    assert receive()["t"] == "searchopened"
    send(c="searchquery", query="alpha", gen=1, count=3, sizes=0)
    first = answer(1)
    assert first[-1]["n"] == 800 and not first[-1]["partial"], first
    assert any(row["n"] == "alpha0000.txt" for line in first for row in line.get("rows", [])), first
    send(c="searchquery", query="al", gen=2, count=3, sizes=0)
    second = answer(2)
    assert [line["t"] for line in second] == ["listed", "rows", "searched"], second
    assert all(line["gen"] == 2 for line in second), second
    send(c="window", start=798, count=2)
    tail = receive()
    assert [row["n"] for row in tail["rows"]] == ["alpha0798.txt", "alpha0799.txt"], tail
    path = scope / "alpha0000.txt"
    assert path.is_absolute() and path.parent == scope and marker.is_file()
    path.write_bytes(b"changed bytes")
    send(c="searchquery", query="alpha0000.txt", gen=3, count=1, sizes=0)
    changed = answer(3)
    assert any(row["s"] == 13 for line in changed for row in line.get("rows", [])), changed
    assert alias.is_absolute() and alias.parent == other and marker.is_file()
    alias.write_bytes(b"hard link changed")
    send(c="searchquery", query="alpha0000.txt", gen=4, count=1, sizes=0)
    linked = answer(4)
    linked_size = [row["s"] for line in linked for row in line.get("rows", [])][-1]
    assert linked_size == path.stat().st_size == 17, linked
    print(f"SEARCH_FB5_HARDLINK cached_size={linked_size} filesystem_size={path.stat().st_size}")
    send(c="searchclose")
    assert receive()["t"] == "searchclosed"
    send(c="searchquery", query="alpha", gen=99)
    send(c="window", start=0, count=1)
    after = receive()
    assert after["t"] == "rows" and "gen" not in after, after
    send(c="quit")
    process.wait(timeout=30)
    assert process.returncode == 0, process.stderr.read()
    print("SEARCH_FB5_PROTOCOL triplets=ok generations=ok backspace=ok paging=800 mutation=ok hardlink=ok close=ok")
finally:
    if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=10)
    assert str(root) and root.is_absolute() and root.parent == parent and root.name.startswith("search-wire.")
    assert marker.read_text() == "flea test sandbox\n"
    shutil.rmtree(root)
