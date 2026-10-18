"""Navigation must answer while rename execution or journal publication is held."""
import fcntl
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import time
from importlib.machinery import SourceFileLoader

Backend = SourceFileLoader("rename_timing", str(Path(__file__).with_name("rename-timing.py"))).load_module().Backend

# A distinct child status proves the kernel refused its nonblocking lock attempt.
CONTENDED_EXIT = 75


def witness_controls(root, shim):
    child = """import fcntl, sys
with open(sys.argv[1], 'r+') as lock:
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        sys.exit(int(sys.argv[2]))
"""
    for name, contended, matches in (("free", False, True), ("other", True, False), ("held", True, True)):
        folder = root / ("witness-" + name)
        folder.mkdir()
        path, entered = folder / "undo-journal.lock", folder / "entered"
        expected = path if matches else folder / "other.lock"
        with path.open("x+") as lock:
            if contended:
                fcntl.flock(lock, fcntl.LOCK_EX)
            result = subprocess.run([sys.executable, "-c", child, str(path), str(CONTENDED_EXIT)],
                env={"LD_PRELOAD": shim, "FLEA_JOURNAL_LOCK": str(expected), "FLEA_JOURNAL_ENTERED": str(entered)},
                stdin=subprocess.DEVNULL, capture_output=True, timeout=10)
            assert result.returncode == (CONTENDED_EXIT if contended else 0), (name, result.stderr)
            assert entered.exists() == (contended and matches), (name, "incorrect flock witness")
    print("RENAMELAG WITNESS PASS cases=3", flush=True)


def wait_for(predicate, message):
    deadline = time.monotonic() + 10
    while not predicate():
        assert time.monotonic() < deadline, message
        time.sleep(.005)


def case(binary, root, shim, mode):
    folder = root / mode
    folder.mkdir()
    source, target = folder / "before", folder / "after"
    source.write_text("payload")
    child = folder / "child"
    child.mkdir()
    (child / "inside").write_text("navigation")
    state = folder / "state"
    state.mkdir()
    entered, release = folder / "entered", folder / "release"
    env = {"LD_PRELOAD": shim}
    if mode == "syscall":
        env.update(FLEA_RENAME_ENTERED=str(entered), FLEA_RENAME_RELEASE=str(release))
    else:
        env.update(FLEA_JOURNAL_LOCK=str(state / "flea_undo_dir" / "undo-journal.lock"),
                   FLEA_JOURNAL_ENTERED=str(entered))
    previous = {key: os.environ.get(key) for key in env}
    os.environ.update(env)
    backend = Backend(binary, state)
    for key, value in previous.items():
        if value is None:
            del os.environ[key]
        else:
            os.environ[key] = value
    lock = None
    failed = None
    renamed = None
    seen = []
    try:
        control, _, _, _ = backend.listing(child)
        backend.listing(folder)
        if mode in ("journal", "journal-timeout"):
            lock = (state / "flea_undo_dir" / "undo-journal.lock").open("a+")
            fcntl.flock(lock, fcntl.LOCK_EX)
        assert source.is_absolute() and target.is_absolute() and root in source.parents and root in target.parents
        assert (root / ".flea-test-sandbox").is_file()
        started = backend.send(dict(c="rename", path=str(source), to=target.name))
        pipelined = backend.send(dict(c="list", path=str(child), first=20))
        wait_for(entered.exists, "rename did not reach the witnessed held phase")
        try:
            pipeline_seen = []
            while True:
                at, row = backend.messages.get(timeout=max(.001, pipelined + .6 - time.monotonic()))
                pipeline_seen.append(row)
                assert row["t"] not in ("error", "renamed", "eof"), row
                if row["t"] == "rows":
                    if at > pipelined + .6:
                        failed = "pipelined navigation exceeded 600 ms"
                    break
        except queue.Empty:
            failed = "pipelined navigation did not answer within 600 ms"
        navigation = backend.send(dict(c="list", path=str(child), first=20))
        deadline = time.monotonic() + .6
        try:
            while True:
                arrived, answer = backend.messages.get(timeout=max(.001, deadline - time.monotonic()))
                seen.append(answer)
                assert answer["t"] not in ("error", "renamed", "eof"), answer
                if answer["t"] == "rows":
                    assert any(row["t"] == "listed" and row["path"] == str(child) for row in seen), seen
                    assert [row["n"] for row in answer["rows"]] == ["inside"], answer
                    elapsed = (arrived - navigation) * 1000
                    break
                if time.monotonic() >= deadline:
                    raise queue.Empty
        except queue.Empty:
            failed = "navigation did not answer while hold was active"
        print(json.dumps(dict(case=mode, held_navigation_failure=failed)), flush=True)
        assert not release.exists(), "syscall was released before the assertion"
        if mode == "syscall":
            assert source.exists() and not target.exists(), "syscall hold was ineffective"
        else:
            assert target.exists() and lock is not None and entered.exists(), "journal hold was ineffective"
        if mode == "journal-timeout":
            elapsed_rename, renamed, _ = backend.until("renamed", started)
            assert 1900 <= elapsed_rename < 3500, elapsed_rename
            assert not release.exists() and lock is not None
            wait_for(lambda: any("stayed busy for 2 seconds" in line for line in backend.stderr), "missing bounded-lock diagnostic")
    finally:
        if mode == "syscall":
            release.touch(exist_ok=False)
        if lock:
            fcntl.flock(lock, fcntl.LOCK_UN)
            lock.close()
        try:
            if renamed is None:
                _, answer, _ = backend.until("renamed", started)
            else:
                answer = renamed
            assert answer["ok"] and answer["path"] == str(target), answer
            assert target.read_text() == "payload" and not source.exists()
            assert source.is_absolute() and target.is_absolute() and root in source.parents and root in target.parents
            assert (root / ".flea-test-sandbox").is_file()
            backend.send(dict(c="undo"))
            _, answer, _ = backend.until("undone", time.monotonic())
            assert answer["ok"] and answer["op"] == "rename", answer
            assert source.read_text() == "payload" and not target.exists()
            assert source.is_absolute() and target.is_absolute() and root in source.parents and root in target.parents
            assert (root / ".flea-test-sandbox").is_file()
            backend.send(dict(c="undo"))
            _, answer = backend.messages.get(timeout=5)
            while answer["t"] in ("changed", "fsinfo"):
                _, answer = backend.messages.get(timeout=5)
            assert answer["t"] == "error" and answer["where"] == "undo" and "nothing to undo" in answer["msg"], answer
        finally:
            backend.close()
    result = dict(case=mode, control_ms=control, navigation_ms=None if failed else elapsed, held=True, failure=failed)
    print(json.dumps(result), flush=True)
    return failed is None


binary, directory, shim = sys.argv[1:]
root = Path(directory)
assert directory and root.is_absolute() and root.resolve() == root
assert (root / ".flea-test-sandbox").is_file() and not root.is_relative_to(Path.home())
assert Path(shim).is_relative_to(root)
witness_controls(root, shim)
results = [case(binary, root, shim, mode) for mode in ("syscall", "journal", "journal-timeout")]
print(f"RENAMELAG DONE cases=3 failed={results.count(False)}")
sys.exit(0 if all(results) else 1)
