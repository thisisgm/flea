#!/usr/bin/env python3
"""Drive every native adapter against owned Wayland and input sockets without a compositor."""
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import tempfile

repo = Path(__file__).resolve().parents[3]
parent = repo / ".superpowers/nr"
parent.mkdir(parents=True, exist_ok=True)
root = Path(tempfile.mkdtemp(prefix="c.", dir=parent)).resolve()
(root / ".flea-test-sandbox").touch()
probe = '''import ctypes, os, socket
from pathlib import Path
runtime = Path(os.environ["XDG_RUNTIME_DIR"])
assert runtime.is_relative_to(Path(os.environ["W"]) / ".superpowers")
assert not (runtime / "quickshell").exists()
client = ctypes.CDLL("libwayland-client.so.0")
client.wl_display_connect.argtypes = [ctypes.c_char_p]
client.wl_display_connect.restype = ctypes.c_void_p
client.wl_display_disconnect.argtypes = [ctypes.c_void_p]
client.wl_display_disconnect.restype = None
display = client.wl_display_connect(None)
if not display:
    raise RuntimeError("private runtime cannot connect to Wayland")
client.wl_display_disconnect(display)
with socket.socket(socket.AF_UNIX) as input_socket:
    input_socket.connect(str(runtime / ".ydotool_socket"))
with open(os.environ["FLEA_RUNTIME_PROBE_LOG"], "a") as report:
    report.write(os.environ["FLEA_FRONTEND"] + " PASS\\n")
'''
adapters = ("op2", "mn2", "op3", "pk2", "pl4")
picker_suites = ("picker-040", "picker-grid", "picker-header", "picker-hunt", "picker-recent",
                 "picker-selection", "picker-save-wrap", "picker-issue313-native")


def stage_at(label, adapter):
    stage = root / label
    (stage / "tests/native/g2").mkdir(parents=True)
    (stage / ".flea-test-sandbox").touch()
    (stage / "fixture/data").mkdir(parents=True)
    shutil.copyfile(repo / f"tests/native/g2/{adapter}.sh", stage / f"tests/native/g2/{adapter}.sh")
    (stage / "probe.py").write_text(probe)
    for name in ("ui", "ui-trash", *picker_suites):
        (stage / f"tests/{name}.sh").write_text('#!/bin/bash\nexec python3 "$W/probe.py"\n')
    native = stage / "tests/native/g2"
    (native / "pk2-fixture.sh").write_text('#!/bin/bash\nprintf "%s\\n" "$W/fixture"\n')
    picker = native / "pk2-gpui.sh"
    picker.write_text('#!/bin/bash\nexec python3 "$W/probe.py"\n')
    picker.chmod(0o755)
    return stage


def environment(stage, runtime, display):
    return dict(PATH="/usr/bin:/bin", HOME=str(stage / "operator-home"), W=str(stage),
                Q_BIN="/usr/bin/true", G_BIN="/usr/bin/true", XDG_RUNTIME_DIR=str(runtime),
                WAYLAND_DISPLAY=display, FLEA_VISUAL_DESIGN=str(stage),
                FLEA_RUNTIME_PROBE_LOG=str(stage / "probe.log"))


def run_adapter(stage, env, adapter):
    fd = os.open(stage, os.O_RDONLY | os.O_DIRECTORY)
    try:
        env["W"] = f"/proc/self/fd/{fd}"
        return subprocess.run(["/bin/bash", f"tests/native/g2/{adapter}.sh"], cwd=stage,
                              env=env, pass_fds=(fd,), stdin=subprocess.DEVNULL,
                              capture_output=True, text=True, start_new_session=True, timeout=30)
    finally:
        os.close(fd)


failures = []
counter = 0
for adapter in adapters:
    for mode in ("relative", "absolute"):
        label = f"{adapter}-{mode}"
        stage = stage_at(str(counter), adapter)
        counter += 1
        runtime = stage / "session"
        runtime.mkdir(mode=0o700)
        (runtime / "hypr").mkdir()
        (runtime / "quickshell").mkdir()
        display = runtime / "w"
        with socket.socket(socket.AF_UNIX) as wayland, socket.socket(socket.AF_UNIX) as input_socket:
            wayland.bind(str(display))
            wayland.listen(128)
            input_socket.bind(str(runtime / ".ydotool_socket"))
            input_socket.listen(128)
            entries = set(runtime.iterdir())
            env = environment(stage, runtime, display.name if mode == "relative" else str(display))
            result = run_adapter(stage, env, adapter)
            (stage / "adapter.log").write_text(result.stdout + result.stderr)
            report = stage / "probe.log"
            lines = report.read_text().splitlines() if report.exists() else []
            passed = (result.returncode == 2 and "scene CLI tools/flea-ci-visual is absent" in result.stderr
                      and {"qml PASS", "gpui PASS"}.issubset(lines) and set(runtime.iterdir()) == entries)
            if not passed:
                failures.append(label)
            print(f"NATIVE RUNTIME {'PASS' if passed else 'FAIL'} {label}", flush=True)

for variable in ("Q_BIN", "G_BIN"):
    stage = stage_at(f"relative-{variable}", "op2")
    runtime = stage / "session"
    runtime.mkdir(mode=0o700)
    env = environment(stage, runtime, "wayland-owned")
    env[variable] = "./relative-bin"
    result = run_adapter(stage, env, "op2")
    passed = (result.returncode == 2 and "Q_BIN and G_BIN must be absolute executables" in result.stderr
              and not (stage / ".superpowers/g2-op2").exists())
    if not passed:
        failures.append(variable)
    print(f"NATIVE EXECUTABLE {'PASS' if passed else 'FAIL'} {variable}", flush=True)
print(f"evidence: {root}", flush=True)
if failures:
    sys.exit("NATIVE RUNTIME FAIL: " + ", ".join(failures))
print("NATIVE RUNTIME PASS 10 adapter/display pairs and 2 executable guards")
