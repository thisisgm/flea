"""Derive right-anchored status cells only after independently verifying disk facts."""
import json
import os
import re
import selectors
import signal
import subprocess
import time

from .cells import width
from .evidence import Blocked

BACKEND_EVIDENCE_BYTES = 4 * 1024 * 1024
BACKEND_QUIT_SECONDS = 3

# Sample input: ext4 · 5.0 kB free; ext4 · 空き 5.0 kB; ext4 · 5.0 kB متاحة; ext4 · свободно 5,0 kB.
FS = r"\b(?P<fs>[A-Za-z0-9_.+-]+) · "
SIZE = r"(?P<size>\d+(?:[.,٫]\d)? (?:B|kB|MB|GB|TB))"
DISK = [re.compile(FS + recipe) for recipe in
        (SIZE + r" free", r"空き " + SIZE, SIZE + r" (?:ةحاتم|متاحة)", r"свободно " + SIZE)]


# Sample input: [{"ch":"e"},{"ch":"x"},{"ch":"t"},{"ch":"4"},{"ch":" · 5.0 kB free"}].
def disk_field(row):
    chars, columns = [], []
    for x, cell in enumerate(row):
        chars.extend(cell["ch"])
        columns.extend([x] * len(cell["ch"]))
    columns.append(len(row))
    for language, pattern in zip(("en", "ja", "ar", "ru"), DISK):
        match = pattern.search("".join(chars))
        if match:
            number = match["size"].replace(",", ".").replace("٫", ".")
            number = "".join(str(int(c)) if c.isdecimal() else c for c in number)
            return {"fs": match["fs"], "size": number, "raw_size": match["size"], "text": match[0],
                    "labels": [match.string[match.end("fs"):match.start("size")],
                               match.string[match.end("size"):match.end()]],
                    "comma": language == "ru",
                    "tokens": {name: [columns[match.start(name)], columns[match.end(name)]] for name in ("fs", "size")},
                    "left": columns[match.start()], "right": columns[match.end()]}
    return None


def size(value):
    units = ("B", "kB", "MB", "GB", "TB")
    index = 0
    while value >= 1000 and index < len(units) - 1:
        value /= 1000
        index += 1
    return f"{value:.1f} {units[index]}" if index else f"{int(value)} B"


# Sample input: b'{"t":"listed"}\n{"t":"fsinfo","path":"/fx/v/work","fs":"ext4","free":5000}\n'.
def fsinfo(candidate, cwd, commands, timeout=8):
    index = len(candidate) - 3
    if candidate[index] != "/opt/flea":
        raise Blocked("unexpected namespace candidate argv")
    argv = candidate[:index + 1] + ["--backend"]
    receipt = {"argv": argv, "start_ns": time.time_ns(), "stdin": []}
    commands.records.append(receipt)
    process = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    result, output, errors, pending = None, bytearray(), bytearray(), bytearray()
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ)
    selector.register(process.stderr, selectors.EVENT_READ)
    def request(value):
        receipt["stdin"].append(value)
        process.stdin.write(json.dumps(value).encode() + b"\n")
        process.stdin.flush()
    try:
        request({"c": "list", "path": cwd, "first": 0, "hidden": False})
        deadline = time.monotonic() + timeout
        quitting = False
        while time.monotonic() < deadline:
            for key, _ in selector.select(.05):
                chunk = os.read(key.fileobj.fileno(), 65536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                if key.fileobj == process.stderr:
                    errors.extend(chunk)
                else:
                    output.extend(chunk)
                    pending.extend(chunk)
                    while b"\n" in pending:
                        line, _, tail = pending.partition(b"\n")
                        pending = bytearray(tail)
                        message = json.loads(line)
                        if message.get("t") == "listed" and result is None:
                            request({"c": "fsinfo"})
                        if message.get("t") == "fsinfo" and message.get("path") == cwd:
                            result = message
                if len(output) + len(errors) > BACKEND_EVIDENCE_BYTES:
                    raise Blocked("backend fact evidence exceeds 4 MiB")
            if result is not None and not quitting:
                request({"c": "quit"})
                process.stdin.close()
                quitting = True
                deadline = time.monotonic() + BACKEND_QUIT_SECONDS
            if process.poll() is not None and not selector.get_map():
                break
        if result is None:
            raise Blocked("backend did not answer its actual filesystem facts")
        if process.poll() is None or selector.get_map():
            raise Blocked("filesystem fact backend failed to quit and drain its output")
        code = process.returncode
        if code:
            raise Blocked("filesystem fact backend exited nonzero")
        return result
    except subprocess.TimeoutExpired as error:
        raise Blocked("filesystem fact backend failed to quit") from error
    finally:
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
        selector.close()
        receipt.update(exit_code=process.returncode, end_ns=time.time_ns(),
                       stdout=output.decode(errors="replace"), stderr=errors.decode(errors="replace"))
        for stream in (process.stdin, process.stdout, process.stderr):
            if stream and not stream.closed:
                stream.close()


def masks(board, snapshot, candidate, cwd, commands):
    cells = board.cells(snapshot["columns"], snapshot["rows"])
    y = snapshot["rows"] - 1
    expected_match = disk_field([cells[(x, y)] for x in range(snapshot["columns"])])
    actual_match = disk_field(snapshot["grid"][y])
    if not expected_match:
        return [], None
    if not actual_match:
        raise Blocked("declared disk field disappeared from native status row")
    if actual_match["labels"] != expected_match["labels"]:
        raise Blocked("localized disk labels differ from the reference")
    reply = fsinfo(candidate, cwd, commands)
    formatted = size(reply["free"])
    localized_size = formatted.replace(".", ",") if expected_match["comma"] else formatted
    if (actual_match["fs"], actual_match["size"]) != (reply["fs"], formatted) or actual_match["raw_size"] != localized_size:
        raise Blocked("native disk facts differ from the real backend reply")
    # The board's fixed hint suffix ends at COLS - 1; only the disk tail start moves.
    columns = snapshot["columns"]
    suffix_width = columns - 1 - expected_match["right"]
    right = columns - 1 - suffix_width
    text = reply["fs"] + expected_match["labels"][0] + localized_size + expected_match["labels"][1]
    left = right - sum(width(ch) for ch in text)
    if left < 0 or any(cells[x, y]["ch"].strip() for x in range(left, expected_match["left"])):
        raise Blocked("verified disk tail overlaps fixed status content")
    start = min(left, expected_match["left"])
    style = {k: v for k, v in cells[expected_match["left"], y].items() if k not in ("ch", "width")}
    derived = {x: dict(style, ch=" ", width=1) for x in range(start, right)}
    x = left
    for ch in text:
        w = width(ch)
        derived[x] = dict(style, ch=ch, width=w)
        if w == 2:
            derived[x + 1] = dict(style, ch="", width=0)
        x += w
    field = [start, y, right - start, 1]
    expectation = {"kind": "verified-disk-text", "field": field,
                   "cells": [{"cell": [x, y], "expected": cell} for x, cell in derived.items()]}
    return [expectation], {"raw_backend": reply, "observed": actual_match["text"],
                           "expected": text, "anchor": [left, right], "mask": expectation}
