"""Identity coverage and partial status through the production JSON-lines executable."""
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import threading


class Backend:
    def __init__(self, binary, sandbox):
        env = dict(os.environ)
        for key in ("XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR", "XDG_DATA_HOME"):
            directory = sandbox / key.lower()
            directory.mkdir(mode=0o700, exist_ok=True)
            env[key] = str(directory)
        self.process = subprocess.Popen([binary, "--backend"], stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                                        text=True, env=env)
        self.lines = queue.Queue()
        def read():
            for line in self.process.stdout:
                self.lines.put(json.loads(line))
            self.lines.put(None)
        self.reader = threading.Thread(target=read, daemon=True)
        self.reader.start()

    def send(self, **request):
        self.process.stdin.write(json.dumps(request) + "\n")
        self.process.stdin.flush()

    def until(self, predicate):
        while True:
            line = self.lines.get(timeout=90)
            assert line is not None, "backend ended before expected reply"
            assert line.get("t") != "error", line
            if predicate(line):
                return line

    def close(self):
        if self.process.poll() is None:
            self.send(c="quit")
        self.process.wait(timeout=15)
        self.reader.join(timeout=5)
        assert self.process.returncode == 0, self.process.stderr.read()


def digest(paths):
    value = hashlib.sha256()
    for path in sorted(paths):
        value.update(path.encode() + b"\0file\n")
    return value.hexdigest()


def run(binary, sandbox):
    tree = sandbox / "tree"
    tree.mkdir()
    expected = []
    for directory in range(300):
        child = tree / f"d{directory:03}"
        child.mkdir()
        for entry in range(directory * 1000, (directory + 1) * 1000):
            relative = f"d{directory:03}/f{entry:06}.txt"
            fd = os.open(tree / relative, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
            os.close(fd)
            expected.append(relative)
    backend = Backend(binary, sandbox)
    try:
        backend.send(c="searchopen", path=str(tree), folder=str(tree), hidden=False)
        backend.until(lambda line: line.get("t") == "searchopened")
        backend.send(c="searchquery", query="/", gen=1, count=3, sizes=0)
        done = backend.until(lambda line: line.get("t") == "searched" and not line["walking"])
        assert done["n"] == 300000 and done["scanned"] == 300300, done
        assert done["partial"] is False and done["partial_reason"] == "", done
        backend.send(c="window", start=0, count=3)
        window = backend.until(lambda line: line.get("t") == "rows")
        assert len(window["rows"]) == 3 and all(row["p"] != 0 for row in window["rows"]), window
        actual = []
        for start in range(0, 300000, 4096):
            end = min(start + 4096, 300000)
            backend.send(c="paths", rows=list(range(start, end)))
            reply = backend.until(lambda line: line.get("t") == "paths")
            assert len(reply["paths"]) == end - start, (start, reply)
            actual.extend(str(Path(path).relative_to(tree)) for path in reply["paths"])
        assert len(set(actual)) == 300000, "missing or duplicated identity"
        assert digest(actual) == digest(expected), "inventory differs"
        print(f"SEARCH_PROTOCOL files=300000 scanned=300300 sha256={digest(actual)} rich_rows=3 partial=false")
        backend.send(c="searchclose")
        backend.until(lambda line: line.get("t") == "searchclosed")
        missing = str(sandbox / "missing")
        backend.send(c="searchopen", path=missing, folder=str(tree), hidden=False)
        backend.until(lambda line: line.get("t") == "searchopened")
        backend.send(c="searchquery", query="anything", gen=2, count=0)
        refused = backend.until(lambda line: line.get("t") == "searched" and not line["walking"])
        assert refused["partial"] is True and refused["partial_reason"] == "unreadable", refused
        assert refused["n"] == 0 and refused["scanned"] == 0, refused
        print("SEARCH_PROTOCOL missing-root partial=true reason=unreadable")
    finally:
        backend.close()


scratch_parent = (Path.cwd() / ".superpowers" / "fb1").resolve()
scratch_parent.mkdir(parents=True, exist_ok=True)
root = Path(subprocess.check_output(["mktemp", "-d", str(scratch_parent / "flea-search-coverage.XXXXXX")], text=True).strip())
marker = root / ".flea-test-sandbox"
marker.write_text("flea test sandbox\n")
try:
    run(sys.argv[1], root)
finally:
    assert str(root) and root.is_absolute() and root.name.startswith("flea-search-coverage.")
    assert root.parent == scratch_parent and marker.read_text() == "flea test sandbox\n"
    shutil.rmtree(root)
