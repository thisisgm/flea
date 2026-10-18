"""Local scope removal and requested rows through the shipping JSON-lines executable."""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import threading
import time


def run(binary, sandbox):
    tree = sandbox / "tree"
    (tree / "deep").mkdir(parents=True)
    (tree / "needle-a").write_text("a")
    (tree / "deep/needle-b").write_text("b")
    (tree / ".needle-hidden").write_text("hidden")
    (tree / "loop").symlink_to(tree, target_is_directory=True)
    env = dict(os.environ)
    for key in ("XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR", "XDG_DATA_HOME"):
        directory = sandbox / key.lower()
        directory.mkdir(mode=0o700)
        env[key] = str(directory)
    process = subprocess.Popen([binary, "--backend"], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    lines = queue.Queue()

    def read():
        for line in process.stdout:
            lines.put(json.loads(line))
        lines.put(None)

    reader = threading.Thread(target=read, daemon=True)
    reader.start()

    def send(**request):
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()

    def answer(gen, partial):
        deadline = time.monotonic() + 15
        listed = rows = None
        while True:
            line = lines.get(timeout=max(0.001, deadline - time.monotonic()))
            assert line is not None and line.get("t") != "error", line
            if line.get("t") == "listed":
                listed = line
                rows = None
            if line.get("t") == "rows":
                assert listed is not None, line
                rows = line
                assert isinstance(rows["listing"], int), rows
                assert len(rows["rows"]) <= 1, rows
            if line.get("t") == "searched" and not line["walking"]:
                assert line["gen"] == gen, line
                if line["partial"] == partial:
                    assert listed is not None and rows is not None, line
                    assert listed["gen"] == rows["gen"] == gen, (listed, rows)
                    return line, rows

    try:
        send(c="searchopen", path=str(tree), folder=str(tree), hidden=False)
        send(c="searchquery", query="needle", gen=41, count=1, sizes=0)
        done, rows = answer(41, False)
        assert done["scanned"] == 4 and done["n"] == 2, done
        assert len(rows["rows"]) == 1 and rows["rows"][0]["p"] != 0, rows
        assert tree.is_absolute() and tree.is_relative_to(sandbox)
        assert (sandbox / ".flea-test-sandbox").is_file()
        tree.rename(sandbox / "removed")
        removed, rows = answer(41, True)
        assert removed["partial_reason"] == "watch_failed", removed
        assert removed["n"] == 0 and rows["rows"] == [], (removed, rows)
        send(c="searchclose")
        send(c="searchopen", path=str(tree), folder=str(sandbox), hidden=False)
        send(c="searchquery", query="needle", gen=42, count=1, sizes=0)
        missing, rows = answer(42, True)
        assert missing["partial_reason"] == "unreadable" and missing["scanned"] == 0, missing
        assert rows["rows"] == [], rows
        send(c="quit")
        process.wait(timeout=15)
        reader.join(timeout=5)
        assert process.returncode == 0, process.stderr.read()
        print("FB7_PROTOCOL rich_rows=1 scanned=4 matches=2 hidden_and_symlink=preserved removed_scope=watch_failed missing_root=unreadable")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)


scratch_parent = (Path.cwd() / ".superpowers/fb7").resolve()
scratch_parent.mkdir(parents=True, exist_ok=True)
root = Path(subprocess.check_output(["mktemp", "-d", str(scratch_parent / "flea-search-mounts.XXXXXX")], text=True).strip())
marker = root / ".flea-test-sandbox"
marker.write_text("flea test sandbox\n")
try:
    run(sys.argv[1], root)
finally:
    assert str(root) and root.is_absolute() and root.name.startswith("flea-search-mounts.")
    assert root.parent == scratch_parent and marker.read_text() == "flea test sandbox\n"
    shutil.rmtree(root)
