#!/usr/bin/env python3
"""Verify selected files against the checksum entered in Flea's native input card."""
import hashlib
import json
import os
import sys


def answer(value):
    print(json.dumps(value), flush=True)


for line in iter(lambda: sys.stdin.buffer.readline(65538), b""):
    if len(line) > 65536:
        break
    try:
        request = json.loads(line)
        operation = request.get("op")
        if operation == "hello":
            answer({"op": "ready", "protocolVersion": 1})
        elif operation == "run" and request.get("action") == "sha256":
            paths = request.get("paths", [])
            if not paths or len(paths) > 512:
                raise ValueError("Choose between 1 and 512 files.")
            expected = request.get("input", {}).get("checksum", "").strip().lower()
            if len(expected) != 64 or any(char not in "0123456789abcdef" for char in expected):
                raise ValueError("Enter a SHA-256 checksum of 64 hexadecimal characters.")
            sums = []
            report_every = max(1, (len(paths) + 127) // 128)
            for completed, path in enumerate(paths, 1):
                if not isinstance(path, str) or not os.path.isabs(path):
                    raise ValueError("Expected an absolute selected file path.")
                with open(path, "rb") as selected:
                    digest = hashlib.file_digest(selected, "sha256").hexdigest()
                sums.append(digest)
                if completed == len(paths) or completed % report_every == 0:
                    answer({"op": "progress", "done": completed, "total": len(paths),
                            "message": "Verifying checksum."})
            if any(digest != expected for digest in sums):
                raise ValueError("Checksum does not match.")
            answer({"op": "done", "message": "Checksum matches."})
        elif operation == "settings":
            answer({"op": "done"})
        else:
            answer({"op": "error", "message": "Unsupported action."})
    except (ValueError, OSError, TypeError) as error:
        answer({"op": "error", "message": str(error)[:4096]})
