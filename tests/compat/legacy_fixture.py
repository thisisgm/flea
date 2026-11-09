#!/usr/bin/python3
"""Private Quickshell-shaped process fixture with a locked registry and IPC socket."""
import fcntl
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import time

root = Path(os.environ["UPGRADE_FIXTURE"])
runtime = Path(os.environ["XDG_RUNTIME_DIR"])
args = sys.argv[1:]
LUA_CLOSE = r'hl\.dsp\.window\.close\(\{ window = "address:(0x[0-9a-f]+)" \}\)'
CLASSIC_CLOSE = r"closewindow address:(0x[0-9a-f]+)"


def exchange(pid, message):
    directory = (runtime / "quickshell/by-pid" / str(pid)).resolve()
    with socket.socket(socket.AF_UNIX) as client:
        directory_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
        client.connect(f"/proc/self/fd/{directory_fd}/ipc.sock")
        os.close(directory_fd)
        client.sendall(message.encode())
        result = bytearray()
        while data := client.recv(4096):
            result.extend(data)
    return bytes(result)


if args[:2] == ["-j", "clients"]:
    print((root / "clients.json").read_text())
elif args[:1] == ["dispatch"]:
    # Hyprland parses a dispatch as Lua unless the session predates it, and a shape it cannot run exits 7.
    shape = CLASSIC_CLOSE if (root / "classic-compositor").exists() else LUA_CLOSE
    close = re.fullmatch(shape, " ".join(args[1:]))
    if not close:
        print("error: ')' expected near 'address'")
        raise SystemExit(7)
    clients = json.loads((root / "clients.json").read_text())
    pid = next(c["pid"] for c in clients if c["address"] == close[1])
    exchange(pid, "CLOSE")
    print("ok")
elif args[:2] == ["ipc", "--pid"]:
    identity = (runtime / "quickshell/by-pid" / args[2]).resolve().name
    if json.loads((root / (identity + ".json")).read_text()).get("ipcError"):
        raise SystemExit(7)
    sys.stdout.buffer.write(exchange(int(args[2]), args[-1]))
elif args[:1] == ["-p"]:
    identity = os.environ["OLD_ID"]
    directory = runtime / "quickshell/by-id" / identity
    directory.mkdir(parents=True, mode=0o700)
    for parent, name in (("by-shell/flea", identity), ("by-pid", str(os.getpid()))):
        p = runtime / "quickshell" / parent
        p.mkdir(parents=True, exist_ok=True, mode=0o700)
        (p / name).symlink_to(directory)
    lock = (directory / "instance.lock").open("w+")
    lock.write(str(os.getpid()))
    lock.flush()
    fcntl.lockf(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    backend = subprocess.Popen([os.environ["FLEA_BIN"], "--backend"], stdin=subprocess.PIPE, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    (directory / "backend.pid").write_text(str(backend.pid))
    server = socket.socket(socket.AF_UNIX)
    directory_fd = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
    server.bind(f"/proc/self/fd/{directory_fd}/ipc.sock")
    os.close(directory_fd)
    server.listen(4)
    (directory / "ready").touch()
    try:
        while True:
            connection, _ = server.accept()
            with connection:
                message = connection.recv(4096).decode()
                state = json.loads((root / (identity + ".json")).read_text())
                if message == "CLOSE":
                    backend.communicate(b'{"c":"quit"}\n', timeout=5)
                    (directory / "drained").touch()
                    break
                if state.get("hung"):
                    time.sleep(2)
                answer = {
                    "statusActivityState": {"activities": [{"running": True}] if state.get("busy") else []},
                    "renameEditorLive": bool(state.get("editor")),
                    "providerState": {"path": state["path"], "cursorPath": state.get("cursor", ""), "selectedPaths": state.get("selection", [])},
                }[message]
                try:
                    connection.sendall((json.dumps(answer) + "\n").encode())
                except BrokenPipeError:
                    pass
    finally:
        if backend.poll() is None:
            backend.communicate(b'{"c":"quit"}\n', timeout=5)
else:
    raise SystemExit("unexpected fixture argv")
