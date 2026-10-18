#!/usr/bin/env python3
"""Exercise glycin's large-image decode through the real backend and shared cache."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import zlib


def make_png(path):
    with path.open("xb") as output:
        def chunk(kind, body):
            output.write(struct.pack(">I", len(body)) + kind + body)
            output.write(struct.pack(">I", zlib.crc32(kind + body) & 0xffffffff))
        output.write(b"\x89PNG\r\n\x1a\n")
        chunk(b"IHDR", struct.pack(">IIBBBBB", 16000, 16000, 8, 2, 0, 0, 0))
        encoder = zlib.compressobj(1)
        row = b"\x00" + b"\x2b\x58\x76" * 16000
        for _ in range(16000):
            block = encoder.compress(row)
            if block:
                chunk(b"IDAT", block)
        chunk(b"IDAT", encoder.flush())
        chunk(b"IEND", b"")


def main():
    binary, parent = Path(sys.argv[1]).resolve(strict=True), Path(sys.argv[2])
    assert parent.is_absolute() and (parent / ".flea-test-sandbox").is_file()
    assert parent == parent.resolve(strict=True), "the test parent must be canonical"
    assert not parent.is_relative_to(Path.home().resolve(strict=True)), "tests must not write inside HOME"
    root = Path(tempfile.mkdtemp(prefix="glycin-limit.", dir=parent))
    (root / ".flea-test-sandbox").touch(exist_ok=False)
    decoder = shutil.which("glycin-thumbnailer")
    assert decoder, "glycin-thumbnailer is required for the large-image regression"
    files = root / "files"
    files.mkdir()
    image = files / "large.png"
    make_png(image)
    data = root / "data" / "thumbnailers"
    data.mkdir(parents=True)
    entries = [path.read_text() for directory in ["/usr/share/thumbnailers", "/usr/local/share/thumbnailers"]
               for path in Path(directory).glob("*.thumbnailer")]
    entry = next((text for text in entries if any(line.startswith("Exec=") and "glycin-thumbnailer" in line
                 for line in text.splitlines()) and "image/png" in text), None)
    assert entry, "the installed glycin thumbnailer registration is required"
    print("large PNG decoder: " + next(line for line in entry.splitlines() if line.startswith("Exec=")), flush=True)
    with (data / "glycin.thumbnailer").open("x") as output:
        output.write(entry)
    timings = []
    for index in range(3):
        run = root / str(index)
        run.mkdir()
        environment = dict(os.environ, HOME=str(run), XDG_CACHE_HOME=str(run / "cache"),
                           XDG_STATE_HOME=str(run / "state"), XDG_CONFIG_HOME=str(run / "config"),
                           XDG_RUNTIME_DIR=str(run / "runtime"), XDG_DATA_HOME=str(root / "data"),
                           TMPDIR=str(run), FLEA_THUMB_TRACE="1", FLEA_THUMB_WORKER="off")
        (run / "runtime").mkdir(mode=0o700)
        requests = [{"c": "list", "path": str(files), "first": 10},
                    {"c": "thumb", "rows": [0]}, {"c": "quit"}]
        started = time.monotonic()
        result = subprocess.run([str(binary), "--backend"], env=environment,
                                input="".join(json.dumps(row) + "\n" for row in requests),
                                capture_output=True, text=True, timeout=90)
        elapsed = time.monotonic() - started
        print(f"large PNG run {index + 1}: backend={result.returncode} elapsed={elapsed:.3f}s", flush=True)
        print(result.stderr.strip(), flush=True)
        assert result.returncode == 0, result.stderr
        rows = [json.loads(line) for line in result.stdout.splitlines()]
        replies = [row for row in rows if row.get("t") == "thumbed"]
        assert len(replies) == 1 and replies[0].get("row") == 0, result.stdout
        assert replies[0].get("file"), "valid 16000x16000 PNG was refused: " + result.stdout
        published = Path(replies[0]["file"])
        key = hashlib.md5(image.as_uri().encode()).hexdigest() + ".png"
        assert published == run / "cache" / "thumbnails" / "large" / key, published
        png = published.read_bytes()
        assert png[:8] == b"\x89PNG\r\n\x1a\n"
        assert struct.unpack(">II", png[16:24]) == (256, 256)
        assert b"Thumb::URI\x00" + image.as_uri().encode() in png
        assert not (run / "cache" / "thumbnails" / "fail" / "flea" / key).exists()
        assert not list(published.parent.glob(".flea-*")), "thumbnail temp survived publish"
        timings.append(elapsed)
    print(f"large PNG: 3 valid 256px thumbnails, elapsed={min(timings):.3f}..{max(timings):.3f}s", flush=True)


if __name__ == "__main__":
    main()
