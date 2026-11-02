"""Real browser, picker and simultaneous-window launch and lifecycle evidence."""
import os
from pathlib import Path
import subprocess
import sys
import time

from flea_transport import TransportError, Window, client_run, load, qml_metadata

root = Path(sys.argv[1])
windows = []
roundtrips = []


def start(path, picker=False):
    ui = Path(os.environ.get("FLEA_TEST_UI", Path(__file__).resolve().parents[2] / "ui"))
    command = ["qs", "-p", str(ui / "boot" / ("picker.qml" if picker else "shell.qml"))] if frontend == "qml" else None
    window = Window.start(path, parent=root, picker=picker, ui=ui, timeout=15, command=command)
    windows.append(window)
    return window


def call(window, target, function):
    before = time.monotonic_ns()
    result = window.call("call", target, function)
    roundtrips.append((time.monotonic_ns() - before) / 1_000_000)
    return result


def wait_path(window, expected, target="flea", function="path"):
    deadline = time.monotonic() + 15
    while True:
        result = call(window, target, function)
        if result.returncode == 3:
            raise TransportError(result.stderr.decode().strip())
        if result.returncode == 0 and result.stdout == (str(expected) + "\n").encode():
            return
        if time.monotonic() >= deadline:
            raise TransportError(f"{target}.{function} did not commit: {result}")
        time.sleep(0.05)


frontend = os.environ.get("FLEA_TEST_FRONTEND", "gpui")
print(f"TRANSPORT_NATIVE scope={'product' if frontend == 'gpui' else 'frozen-qml-oracle'} frontend={frontend}", flush=True)


def public_cli(window, target, function):
    record = load(window.root)
    adapted = call(window, target, function)
    direct = client_run(record, [record["binary"], "ipc", "--pid", str(record["pid"]),
                                 "call", target, function])
    assert (direct.returncode, direct.stdout, direct.stderr) == (adapted.returncode, adapted.stdout, adapted.stderr), (direct, adapted)
    print(f"TRANSPORT_NATIVE public-cli={target}.{function} bytes=equal exit={direct.returncode}", flush=True)


try:
    for name in ("first", "second"):
        (root / name).mkdir()
        (root / name / "file.txt").touch()
    first = start(root / "first")
    wait_path(first, root / "first")
    second = start(root / "second")
    wait_path(second, root / "second")
    wait_path(first, root / "first")
    for window in (first, second):
        show = window.call("show")
        assert show.returncode == 0 and b"target flea\n" in show.stdout, show
    if frontend == "gpui":
        for window in (first, second):
            public_cli(window, "flea", "path")
        refusal = first.call("call", "fleapicker", "ready")
        assert (refusal.returncode, refusal.stdout, refusal.stderr) == (3, b"", b"FLEA_IPC_NOT_PORTED fleapicker.ready\n"), refusal
        wait_path(first, root / "first")
        print("TRANSPORT_NATIVE wrong-surface=refused browser-still-live=true", flush=True)
        environment = dict(os.environ, FLEA_FRONTEND="gpui")
        environment.pop("FLEA_PICKER", None)
        missing = subprocess.run([load(first.root)["binary"], "--pick", str(root / "missing-request-reply")],
                                 env=environment, capture_output=True, timeout=5)
        assert missing.returncode == 2 and b"--pick needs FLEA_PICKER" in missing.stderr, missing
        assert not (root / "missing-request-reply").exists()
        print("TRANSPORT_NATIVE missing-picker-request=refused exit=2", flush=True)
    if os.environ.get("FLEA_TEST_FRONTEND", "gpui") == "qml":
        instance = qml_metadata(load(first.root))["id"]
        cli = Path(__file__).with_name("flea_transport_cli.py")
        for flag in ("-i", "--id"):
            result = subprocess.run([sys.executable, "-B", str(cli), "route", "--", flag, instance,
                                     "call", "flea", "path"], capture_output=True, timeout=15)
            assert (result.returncode, result.stdout, result.stderr) == (0, (str(root / "first") + "\n").encode(), b""), result
        print("TRANSPORT_NATIVE qml-instance-id=stable selectors=ok", flush=True)
    print("TRANSPORT_NATIVE browser=ok two-window=ok", flush=True)
    for window in (first, second):
        window.quit()
    teardown = "quit=clean" if frontend == "gpui" else "teardown=owned-stop"
    print(f"TRANSPORT_NATIVE browser-{teardown} roundtrip-ms={min(roundtrips):.3f}..{max(roundtrips):.3f}", flush=True)
    try:
        picker = start(root / "first", picker=True)
    except TransportError as error:
        print(f"TRANSPORT_NATIVE picker=BLOCKED {error}", flush=True)
        raise
    result = call(picker, "fleapicker", "ready")
    if result.returncode == 3:
        raise TransportError(result.stderr.decode().strip())
    assert (result.returncode, result.stdout, result.stderr) == (0, b"true\n", b""), result
    wait_path(picker, root / "first", "fleapicker")
    wait_path(picker, 1, "fleapicker", "total")
    if frontend == "gpui":
        public_cli(picker, "fleapicker", "ready")
        public_cli(picker, "fleapicker", "path")
        invalid = picker.call("call", "fleapicker", "transportUnknownFunction")
        assert (invalid.returncode, invalid.stdout, invalid.stderr) == (0, b"Function not found.\n", b""), invalid
        assert call(picker, "fleapicker", "ready").stdout == b"true\n"
        print("TRANSPORT_NATIVE unknown-function=refused picker-still-live=true", flush=True)
    picker.quit()
    print(f"TRANSPORT_NATIVE picker=ok {teardown} roundtrip-ms={min(roundtrips):.3f}..{max(roundtrips):.3f}")

finally:
    for window in windows:
        window.stop()
