"""Exercise rename and the lists its client sends, with isolated backend state."""
import argparse
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time


class Backend:
    def __init__(self, binary, root, test_binary=False):
        self.messages = queue.Queue()
        self.test_binary = test_binary
        self.trace = []
        self.stderr = []
        env = dict(os.environ)
        for key in ("XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_DATA_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "FLEA_UNDO_DIR"):
            directory = root / key.lower()
            directory.mkdir(mode=0o700)
            env[key] = str(directory)
        command = [binary, "--backend"]
        if test_binary:
            env["FLEA_RENAME_TIMING_CHILD"] = "1"
            command = [binary, "--exact", "backend::renametiming::tests::protocol_child", "--nocapture", "--quiet"]
        self.child = subprocess.Popen(command, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.error_reader = threading.Thread(target=self.read_errors, daemon=True)
        self.reader.start()
        self.error_reader.start()

    def read(self):
        for line in self.child.stdout:
            if self.test_binary:
                start = line.find('{"t":')
                if start < 0:
                    continue
                line = line[start:]
            self.messages.put((time.monotonic(), json.loads(line)))
        self.messages.put((time.monotonic(), {"t": "eof"}))

    def read_errors(self):
        for line in self.child.stderr:
            self.stderr.append(line)
            if line.startswith('{"rename_timing":true'):
                self.trace.append(json.loads(line))

    def send(self, request):
        started = time.monotonic()
        self.child.stdin.write(json.dumps(request) + "\n")
        self.child.stdin.flush()
        return started

    def until(self, kind, started):
        seen = []
        deadline = time.monotonic() + 20
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError(f"backend did not answer {kind} within 20 seconds")
            arrived, message = self.messages.get(timeout=remaining)
            seen.append((arrived, message))
            assert message["t"] not in ("error", "eof"), message
            if message["t"] == kind:
                return (arrived - started) * 1000, message, seen

    def listing(self, path, **options):
        started = self.send(dict(c="list", path=str(path), first=321, **options))
        elapsed, rows, seen = self.until("rows", started)
        listed = [message for _, message in seen if message["t"] == "listed"]
        assert len(listed) == 1 and listed[0]["path"] == str(path), seen
        return elapsed, listed[0], rows, seen

    def close(self):
        if self.child.poll() is None:
            self.send(dict(c="quit"))
        self.child.stdin.close()
        try:
            self.child.wait(timeout=30)
        except subprocess.TimeoutExpired:
            self.child.kill()
            self.child.wait(timeout=5)
            raise
        self.reader.join(timeout=2)
        self.error_reader.join(timeout=2)
        assert not self.reader.is_alive(), "backend stdout did not close"
        assert not self.error_reader.is_alive(), "backend stderr did not close"
        assert self.child.returncode == 0, self.child.returncode


def timing(trace, started, rename_ms):
    stages = {row["stage"]: row for row in trace if row["stage"] not in ("sample", "flock_begin", "flock_end")}
    pairs = {
        "request_queue": ("input_rename", "dispatch_rename"),
        "validation": ("dispatch_rename", "syscall_begin"),
        "syscall": ("syscall_begin", "syscall_end"),
        "identity": ("syscall_end", "journal_begin"),
        "journal": ("journal_begin", "journal_end"),
        "reply": ("journal_end", "reply_written"),
        "listing_patch": ("patch_begin", "patch_end"),
    }
    phases = {}
    for name, (first, last) in pairs.items():
        a, b = stages[first], stages[last]
        phases[name] = dict(wall_ms=(b["mono_ns"] - a["mono_ns"]) / 1e6)
        if a["tid"] == b["tid"]:
            phases[name].update(cpu_ms=(b["cpu_ns"] - a["cpu_ns"]) / 1e6, runqueue_ms=(b["runqueue_ns"] - a["runqueue_ns"]) / 1e6)
    phases["client_to_input_ms"] = stages["input_rename"]["mono_ns"] / 1e6 - started * 1000
    phases["reply_to_client_ms"] = started * 1000 + rename_ms - stages["reply_written"]["mono_ns"] / 1e6
    return phases


def run(binary, root, count, repeats, test_binary):
    folder = root / f"files-{count}"
    folder.mkdir()
    child = folder / "child"
    child.mkdir()
    with (child / "inside.txt").open("x") as stream:
        stream.write("child")
    for number in range(count):
        with (folder / f"f{number:06}.txt").open("x") as stream:
            stream.write("payload")
    for repeat in range(repeats):
        state = root / f"state-{count}-{repeat}"
        state.mkdir()
        backend = Backend(binary, state, test_binary)
        try:
            initial, listed, _, _ = backend.listing(folder)
            assert listed["n"] == count + 1, listed
            control, _, control_rows, _ = backend.listing(child)
            assert [row["n"] for row in control_rows["rows"]] == ["inside.txt"], control_rows
            ordinary_relist, _, _, _ = backend.listing(folder)
            source = folder / f"f{repeat:06}.txt"
            target = folder / f"renamed-{repeat}.txt"
            assert source.is_absolute() and target.is_absolute() and root in source.parents and root in target.parents
            assert (root / ".flea-test-sandbox").is_file()
            started = backend.send(dict(c="rename", path=str(source), to=target.name))
            rename, answer, seen = backend.until("renamed", started)
            assert answer["ok"] and answer["path"] == str(target), answer
            assert not any(message["t"] in ("listed", "rows") for _, message in seen), seen
            assert not source.exists() and target.read_text() == "payload"
            refresh, listed, refreshed_rows, refresh_seen = backend.listing(folder, select=str(target))
            assert listed["n"] == count + 1, listed
            assert target.name in [row["n"] for row in refreshed_rows["rows"]], refreshed_rows
            refresh_read = listed["read"]
            refresh_sort = listed["sort"]
            navigate, listed, navigated_rows, navigation_seen = backend.listing(child)
            assert listed["n"] == 1, listed
            assert [row["n"] for row in navigated_rows["rows"]] == ["inside.txt"], navigated_rows
            changes = [(at - started) * 1000 for at, message in seen + refresh_seen + navigation_seen if message["t"] == "changed"]
            result = dict(files=count, run=repeat + 1, initial_list_ms=initial, ordinary_relist_ms=ordinary_relist, rename_ms=rename, refresh_ms=refresh, refresh_read_ms=refresh_read, refresh_sort_ms=refresh_sort, navigate_ms=navigate, control_ms=control, changed_ms=changes)
        finally:
            backend.close()
        if test_binary:
            result["phases"] = timing(backend.trace, started, rename)
            result["trace"] = backend.trace
        print(json.dumps(result), flush=True)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("binary")
    parser.add_argument("directory")
    parser.add_argument("--runs", type=int, default=3)
    parser.add_argument("--files", type=int, nargs="+", default=[400, 100000])
    parser.add_argument("--test-binary", action="store_true")
    args = parser.parse_args()
    binary, directory = args.binary, args.directory
    root = Path(directory)
    assert directory and root.is_absolute() and root.resolve() == root
    assert (root / ".flea-test-sandbox").is_file()
    assert not root.is_relative_to(Path.home())
    assert 0 < args.runs <= min(args.files)
    for count in args.files:
        run(binary, root, count, args.runs, args.test_binary)
    print(f"RENAMELAG DONE runs={args.runs * len(args.files)} failed=0")


if __name__ == "__main__":
    main()
