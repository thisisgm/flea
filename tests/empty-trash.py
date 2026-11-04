"""Check operation acknowledgement, progress, cancellation, and Trash pairing."""
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time

binary, directory = sys.argv[1:]
root = Path(directory)
assert root.is_absolute() and str(root) and (root / ".flea-test-sandbox").is_file()
assert Path(os.environ["XDG_DATA_HOME"]) == root / "data"
files, infos = root / "data/Trash/files", root / "data/Trash/info"
files.mkdir(parents=True)
infos.mkdir(parents=True)


def fixture(name):
    (files / name).touch(exist_ok=False)
    (infos / (name + ".trashinfo")).write_text(
        f"[Trash Info]\nPath={root}/original/{name}\nDeletionDate=2026-10-17T10:00:00\n"
    )


with (root / "backend.log").open("w") as log:
    backend = subprocess.Popen([binary, "--backend"], stdin=subprocess.PIPE,
                               stdout=subprocess.PIPE, stderr=log, text=True)
    messages = queue.Queue()

    def reader():
        for line in backend.stdout:
            messages.put(json.loads(line))

    threading.Thread(target=reader, daemon=True).start()

    def send(command):
        backend.stdin.write(json.dumps(command) + "\n")
        backend.stdin.flush()

    held = []

    def until(predicate, seconds=30):
        for index, message in enumerate(held):
            if predicate(message):
                return held.pop(index)
        deadline = time.monotonic() + seconds
        while True:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise queue.Empty
            message = messages.get(timeout=remaining)
            if predicate(message):
                return message
            if message.get("t") in ("listed", "transferdone", "trashbrowse", "jobs"):
                held.append(message)

    def request(op, **fields):
        send(dict(c="trashbrowse", op=op, id=42, **fields))
        return until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == op, 1800)

    try:
        fixture("one")
        payload = files / "one"
        assert payload.is_absolute() and str(payload) and payload.is_relative_to(root)
        payload.write_text("payload")
        prepared = request("prepare", all=True, emptyTrash=True)
        assert prepared["ok"] and prepared["count"] == 1 and prepared["bytes"] == 7, prepared
        started_at = time.monotonic()
        send(dict(c="trashbrowse", op="delete", id=43, token=prepared["token"]))
        first = until(lambda m: m.get("t") == "trashdeletestarted", 3)
        assert first["t"] == "trashdeletestarted", first
        operation = first["id"]
        progress = until(lambda m: m.get("t") == "transferprogress" and m["id"] == operation)
        assert progress["unit"] == "items" and progress["bytes"] == 0, progress
        print(f"small first_progress_ms={(time.monotonic() - started_at) * 1000:.3f}", flush=True)
        done = until(lambda m: m.get("t") == "transferdone" and m["id"] == operation)
        assert done["ok"] == 1 and not done["cancelled"], done
        assert not (files / "one").exists() and not (infos / "one.trashinfo").exists()
        until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "delete")
        tree = files / "tree"
        tree.mkdir()
        (tree / "a").write_text("abc")
        (tree / "b").write_text("de")
        (infos / "tree.trashinfo").write_text(
            f"[Trash Info]\nPath={root}/original/tree\nDeletionDate=2026-10-17T10:00:00\n"
        )
        prepared = request("prepare", all=True, emptyTrash=True)
        assert prepared["ok"] and prepared["count"] == 1 and prepared["bytes"] == 5, prepared
        send(dict(c="trashbrowse", op="delete", id=44, token=prepared["token"]))
        operation = until(lambda m: m.get("t") == "trashdeletestarted")["id"]
        named = until(lambda m: m.get("t") == "transferprogress" and m["id"] == operation and m["name"] == "tree")
        assert named["itemSize"] == 5 and named["total"] == 3 and named["bytes"] == 0, named
        final = until(lambda m: m.get("t") == "transferprogress" and m["id"] == operation and m["bytes"] == 3)
        assert final["total"] == 3 and final["unit"] == "items", final
        done = until(lambda m: m.get("t") == "transferdone" and m["id"] == operation)
        assert done["ok"] == 1 and not done["cancelled"], done
        assert not tree.exists() and not (infos / "tree.trashinfo").exists()
        until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "delete")
        print("nested progress=3/3 top_level_done=1 item=tree itemSize=5", flush=True)
        trash_samples = []
        originals = root / "original"
        originals.mkdir()
        for index in range(3):
            source = originals / f"incoming-{index}"
            source.write_text("payload")
            started_at = time.monotonic()
            send(dict(c="trash", paths=[str(source)]))
            started = until(lambda m: m.get("t") == "trashstarted")
            trash_samples.append((time.monotonic() - started_at) * 1000)
            assert started["n"] == 1 and not started["cancellable"], started
            trashed = until(lambda m: m.get("t") == "trashed")
            assert trashed["ok"] == 1 and not source.exists(), trashed
        prepared = request("prepare", all=True, emptyTrash=True)
        assert prepared["ok"] and prepared["count"] == 3 and prepared["bytes"] == 21, prepared
        send(dict(c="trashbrowse", op="delete", id=45, token=prepared["token"]))
        started = until(lambda m: m.get("t") == "trashdeletestarted")
        done = until(lambda m: m.get("t") == "transferdone" and m["id"] == started["id"])
        assert done["ok"] == 3, done
        until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "delete")
        print(f"trash first_ack_ms={min(trash_samples):.3f}..{max(trash_samples):.3f}", flush=True)
        for index in range(100000):
            fixture(f"item-{index:06d}")
        print("fixture=100000 marked_root=true isolated_xdg=true", flush=True)
        prepared = request("prepare", all=True, emptyTrash=True)
        assert prepared["ok"] and prepared["count"] == 100000 and prepared["bytes"] == 0, prepared
        samples = []
        determinate_samples = []
        for run in range(3):
            started_at = time.monotonic()
            send(dict(c="trashbrowse", op="delete", id=50 + run, token=prepared["token"]))
            first = until(lambda m: m.get("t") == "trashdeletestarted", 3)
            operation = first["id"]
            progress = until(lambda m: m.get("t") == "transferprogress" and m["id"] == operation)
            samples.append((time.monotonic() - started_at) * 1000)
            determinate = until(lambda m: m.get("t") == "transferprogress" and m["id"] == operation and m["total"] == 100000, 3)
            determinate_samples.append((time.monotonic() - started_at) * 1000)
            assert determinate["unit"] == "items" and determinate["bytes"] == 0, determinate
            send(dict(c="transfercancel", id=operation))
            send(dict(c="list", path=str(root), first=1))
            until(lambda m: m.get("t") == "listed", 3)
            done = until(lambda m: m.get("t") == "transferdone" and m["id"] == operation, 20)
            assert done["cancelled"] and done["ok"] == 0 and done["skipped"] == 100000, done
            until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "delete")
        remaining = {path.name for path in files.iterdir()}
        metadata = {path.name.removesuffix(".trashinfo") for path in infos.iterdir()}
        assert len(remaining) == 100000 and remaining == metadata
        assert max(samples) < 100, samples
        assert max(determinate_samples) < 100, determinate_samples
        print(f"100k first_progress_ms={min(samples):.3f}..{max(samples):.3f}", flush=True)
        print(f"100k determinate_progress_ms={min(determinate_samples):.3f}..{max(determinate_samples):.3f}", flush=True)
        send(dict(c="trashbrowse", op="check", id=70, token=prepared["token"]))
        time.sleep(0.2)
        send(dict(c="trashbrowse", op="delete", id=71, token=prepared["token"]))
        operation = until(lambda m: m.get("t") == "trashdeletestarted", 3)["id"]
        cancel_started_at = time.monotonic()
        send(dict(c="transfercancel", id=operation))
        send(dict(c="list", path=str(root), first=1))
        until(lambda m: m.get("t") == "listed", 3)
        try:
            done = until(lambda m: m.get("t") == "transferdone" and m["id"] == operation, 20)
        except queue.Empty:
            print("FAIL queued_cancel_seconds>20: periodic check still owns Trash worker after cancelling accepted delete", flush=True)
            done = until(lambda m: m.get("t") == "transferdone" and m["id"] == operation, 1800)
            print(f"eventual_queued_cancel_seconds={time.monotonic() - cancel_started_at:.3f}", flush=True)
            raise
        queued_cancel_seconds = time.monotonic() - cancel_started_at
        assert done["cancelled"] and done["ok"] == 0 and done["failed"] == 0 and done["skipped"] == 100000, done
        checked = until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "check" and m["id"] == 70)
        assert not checked["ok"], checked
        until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "delete")
        print(f"100k queued_cancel_seconds={queued_cancel_seconds:.3f} check_interrupted=true", flush=True)
        send(dict(c="trashbrowse", op="delete", id=60, token=prepared["token"]))
        started = until(lambda m: m.get("t") == "trashdeletestarted")
        operation = started["id"]
        progress = until(lambda m: m.get("t") == "transferprogress" and m["id"] == operation and m["phase"] == "deleting" and m["bytes"] > 0, 1800)
        assert progress["total"] == 100000 and progress["unit"] == "items", progress
        send(dict(c="jobs", op="list"))
        snapshot = until(lambda m: m.get("t") == "jobs" and any(j["id"] == operation and j["done"] > 0 for j in m["jobs"]))
        active = [j for j in snapshot["jobs"] if j["state"] in ("running", "waiting")]
        assert len(active) == 1 and snapshot["unfinished"] == 1, snapshot
        job = active[0]
        assert job["id"] == operation and job["kind"] == "emptyTrash" and job["head"] == "Emptying Trash", job
        assert progress["bytes"] <= job["done"] < job["total"] == 100000, job
        assert job["bytes"] is None and job["totalBytes"] is None and not job["undo"], job
        print(f"jobs snapshot kind=emptyTrash done={job['done']} total={job['total']} unfinished=1", flush=True)
        send(dict(c="jobs", op="cancel", id=operation))
        done = until(lambda m: m.get("t") == "transferdone" and m["id"] == operation, 20)
        assert done["cancelled"] and 0 < done["ok"] < 100000 and done["failed"] == 0, done
        assert done["ok"] + done["failed"] + done["skipped"] == 100000, done
        until(lambda m: m.get("t") == "trashbrowse" and m.get("op") == "delete")
        remaining = {path.name for path in files.iterdir()}
        metadata = {path.name.removesuffix(".trashinfo") for path in infos.iterdir()}
        assert len(remaining) == 100000 - done["ok"] and remaining == metadata
        print(f"100k deleting_cancel deleted={done['ok']} failed={done['failed']} skipped={done['skipped']} paired_survivors={len(remaining)}", flush=True)
        print("PASS empty-trash: completion, 100k determinate progress, cancel during validation and deletion, responsive listing, paired metadata")
    finally:
        send(dict(c="quit"))
        backend.stdin.close()
        backend.wait(timeout=20)
