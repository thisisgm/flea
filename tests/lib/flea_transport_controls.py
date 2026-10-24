"""Passing twins and deliberate failures at the owned transport boundary."""
import json
import os
from pathlib import Path
import socket
import stat
import subprocess
import sys
import tempfile
import time

from flea_transport import MARKER, REPO, TransportError, Window, load
from flea_native_inputs_controls import check_inputs, check_launch_inputs
from flea_native_harness_controls import check_harness
from flea_native_envelope_controls import check_envelope

root = Path(tempfile.mkdtemp(prefix="controls.", dir=REPO / ".superpowers/g010t"))
(root / MARKER).touch()
(root / "target/debug").mkdir(parents=True)
fixture = '''#!/usr/bin/env python3
import json, os, socket, subprocess, sys, time
from pathlib import Path
runtime = Path(os.environ["XDG_RUNTIME_DIR"])
directory = runtime / "flea-ipc"
directory.mkdir(mode=0o700, exist_ok=True)
path = directory / f"{os.getpid()}.sock"
if os.environ.get("CONTROL_OTHER_PEER"):
    env = dict(os.environ)
    del env["CONTROL_OTHER_PEER"]
    subprocess.Popen([sys.executable, __file__, *sys.argv[1:]], env=dict(env, CONTROL_SOCKET=str(path)))
    time.sleep(60)
    sys.exit()
path = Path(os.environ.get("CONTROL_SOCKET", str(path)))
server = socket.socket(socket.AF_UNIX)
server.bind(str(path))
os.chmod(path, 0o600)
server.listen()
while True:
    connection, _ = server.accept()
    with connection:
        data = connection.recv(65536)
        if not data:
            continue
        if os.environ.get("CONTROL_STALL"):
            time.sleep(2)
        request = json.loads(data)
        function = request.get("call", ["", ""])[1]
        reply = {"code": 0, "stdout": "owned\\n", "stderr": ""}
        if function == "palette":
            reply.update(code=3, stdout="", stderr="FLEA_IPC_NOT_PORTED flea.palette\\n")
        if function == "hypr-runtime":
            reply["stdout"] = str((runtime / "hypr/control/.socket.sock").is_socket()) + "\\n"
        if function == "quit":
            reply["stdout"] = ""
        connection.sendall((json.dumps(reply) + "\\n").encode())
        if function == "quit":
            assert path.is_absolute() and str(path) and path.parent == directory
            assert (runtime / ".flea-test-sandbox").is_file()
            path.unlink()
            break
'''
for name in ("flea", "flea-gui"):
    path = root / "target/debug" / name
    path.write_text(fixture)
    path.chmod(0o700)
os.environ["CARGO_TARGET_DIR"] = str(root / "target")
os.environ.pop("FLEA_BIN", None)
os.environ.pop("FLEA_TEST_WINDOWS_ROOT", None)
os.environ.pop("FLEA_TEST_WINDOW", None)
windows = []
checks = 0


def check(condition, name):
    global checks
    assert condition, name
    checks += 1
    print(f"TRANSPORT TWIN {name} exit=0", flush=True)


def refuse(name, action, diagnostic):
    global checks
    try:
        action()
    except (TransportError, OSError, subprocess.SubprocessError) as error:
        assert diagnostic in str(error), (name, error)
        checks += 1
        print(f"TRANSPORT CONTROL {name} exit=1 reason={error}", flush=True)
    else:
        raise AssertionError(f"control accepted: {name}")


def start(**environment):
    os.environ.update(environment)
    try:
        window = Window.start(root, parent=root, frontend="gpui", timeout=1)
        windows.append(window)
        return window
    finally:
        for key in environment:
            os.environ.pop(key, None)


try:
    check_inputs(root)
    check_launch_inputs(root, Window)
    check_harness(root)
    check_envelope(root)
    stub = root / "clients"
    stub.mkdir()
    for name in ("qs", "omarchy-drive"):
        program = stub / name
        program.write_text('#!/bin/bash\nprintf "<%s>\\n" "$@"\nprintf "client diagnostic\\n" >&2\nexit "${STUB_STATUS:-0}"\n')
        program.chmod(0o700)
    os.environ["PATH"] = str(stub) + ":" + os.environ["PATH"]
    qml = Window.start(root, parent=root, frontend="qml", timeout=1)
    windows.append(qml)
    for status in (0, 1, 3, 255):
        os.environ["STUB_STATUS"] = str(status)
        for drive, prefix in ((False, ["qs", "ipc"]), (True, ["omarchy-drive", "ipc"])):
            arguments = ["call", "flea", "path", " a\nb "]
            original = subprocess.run(prefix + ["--pid", str(qml.pid)] + (arguments[1:] if drive else arguments), capture_output=True)
            forwarded = qml.call(*arguments, drive=drive)
            check((original.returncode, original.stdout, original.stderr) ==
                  (forwarded.returncode, forwarded.stdout, forwarded.stderr), f"qml-bytes-{drive}-{status}")
    os.environ.pop("STUB_STATUS")
    qml.quit()
    wrapper = root / "launch-wrapper"
    wrapper.write_text('#!/bin/sh\n"$@" &\nwait "$!"\n')
    wrapper.chmod(0o700)
    wrapped = Window.start(root, parent=root, frontend="gpui", timeout=1, wrap=[str(wrapper)])
    windows.append(wrapped)
    check(wrapped.call("call", "flea", "path").stdout == b"owned\n", "owned-wrapper-descendant")
    wrapped.stop()
    check((wrapped.root / "stopped").is_file(), "wrapper-group-drained")
    first = start()
    second = start()
    record = load(first.root)
    runtime_info = os.lstat(record["runtime"])
    check(stat.S_ISDIR(runtime_info.st_mode) and runtime_info.st_mode & 0o077 == 0, "native-runtime-contract")
    compositor_runtime = root / "compositor-runtime"
    (compositor_runtime / "hypr/control").mkdir(parents=True)
    with socket.socket(socket.AF_UNIX) as compositor_socket:
        runtime_fd = os.open(compositor_runtime, os.O_RDONLY | os.O_DIRECTORY)
        try:
            compositor_socket.bind(f"/proc/self/fd/{runtime_fd}/hypr/control/.socket.sock")
        finally:
            os.close(runtime_fd)
        saved_runtime = os.environ.get("XDG_RUNTIME_DIR")
        try:
            os.environ["XDG_RUNTIME_DIR"] = str(compositor_runtime)
            compositor_window = Window.start(root, parent=root, frontend="gpui", timeout=1)
            windows.append(compositor_window)
        finally:
            if saved_runtime is None:
                os.environ.pop("XDG_RUNTIME_DIR", None)
            else:
                os.environ["XDG_RUNTIME_DIR"] = saved_runtime
        hypr = compositor_window.root / "runtime/hypr"
        check(hypr.is_symlink() and hypr.resolve() == compositor_runtime / "hypr", "private-runtime-exposes-hypr")
        check(compositor_window.call("call", "flea", "hypr-runtime").stdout == b"True\n", "owned-child-sees-hypr-socket")
        compositor_window.quit()
    environment = dict(os.environ, FLEA_TEST_WINDOWS_ROOT=str(root), FLEA_IPC_KIND="gpui",
                       FLEA_IPC_BIN=str(root / "target/debug/flea"), FLEA_IPC_HELPER=str(REPO / "tools/flea-ipc"))
    helper = str(REPO / "tools/flea-ipc")
    def shell(*arguments):
        return subprocess.run([helper, *map(str, arguments)], env=environment, capture_output=True, timeout=3)
    check(shell("--pid", first.pid, "call", "flea", "path").stdout == b"owned\n", "shell-pid-route")
    check(shell("drive", "--pid", first.pid, "flea", "path").stdout == b"owned\n", "shell-drive-route")
    check(shell("wait", "--pid", first.pid, "flea", "path", "owned", "--timeout", "0").returncode == 0, "shell-wait-route")
    check(shell("--pid", os.getpid(), "call", "flea", "path").returncode == 255, "shell-foreign-pid-refused")
    listing = shell("list", "--all", "--json")
    check(listing.returncode == 0 and len(json.loads(listing.stdout)) == 2, "shell-owned-list")
    ambiguous = shell("-p", REPO / "ui/boot", "call", "flea", "path")
    check(ambiguous.returncode == 255 and b"2 windows" in ambiguous.stderr, "ambiguous-config-refused")
    check(first.call("call", "flea", "path").stdout == b"owned\n", "correct-socket")
    refuse("wrong-socket", lambda: first.call("call", "flea", "path", socket_path=load(second.root)["socket"]), "wrong socket")
    check(first.call("call", "flea", "path", pid=first.pid).returncode == 0, "owned-pid")
    refuse("foreign-pid", lambda: first.call("call", "flea", "path", pid=os.getpid()), "foreign PID")
    check(first.call("call", "flea", "path", timeout=1).returncode == 0, "completed-read")
    stalled = start(CONTROL_STALL="1")
    before = time.monotonic()
    refuse("read-timeout", lambda: stalled.call("call", "flea", "path", timeout=0.1), "timed out")
    check(time.monotonic() - before < 0.5, "bounded-read")
    check(second.call("call", "flea", "path").returncode == 0, "second-window-peer")
    other = start()
    options = json.loads((other.root / "window.json").read_text())
    options["socket"] = load(second.root)["socket"]
    options["socket_identity"] = load(second.root)["socket_identity"]
    (other.root / "window.json").write_text(json.dumps(options))
    refuse("another-window-reply", lambda: other.call("call", "flea", "path"), "foreign socket")
    refuse("another-window-peer", lambda: start(CONTROL_OTHER_PEER="1"), "reply from another window")
    check(first.call("call", "flea", "palette").stderr == b"FLEA_IPC_NOT_PORTED flea.palette\n", "named-refusal")
    refuse("foreign-teardown", lambda: first.stop(pid=second.pid), "foreign PID")
    check(second.call("call", "flea", "path").returncode == 0, "foreign-process-survives")
    first.stop(pid=first.pid)
    check((first.root / "stopped").is_file(), "owned-teardown")
    check(second.quit().returncode == 0, "quit-acknowledged")
    second.stop()
    check((second.root / "stopped").is_file(), "quit-drained")
    print(f"TRANSPORT_CONTROLS checks={checks} failed=0")
finally:
    for window in windows:
        window.stop()
