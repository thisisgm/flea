#!/usr/bin/env python3
"""A Flea previewer: answers each preview request with a torrent's name, size and files as plain text."""
import json
import sys

MAX_FILES = 200
MAX_INPUT = 8 << 20
MAX_MESSAGE = 64 * 1024


# Sample input: b"d4:infod4:name7:fixture6:lengthi1eee".
def decode(data, at=0):
    """Decodes one bencoded value at `at`, returning it and the offset after it."""
    lead = data[at:at + 1]
    if lead == b"i":
        end = data.index(b"e", at)
        return int(data[at + 1:end]), end + 1
    if lead == b"l":
        items, at = [], at + 1
        while data[at:at + 1] != b"e":
            item, at = decode(data, at)
            items.append(item)
        return items, at + 1
    if lead == b"d":
        table, at = {}, at + 1
        while data[at:at + 1] != b"e":
            key, at = decode(data, at)
            table[key], at = decode(data, at)
        return table, at + 1
    if lead.isdigit():
        colon = data.index(b":", at)
        size = int(data[at:colon])
        return data[colon + 1:colon + 1 + size], colon + 1 + size
    raise ValueError("not a torrent")


def text(value):
    return value.decode("utf-8", "replace") if isinstance(value, bytes) else str(value)


def size(n):
    for unit in ("bytes", "KB", "MB", "GB", "TB"):
        if n < 1000 or unit == "TB":
            return f"{n} {unit}" if unit == "bytes" else f"{n:.1f} {unit}"
        n /= 1000


def describe(path):
    with open(path, "rb") as file:
        data = file.read(MAX_INPUT)
    meta, _ = decode(data)
    info = meta.get(b"info", {})
    files = info.get(b"files") or [{b"length": info.get(b"length", 0), b"path": [info.get(b"name", b"")]}]
    total = sum(int(f.get(b"length", 0)) for f in files)
    lines = [text(info.get(b"name", b"")), f"{len(files)} files, {size(total)}"]
    if b"announce" in meta:
        lines.append(text(meta[b"announce"]))
    lines.append("")
    for entry in files[:MAX_FILES]:
        lines.append(f"{'/'.join(text(p) for p in entry.get(b'path', []))}  {size(int(entry.get(b'length', 0)))}")
    return "\n".join(lines)


def answer(message):
    encoded = json.dumps(message)
    if message.get("op") == "preview" and len(encoded.encode("utf-8")) > MAX_MESSAGE:
        text = message["text"]
        suffix = "\n[Preview truncated]"
        low, high = 0, len(text)
        while low < high:
            middle = (low + high + 1) // 2
            candidate = json.dumps({**message, "text": text[:middle] + suffix})
            if len(candidate.encode("utf-8")) <= MAX_MESSAGE:
                low = middle
            else:
                high = middle - 1
        encoded = json.dumps({**message, "text": text[:low] + suffix})
    sys.stdout.write(encoded + "\n")
    sys.stdout.flush()


for line in sys.stdin:
    request = json.loads(line)
    op = request.get("op")
    if op == "hello":
        answer({"op": "ready", "protocolVersion": 1})
    elif op == "preview":
        try:
            answer({"op": "preview", "text": describe(request["path"])})
        except (OSError, ValueError, KeyError, IndexError, AttributeError, TypeError, OverflowError, RecursionError):
            answer({"op": "error", "message": "This torrent could not be read."})
    else:
        answer({"op": "error", "message": "Unsupported request."})
