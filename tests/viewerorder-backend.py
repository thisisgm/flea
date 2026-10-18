#!/usr/bin/env python3
"""Check the real backend, opener and GIO field-code expansion in a marked root."""
import base64
import json
import os
from pathlib import Path
import subprocess
import sys
import time
from urllib.parse import unquote, urlparse


def owned(root, path):
    assert root.is_absolute() and (root / ".flea-test-sandbox").is_file()
    assert path.is_absolute() and path != root and path.is_relative_to(root)
    assert not path.is_symlink() and path.resolve().is_relative_to(root.resolve())
    return path


def write(root, path, body):
    owned(root, path).write_text(body)


def record():
    root = Path(os.environ["VIEWERORDER_ROOT"])
    path = owned(root, Path(os.environ["VIEWERORDER_RECEIPT"]))
    with path.open("x") as receipt:
        json.dump(sys.argv[2:], receipt)


def escaped_exec_controls(binary, root, env, desktop, viewer, paths):
    cases = [
        ("space", r'"a\sb"', "a b"),
        ("newline", r'"a\nb"', "a\nb"),
        ("tab", r'"a\tb"', "a\tb"),
        ("carriage-return", r'"a\rb"', "a\rb"),
        ("backslash", r'"a\\\\b"', "a\\b"),
        ("escaped-quote", r'"a\\"b"', 'a"b'),
        ("literal-backslash-s", r'"a\\\\sb"', r'a\sb'),
    ]
    for label, argument, expected in cases:
        for code in ["F", "U"]:
            write(root, desktop, f'[Desktop Entry]\nType=Application\nName=Viewer order\nX-GIO-NoFuse=true\nExec="{viewer}" {argument} %{code}\n')
            received = []
            for mode in ["direct", "flea"]:
                receipt = owned(root, root / f"escape-{label}-{code}-{mode}.json")
                env["VIEWERORDER_RECEIPT"] = str(receipt)
                launcher = ["gio", "launch", str(desktop)] if mode == "direct" else [binary, "--open"]
                result = subprocess.run([*launcher, *paths], env=env, cwd=root,
                                        capture_output=True, text=True, timeout=20)
                assert result.returncode == 0, (label, code, mode, result.returncode, result.stderr)
                deadline = time.monotonic() + 5
                while True:
                    try:
                        args = json.loads(receipt.read_text())
                        break
                    except (FileNotFoundError, json.JSONDecodeError):
                        assert time.monotonic() < deadline, (label, code, mode, "no completed receipt")
                        time.sleep(0.01)
                assert args[0] == expected, (label, code, mode, args)
                image_args = args[1:]
                if code == "U":
                    assert all(path.startswith("file://") for path in image_args), image_args
                    image_args = [unquote(urlparse(path).path) for path in image_args]
                assert image_args == paths, (label, code, mode, image_args, paths)
                received.append(args)
            assert received[0] == received[1], (label, code, received)
            print(f"viewerorder-backend: escape {label} %{code}, direct GIO and Flea agree on both images", flush=True)


def run(binary, root):
    for name in ["home", "data", "data/applications", "config", "cache", "state", "runtime", "images"]:
        owned(root, root / name).mkdir(mode=0o700)
    owned(root, root / "data/mime").symlink_to("/usr/share/mime", target_is_directory=True)
    env = dict(os.environ, HOME=str(root / "home"), XDG_DATA_HOME=str(root / "data"),
               XDG_DATA_DIRS=str(root / "data"), XDG_CONFIG_HOME=str(root / "config"),
               XDG_CONFIG_DIRS=str(root / "config"), XDG_CACHE_HOME=str(root / "cache"),
               XDG_STATE_HOME=str(root / "state"), XDG_RUNTIME_DIR=str(root / "runtime"),
               VIEWERORDER_ROOT=str(root), LC_ALL="C")
    picture = base64.b64decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aN1sAAAAASUVORK5CYII=")
    images = root / "images"
    names = [f"image-{index:04}.png" for index in range(1000)]
    for name in [*names, ".hidden.png"]:
        with owned(root, images / name).open("xb") as output:
            output.write(picture)
    write(root, images / "notes.txt", "not an image\n")
    owned(root, images / "folder.png").mkdir()
    galleries = []
    for descending, hidden in [(False, False), (True, False), (False, True)]:
        ordered_names = sorted([*names, "notes.txt", *([".hidden.png"] if hidden else [])], reverse=descending)
        listing = [str(images / "folder.png"), *[str(images / name) for name in ordered_names]]
        start = listing.index(str(images / "image-0500.png"))
        marked = [listing.index(str(images / name)) for name in ["image-0900.png", "image-0002.png", "image-0500.png"]]
        scope = sorted(marked)
        commands = [
            dict(c="list", path=str(images), first=0, by="name", desc=descending, hidden=hidden),
            dict(c="paths", rows=list(range(len(listing))), listing=1),
            dict(c="paths", images=start, listing=1),
            dict(c="paths", images=start, listing=1, scoped=True, rows=marked),
            dict(c="paths", images=scope[0], listing=1, scoped=True, rows=[*marked, marked[0]], selected=True),
            dict(c="paths", images=start, listing=1, scoped=True, rows=[]),
            dict(c="paths", images=start, listing=0),
            dict(c="quit"),
        ]
        result = subprocess.run([binary, "--backend"], input="".join(json.dumps(row) + "\n" for row in commands),
                                env=env, cwd=root, capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, result.stderr
        replies = [json.loads(line) for line in result.stdout.splitlines()]
        assert not [row for row in replies if row["t"] == "error" and row.get("where") != "stale"], replies
        paths = [row["paths"] for row in replies if row["t"] == "paths"]
        expected = [path for path in listing[start:] + listing[:start] if path.endswith(".png") and not path.endswith("/folder.png")]
        rotated = [row for row in scope if row >= start] + [row for row in scope if row < start]
        assert paths == [listing, expected, [listing[row] for row in rotated], [listing[row] for row in scope], []], paths
        assert len([row for row in replies if row.get("where") == "stale" and row.get("path") == "paths"]) == 1, replies
        galleries.append(expected)
        print(f"viewerorder-backend: descending={descending} hidden={hidden}, {len(expected)} ordered images", flush=True)

    viewer = root / "viewer"
    write(root, viewer, '#!/bin/sh\nexec python3 -I -B "$VIEWERORDER_DRIVER" record "$@"\n')
    owned(root, viewer).chmod(0o755)
    env["VIEWERORDER_DRIVER"] = str(Path(__file__).resolve())
    desktop = root / "data/applications/viewerorder.desktop"
    write(root, root / "config/mimeapps.list", "[Default Applications]\nimage/png=viewerorder.desktop;\n")
    kind = subprocess.run(["gio", "info", "--attributes=standard::content-type", galleries[0][0]],
                          env=env, cwd=root, capture_output=True, text=True, timeout=5)
    assert kind.returncode == 0 and "standard::content-type: image/png" in kind.stdout, (kind.stdout, kind.stderr)
    for code in ["F", "U", "f", "u"]:
        write(root, desktop, f'[Desktop Entry]\nType=Application\nName=Viewer order\nX-GIO-NoFuse=true\nExec="{viewer}" %{code}\n')
        handler = subprocess.run(["gio", "mime", "image/png"], env=env, cwd=root,
                                 capture_output=True, text=True, timeout=5)
        assert handler.returncode == 0 and handler.stdout.splitlines()[0].endswith(": viewerorder.desktop"), (handler.stdout, handler.stderr)
        receipt = owned(root, root / f"receipt-{code}.json")
        env["VIEWERORDER_RECEIPT"] = str(receipt)
        expected = galleries[0] if code.isupper() else galleries[0][:1]
        result = subprocess.run([binary, "--open", *galleries[0]], env=env, cwd=root,
                                capture_output=True, text=True, timeout=20)
        assert result.returncode == 0, (code, result.returncode, result.stderr)
        deadline = time.monotonic() + 5
        while True:
            try:
                args = json.loads(receipt.read_text())
                break
            except (FileNotFoundError, json.JSONDecodeError):
                assert time.monotonic() < deadline, f"no completed receipt for %{code}"
                time.sleep(0.01)
        if code.lower() == "u":
            assert all(path.startswith("file://") for path in args), args
            args = [unquote(urlparse(path).path) for path in args]
        assert args == expected, (code, len(args), len(expected))
        print(f"viewerorder-backend: real GIO %{code} delivered {len(args)} ordered paths", flush=True)
    escaped_exec_controls(binary, root, env, desktop, viewer, galleries[0][:2])
    print("viewerorder-backend: PASS", flush=True)


if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "record":
        record()
    else:
        run(str(Path(sys.argv[1]).resolve(strict=True)), Path(sys.argv[2]))
