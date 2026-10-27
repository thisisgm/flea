#!/usr/bin/env python3
"""Replay files produced by the source-built 0.3.12 backend in a marked home."""
import base64
import json
import os
from pathlib import Path
import subprocess
import shutil
import select
import time
import sys

import deltas
import sandbox

repo = Path(__file__).resolve().parents[2]
binary = Path(sys.argv[1]).resolve()
capture = "--capture" in sys.argv
root = sandbox.make("state-")
for name in ("home", "config", "state", "data", "cache", "runtime", "bin", "files"):
    (root / name).mkdir(mode=0o700)
env = {"PATH": str(root / "bin") + ":/usr/bin:/bin", "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"), "XDG_STATE_HOME": str(root / "state"), "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"), "XDG_RUNTIME_DIR": str(root / "runtime"), "XDG_DATA_DIRS": str(root / "missing"), "FLEA_UI": str(repo / "ui"), "LC_ALL": "C"}
for name in ("hyprctl", "zoxide"):
    p = root / "bin" / name
    p.write_text("#!/bin/sh\nexit 0\n")
    p.chmod(0o700)
file = root / "files/a.txt"
file.write_text("state compatibility\n")
key = "math\nfixture-theme\nfalse\nx^2"
svg = '<svg xmlns="http://www.w3.org/2000/svg"><text>x</text></svg>'


def run(args, stdin=b"", extra=None):
    result = subprocess.run([str(binary), *args], input=stdin, env=env | (extra or {}), cwd=root, capture_output=True, timeout=8)
    assert result.returncode == 0, (args, result.returncode, result.stderr)
    return result.stdout


def normalize(data):
    return data.replace(str(root).encode(), b"@ROOT@")


def snapshot():
    return {str(p.relative_to(root)): base64.b64encode(normalize(p.read_bytes())).decode() for area in ("config", "state", "data", "cache") for p in (root / area).rglob("*") if p.is_file()}


def tags(write=False):
    child = subprocess.Popen([str(binary), "--backend"], env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    pending = bytearray()
    def send(request):
        child.stdin.write((json.dumps(request) + "\n").encode())
        child.stdin.flush()
    # Sample input: b'{"t":"tagstates","id":4,"applied":["Red"],"mixed":[]}\n', one backend reply per line.
    def receive(kind):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            while b"\n" in pending:
                line, _, rest = pending.partition(b"\n")
                pending[:] = rest
                message = json.loads(line)
                assert message.get("t") != "error", message
                if message.get("t") == kind:
                    return message
            ready, _, _ = select.select([child.stdout], [], [], max(0, deadline-time.monotonic()))
            assert ready, f"backend did not answer {kind}"
            block = os.read(child.stdout.fileno(), 65536)
            assert block, f"backend exited before {kind}"
            pending.extend(block)
        raise AssertionError(f"backend did not answer {kind}")
    try:
        send({"c": "list", "path": str(root / "files"), "first": 1})
        listing = receive("rows")["listing"]
        if write:
            send({"c": "settags", "op": "add", "tag": "Red", "rows": [0], "listing": listing})
            result = receive("settagged")
            assert result["ok"] == 1, result
        send({"c": "tagstates", "rows": [0], "id": 9, "listing": listing})
        return receive("tagstates")
    finally:
        child.communicate(b'{"c":"quit"}\n', timeout=8)


def reads():
    return {
        "ui-state": base64.b64encode(normalize(run(["--ui-state"]))).decode(),
        "favourites": base64.b64encode(normalize(run(["--favourites", '{"op":"inspect","indices":[0]}']))).decode(),
        "piles": base64.b64encode(normalize(run(["shelf", "piles"]))).decode(),
        "figure": base64.b64encode(run(["--figure-store"], (json.dumps({"op": "get", "id": 4, "key": key}) + "\n").encode())).decode(),
    }


if capture:
    patch = {"keys": "vim", "view": "grid", "lastPath": str(root / "files"), "lastTabs": {"paths": [str(root / "files"), str(root / "home")], "index": 1}, "display": {"textSize": {"mode": 14}}}
    run(["--ui-state", json.dumps(patch)])
    run(["--favourites", json.dumps({"op": "add", "record": {"path": str(root / "files"), "label": "Kept folder"}})])
    run(["shelf", "add", str(file)])
    run(["shelf", "pin", str(file)])
    run(["shelf", "drag-begin", "copy", str(file)])
    run(["shelf", "clear"])
    run(["shelf", "restore"])
    run(["shelf", "toggle"])
    (root / "transfer").mkdir()
    (root / "landed").mkdir()
    transfer = root / "transfer/moved.txt"
    transfer.write_text("move journal fixture\n")
    run(["shelf", "add", str(transfer)])
    run(["shelf", "move", str(root / "landed"), str(transfer)])
    run(["shelf", "zip", "2026-10-18", str(file)])
    run(["shelf", "cancel"])
    tagstates = tags(write=True)
    run(["--figure-store"], (json.dumps({"op": "put", "key": key, "svg": svg}) + "\n").encode())
    fixture = {"source": "v0.3.12/f21789e56", "files": snapshot(), "reads": reads(), "tags": base64.b64encode(os.getxattr(file, "user.xdg.tags")).decode(), "tagstates": tagstates, "writers": ["--ui-state", "--favourites add", "shelf add/pin/drag-begin/clear/restore/toggle/move/zip/cancel", "backend settags", "--figure-store put"]}
    print("STATE_GOLDEN=" + json.dumps(fixture, sort_keys=True, separators=(",", ":")))
else:
    fixture = json.loads((repo / "tests/compat/state-v0312.json").read_text())
    for relative, encoded in fixture["files"].items():
        path = root / relative
        assert path.is_absolute() and ".." not in Path(relative).parts and path.resolve().is_relative_to(root.resolve()), relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(base64.b64decode(encoded).replace(b"@ROOT@", str(root).encode()))
    (root / "landed").mkdir()
    before = snapshot()
    if "tags" in fixture:
        os.setxattr(file, "user.xdg.tags", base64.b64decode(fixture["tags"]))
        assert tags() == fixture["tagstates"], "0.3.12 tags changed in the backend reader"
        assert os.getxattr(file, "user.xdg.tags") == base64.b64decode(fixture["tags"])
    expected_reads = dict(fixture["reads"])
    for name, changes in deltas.load("state").items():
        expected_reads[name] = deltas.apply(name, expected_reads[name], changes)
    assert reads() == expected_reads, "0.3.12 state reader output changed"
    assert snapshot() == before, "reading 0.3.12 state rewrote a persisted file"
    shutil.copy2(binary, root / "bin/flea")
    binary = root / "bin/flea"
    gui = root / "bin/flea-gui"
    gui.write_text("#!/bin/sh\nexit 0\n")
    gui.chmod(0o700)
    state = root / "state/flea/ui.json"
    from persisted_docs import check_documents
    check_documents(root, binary, env, file, run)
    for unknown in (b'{"stateVersion":999,"future":{"opaque":[1,2,3]}}\n', b'{"not": "a supported document"', b'[1,2,3]\n'):
        state.write_bytes(unknown)
        run(["--ui-state"])
        assert state.read_bytes() == unknown, "reading unknown state rewrote its source"
        run(["--gui"], extra={"FLEA_FRONTEND": "gpui", "WAYLAND_DISPLAY": "fixture", "QSG_RHI_BACKEND": "opengl", "VK_DRIVER_FILES": str(root / "missing.json")})
        assert state.read_bytes() == unknown, "launching on unknown state rewrote its source"
    state.write_bytes(base64.b64decode(fixture["files"]["state/flea/ui.json"]).replace(b"@ROOT@", str(root).encode()))
    run(["--ui-state", '{"keyHints":true}'])
    saved = json.loads(state.read_text())
    assert saved["keys"] == "vim" and saved["lastTabs"]["index"] == 1
    assert saved["places"]["favourites"][0]["label"] == "Kept folder"
    # The first 0.4.0 write migrates the 0.3.12 document to state version 4.
    assert saved["keyHints"] is True and saved["stateVersion"] == 4
    print(f"state-compat: replayed {len(fixture['files'])} files from {fixture['source']}")
print("state-compat: all covered readers preserved source files")
