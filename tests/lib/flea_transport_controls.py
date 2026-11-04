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
from types import SimpleNamespace
from unittest import mock

from flea_transport import MARKER, REPO, TransportError, Window, load, sandbox_backend
from flea_native_inputs_controls import check_inputs, check_launch_inputs
from flea_native_harness_controls import check_harness
from flea_native_envelope_controls import check_envelope
from flea_transport_teardown_controls import check_teardown, check_teardown_past_an_unwritable_error
from flea_uish_headless_drain_controls import (fixreaders_ey25_forced_teardown_keeps_the_short_grace,
                                               fixreaders_ey25_owned_teardown_waits_for_a_held_drain)
from flea_transport_log_controls import check_shared_log
from flea_transport_race_controls import disappearing_peer
from flea_transport_peers import PeerWindow
from flea_transport_lifecycle import identity
from flea_transport_peer_controls import vanishing_peer

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
        if function == "launch":
            reply["stdout"] = json.dumps(dict(argv=sys.argv[1:], picker=os.environ.get("FLEA_PICKER"),
                                              reply=os.environ.get("FLEA_PICKER_REPLY"))) + "\\n"
        if function == "backend":
            reply["stdout"] = os.environ["FLEA_BIN"] + "\\n"
        if function == "spawn":
            child = subprocess.Popen([sys.executable, __file__, *sys.argv[1:]], env=dict(os.environ, CONTROL_APP_CHILD="1"))
            reply["stdout"] = str(child.pid) + "\\n"
        if function == "path" and os.environ.get("CONTROL_APP_CHILD"):
            reply["stdout"] = "child:" + str(os.getpid()) + "\\n"
        if function == "hypr-runtime":
            for name in (".socket.sock", ".socket2.sock"):
                with socket.socket(socket.AF_UNIX) as compositor:
                    compositor.settimeout(1)
                    compositor.connect(str(runtime / "hypr" / ("h" * 64) / name))
                    compositor.sendall(name.encode())
            reply["stdout"] = "True\\n"
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
    check_teardown(root, check)
    fixreaders_ey25_owned_teardown_waits_for_a_held_drain(check)
    fixreaders_ey25_forced_teardown_keeps_the_short_grace(check)
    check_teardown_past_an_unwritable_error(root, check)
    check_shared_log(root)
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
    shared_owner = start()
    shared_wrapped = Window.start(root, parent=root, frontend="gpui", timeout=1,
                                  wrap=[str(wrapper)], share=shared_owner.root)
    windows.extend([shared_owner, shared_wrapped])
    shared_record = load(shared_wrapped.root)
    check(shared_record["pid"] != shared_record["group"], "shared-wrapper-discovers-gui-descendant")
    check(Path(shared_record["runtime_dir"]) == shared_owner.root / "runtime", "shared-wrapper-uses-peer-runtime")
    check(shared_wrapped.call("call", "flea", "path").stdout == b"owned\n", "shared-wrapper-owned-endpoint")
    shared_wrapped.stop()
    check(shared_owner.call("call", "flea", "path").stdout == b"owned\n", "shared-wrapper-stop-keeps-peer-alive")
    shared_owner.quit()
    disappearing_peer(root, start, check, windows)
    first = start()
    second = start()
    picker = Window.start(root, parent=root, frontend="gpui", picker=True, timeout=1)
    windows.append(picker)
    # Sample input: {"argv":["--pick","/fixture/picker-reply"],"picker":"{\"folder\": \"/fixture\"}","reply":"/fixture/picker-reply"}
    launch = json.loads(picker.call("call", "fleapicker", "launch").stdout)
    reply = str(picker.root / "picker-reply")
    check(launch == dict(argv=["--pick", reply], picker=json.dumps({"folder": str(root)}), reply=reply),
          "picker-public-launch-envelope")
    picker.quit()
    # Sample input: {"returncode":0,"forced":false,"error":null}
    check(not json.loads((picker.root / "outcome.json").read_text())["forced"], "picker-natural-quit")
    record = load(first.root)
    alias_parts = record["runtime"].split("/")
    alias_parts[2] = "9" * len(Path("/proc/sys/kernel/pid_max").read_text().strip())
    alias_parts[4] = "255"
    worst_alias = "/".join(alias_parts)
    socket_lengths = {name: len(os.fsencode(f"{worst_alias}/hypr/{'h' * 64}/{name}"))
                      for name in (".socket.sock", ".socket2.sock")}
    print(f"HYPR_SOCKET_LENGTHS {socket_lengths} limit=107", flush=True)
    check(all(length <= 107 for length in socket_lengths.values()), "hypr-sockets-fit-sun-path")
    runtime_info = os.lstat(record["runtime"])
    check(stat.S_ISDIR(runtime_info.st_mode) and runtime_info.st_mode & 0o077 == 0 and
          Path(record["runtime"]).resolve() == first.root / "runtime" and
          (Path(record["runtime"]) / MARKER).is_file(), "native-runtime-contract")
    compositor_runtime = root / "compositor-runtime"
    signature = "h" * 64
    (compositor_runtime / "hypr" / signature).mkdir(parents=True)
    with socket.socket(socket.AF_UNIX) as command_socket, socket.socket(socket.AF_UNIX) as event_socket:
        compositor_sockets = ((".socket.sock", command_socket), (".socket2.sock", event_socket))
        runtime_fd = os.open(compositor_runtime, os.O_RDONLY | os.O_DIRECTORY)
        try:
            for name, compositor_socket in compositor_sockets:
                compositor_socket.bind(f"/proc/self/fd/{runtime_fd}/hypr/{signature}/{name}")
                compositor_socket.listen()
                compositor_socket.settimeout(1)
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
        for name, compositor_socket in compositor_sockets:
            connection, _ = compositor_socket.accept()
            with connection:
                connection.settimeout(1)
                check(connection.recv(64) == name.encode(), f"owned-child-reaches-hypr-{name}")
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
    child_pid = int(first.call("call", "flea", "spawn").stdout)
    child_socket = Path(load(first.root)["runtime_dir"]) / "flea-ipc" / f"{child_pid}.sock"
    deadline = time.monotonic() + 2
    while not child_socket.exists() and time.monotonic() < deadline:
        time.sleep(0.01)
    assert child_socket.exists(), "fixture child never served its endpoint"
    pids = subprocess.run([sys.executable, str(REPO / "tests/lib/flea_transport_cli.py"), "pids", str(root)],
                          env=environment, capture_output=True, timeout=3)
    check(pids.returncode == 0 and str(child_pid).encode() in pids.stdout.splitlines(), "shell-discovers-app-created-window")
    check(shell("--pid", child_pid, "call", "flea", "path").stdout == f"child:{child_pid}\n".encode(), "shell-app-created-window-route")
    check(any(row["pid"] == child_pid for row in json.loads(shell("list", "--all", "--json").stdout)), "shell-app-created-window-list")
    child_record = dict(pid=child_pid, start="0", socket=str(child_socket),
                        socket_identity=[child_socket.stat().st_dev, child_socket.stat().st_ino])
    refuse("app-child-reused-pid", lambda: PeerWindow(first.root, child_record).call("call", "flea", "path"), "PID was reused")
    parent_record = load(first.root)
    child_record.update(start=parent_record["start"], pid=first.pid)
    refuse("app-child-foreign-group", lambda: PeerWindow(second.root, child_record).record(), "foreign process group")
    child_record.update(pid=child_pid, start=identity(child_pid)["start"],
                        socket=parent_record["socket"], socket_identity=parent_record["socket_identity"])
    refuse("app-child-other-endpoint", lambda: PeerWindow(first.root, child_record).call("call", "flea", "path"), "reply from another window")
    check(shell("--pid", child_pid, "call", "flea", "quit").returncode == 0, "shell-app-created-window-quit")
    vanishing_peer(first, second, environment, check)
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
    cwd = os.getcwd()
    caller = root / "caller"
    caller.mkdir()
    try:
        # The caller's cwd differs from the start path, so only cwd-relative resolution passes.
        os.chdir(caller)
        want_select = str(Path(os.getcwd()) / "target.txt")
        selected = Window.start(root, parent=root, frontend="gpui", timeout=1, select="target.txt")
        windows.append(selected)
    finally:
        os.chdir(cwd)
    check(load(selected.root)["select"] == want_select, "select-target-made-absolute")
    selected.stop()
    refuse("select-with-direct-qml", lambda: Window.start(root, parent=root, frontend="qml", timeout=1,
                                                          command=["qs", "-p", str(root / "boot")], select=root / "b.txt"),
           "a select target needs the flea --gui launch")
    refuse("select-empty", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, select=""), "select target is empty")
    stubs = root / "stubs"
    stubs.mkdir()
    backend_stub = stubs / "flea"
    backend_stub.write_text("#!/bin/sh\nexit 0\n")
    backend_stub.chmod(0o700)
    stubbed = Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(backend_stub))
    windows.append(stubbed)
    stubbed_record = load(stubbed.root)
    check(stubbed_record["backend"] == str(backend_stub.resolve()) and stubbed_record["binary"] == str(root / "target/debug/flea"),
          "marked-sandbox-stub-backend")
    check(stubbed.call("call", "flea", "backend").stdout == f"{backend_stub.resolve()}\n".encode(), "stub-backend-reaches-window")
    check(stubbed.quit().returncode == 0, "stub-backend-quit")
    outside = Path("/usr/bin/true")
    check(outside.is_file() and not any((ancestor / MARKER).exists() for ancestor in outside.resolve().parents), "outside-has-no-marker")
    refuse("stub-backend-outside-sandbox", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(outside)),
           "outside a marked sandbox")
    escape = stubs / "escape"
    escape.symlink_to(outside)
    refuse("stub-backend-symlink-escape", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(escape)),
           "is a symlink")
    plain = stubs / "plain"
    plain.write_text("#!/bin/sh\nexit 0\n")
    plain.chmod(0o600)
    refuse("stub-backend-not-executable", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(plain)),
           "not an owned executable")
    refuse("stub-backend-relative", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend="stubs/flea"),
           "absolute path")
    refuse("stub-backend-missing", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(stubs / "missing")),
           "does not exist")
    folder = stubs / "folder"
    folder.mkdir(mode=0o700)
    refuse("stub-backend-directory", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(folder)),
           "not an owned executable")
    # Not under TMPDIR: the CI wrapper points it into a marked root inside REPO, and this dir must be outside both.
    with tempfile.TemporaryDirectory(prefix="flea-unmarked.", dir="/tmp") as unmarked:
        (Path(unmarked) / "flea").write_text("#!/bin/sh\nexit 0\n")
        (Path(unmarked) / "flea").chmod(0o700)
        (stubs / "out").symlink_to(unmarked)
        refuse("stub-backend-parent-symlink-escape",
               lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(stubs / "out/flea")),
               "outside a marked sandbox")
    qml_stubbed = Window.start(root, parent=root, frontend="qml", timeout=1, backend=str(backend_stub))
    windows.append(qml_stubbed)
    check(load(qml_stubbed.root)["backend"] == str(backend_stub.resolve()), "qml-stub-backend")
    qml_stubbed.quit()
    combined = Window.start(root, parent=root, frontend="gpui", timeout=1, backend=str(backend_stub), select=root / "target.txt")
    windows.append(combined)
    combined_record = load(combined.root)
    check(combined_record["backend"] == str(backend_stub.resolve()) and combined_record["select"] == str(root / "target.txt"),
          "stub-backend-with-select")
    combined.quit()
    saved_home = os.environ["HOME"]
    # Not under TMPDIR, for the same reason: a fake home inside a marked root or REPO proves nothing.
    with tempfile.TemporaryDirectory(prefix="flea-fake-home.", dir="/tmp") as outside_home:
        fake_home = Path(outside_home)
        (fake_home / MARKER).touch()
        dotted = fake_home / ".local/flea"
        tool = fake_home / ".cargo/bin/flea"
        worktree = fake_home / ".t3/worktrees/flea"
        for planted in (dotted, tool, worktree / "stubs/flea"):
            planted.parent.mkdir(parents=True)
            planted.write_text("#!/bin/sh\nexit 0\n")
            planted.chmod(0o700)
        try:
            os.environ["HOME"] = str(fake_home)
            refuse("stub-backend-home-dot-dir", lambda: sandbox_backend(str(dotted)), "home dot-directory")
            refuse("stub-backend-home-tool-dir", lambda: sandbox_backend(str(tool)), "home dot-directory")
            refuse("stub-backend-worktree-dot-dir-outside-repo", lambda: sandbox_backend(str(worktree / "stubs/flea")), "home dot-directory")
            with mock.patch("flea_transport.REPO", worktree):
                check(sandbox_backend(str(worktree / "stubs/flea")) == str(worktree / "stubs/flea"), "stub-backend-worktree-dot-dir-accepted")
            os.environ["HOME"] = str(root)
            with mock.patch("pwd.getpwuid", return_value=SimpleNamespace(pw_dir=str(fake_home))):
                refuse("stub-backend-passwd-home-dot-dir", lambda: sandbox_backend(str(dotted)), "home dot-directory")
        finally:
            os.environ["HOME"] = saved_home
    with mock.patch("os.geteuid", return_value=os.geteuid() + 1):
        refuse("stub-backend-foreign-owner", lambda: sandbox_backend(str(backend_stub)), "not an owned executable")
    refuse("stub-as-wrapper-binary", lambda: Window.start(root, parent=root, frontend="gpui", timeout=1, binary=str(backend_stub)),
           "flea must come from this worktree's build")
    refuse("stub-as-flea-gui", lambda: start(FLEA_GUI_BIN=str(backend_stub)), "flea-gui must come from this worktree's build")
    print(f"TRANSPORT_CONTROLS checks={checks} failed=0")
finally:
    for window in windows:
        window.stop()
