"""Real browser, picker and simultaneous-window launch and lifecycle evidence."""
import os
from pathlib import Path
import subprocess
import sys
import time

from flea_transport import TransportError, Window, load, qml_metadata

root = Path(sys.argv[1])
windows = []
roundtrips = []


def start(path, picker=False):
    window = Window.start(path, parent=root, picker=picker, ui=os.environ.get("FLEA_TEST_UI"), timeout=15)
    windows.append(window)
    return window


def call(window, target, function):
    before = time.monotonic_ns()
    result = window.call("call", target, function)
    roundtrips.append((time.monotonic_ns() - before) / 1_000_000)
    return result


def wait_path(window, expected):
    deadline = time.monotonic() + 15
    while True:
        result = call(window, "flea", "path")
        if result.returncode == 3:
            raise TransportError(result.stderr.decode().strip())
        if result.returncode == 0 and result.stdout == (str(expected) + "\n").encode():
            return
        if time.monotonic() >= deadline:
            raise TransportError(f"path did not commit: {result}")
        time.sleep(0.05)


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
    print(f"TRANSPORT_NATIVE browser-quit=clean roundtrip-ms={min(roundtrips):.3f}..{max(roundtrips):.3f}", flush=True)
    try:
        picker = start(root / "first", picker=True)
    except TransportError as error:
        print(f"TRANSPORT_NATIVE picker=BLOCKED {error}", flush=True)
        raise
    result = call(picker, "fleapicker", "ready")
    if result.returncode == 3:
        raise TransportError(result.stderr.decode().strip())
    assert (result.returncode, result.stdout, result.stderr) == (0, b"true\n", b""), result
    picker.quit()
    print(f"TRANSPORT_NATIVE picker=ok quit=clean roundtrip-ms={min(roundtrips):.3f}..{max(roundtrips):.3f}")

finally:
    for window in windows:
        window.stop()
