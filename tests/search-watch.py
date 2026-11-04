"""Standing-query recursive freshness through the shipping JSON-lines executable."""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time


def run(binary, sandbox):
    tree = sandbox / "tree"
    (tree / "deep" / "child").mkdir(parents=True)
    (tree / "other").mkdir()
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
    threading.Thread(target=read, daemon=True).start()
    def send(**request):
        process.stdin.write(json.dumps(request) + "\n")
        process.stdin.flush()
    def checked(path):
        assert path.is_absolute() and path != sandbox and path.is_relative_to(sandbox)
        assert (sandbox / ".flea-test-sandbox").is_file()
        return path
    def answer(expected, identity=None, timeout=10):
        deadline = time.monotonic() + timeout
        rows = None
        while True:
            line = lines.get(timeout=max(0.001, deadline - time.monotonic()))
            assert line is not None, "backend closed"
            assert line.get("t") != "error", line
            if line.get("t") == "rows":
                rows = line["rows"]
            if line.get("t") == "searched" and not line["walking"]:
                assert line["gen"] == 41, line
                assert not line["partial"], line
                if line["n"] == expected and (identity is None or (rows and rows[0]["n"] == identity)):
                    return
            assert time.monotonic() < deadline, (expected, line)
    def mutate(action, expected, identity=None):
        started = time.perf_counter_ns()
        action()
        answer(expected, identity)
        timings.append((time.perf_counter_ns() - started) / 1e6)
    timings = []
    try:
        send(c="searchopen", path=str(tree), folder=str(tree), hidden=False)
        send(c="searchquery", query="needle", gen=41, count=3, sizes=0)
        answer(0)
        mutate(lambda: (tree / "deep/child/needle-a").write_text("a"), 1)
        mutate(lambda: checked(tree / "deep/child/needle-a").rename(checked(tree / "other/needle-b")), 1, "other/needle-b")
        send(c="paths", rows=[0])
        while True:
            reply = lines.get(timeout=10)
            if reply.get("t") == "paths":
                assert reply["paths"] == [str(tree / "other/needle-b")], reply
                break
        mutate(lambda: checked(tree / "other/needle-b").unlink(), 0)
        moved = sandbox / "incoming"
        moved.mkdir()
        (moved / "needle-c").write_text("c")
        mutate(lambda: checked(moved).rename(checked(tree / "deep/incoming")), 1)
        mutate(lambda: checked(tree / "deep/incoming").rename(checked(tree / "other/relocated")), 1, "other/relocated/needle-c")
        mutate(lambda: checked(tree / "other/relocated").rename(checked(sandbox / "outgoing")), 0)
        mutate(lambda: (tree / "needle-root").write_text("r"), 1)
        mutate(lambda: checked(tree / "needle-root").rename(checked(tree / "plain")), 0)
        (tree / "replace").mkdir()
        mutate(lambda: (tree / "replace/needle-old").write_text("o"), 1)
        replacement = sandbox / "replacement"
        replacement.mkdir()
        (replacement / "needle-new").write_text("n")
        def replace():
            checked(tree / "replace").rename(checked(sandbox / "old"))
            checked(replacement).rename(checked(tree / "replace"))
        mutate(replace, 1, "replace/needle-new")
        send(c="paths", rows=[0])
        while True:
            reply = lines.get(timeout=10)
            if reply.get("t") == "paths":
                assert reply["paths"] == [str(tree / "replace/needle-new")], reply
                break
        mutate(lambda: checked(tree / "replace/needle-new").unlink(), 0)
        mutate(lambda: (tree / "needle-kind").mkdir(), 1)
        mutate(lambda: (tree / "needle-kind/needle-child").write_text("k"), 2)
        def dir_to_file():
            checked(tree / "needle-kind/needle-child").unlink()
            checked(tree / "needle-kind").rmdir()
            (tree / "needle-kind").write_text("f")
        mutate(dir_to_file, 1, "needle-kind")
        def file_to_dir():
            checked(tree / "needle-kind").unlink()
            (tree / "needle-kind").mkdir()
            (tree / "needle-kind/needle-child").write_text("d")
        mutate(file_to_dir, 2)
        mutate(lambda: shutil.rmtree(checked(tree / "needle-kind")), 0)
        send(c="searchclose")
        send(c="quit")
        process.wait(timeout=10)
        assert process.returncode == 0, process.stderr.read()
        print(f"SEARCH_WATCH mutations={len(timings)} visibility_ms={min(timings):.3f}..{max(timings):.3f} standing_gen=41")
    finally:
        if process.poll() is None:
            process.kill()
            process.wait(timeout=10)


with tempfile.TemporaryDirectory(prefix="flea-search-watch-") as name:
    sandbox = Path(name).resolve()
    assert sandbox.is_absolute() and not sandbox.is_relative_to(Path.home())
    (sandbox / ".flea-test-sandbox").write_text("search-watch\n")
    run(sys.argv[1], sandbox)
