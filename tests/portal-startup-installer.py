#!/usr/bin/env python3
"""Verify terminal restart reporting without contacting any session services."""

import errno
import os
from pathlib import Path
import pty
import shutil
import subprocess
import sys
import tempfile


REPO = Path(__file__).resolve().parent.parent
BINARY = Path(os.environ.get("FLEA_BIN") or REPO / "target/release/flea").resolve()
if not BINARY.is_file() or not os.access(BINARY, os.X_OK):
    sys.exit(f"portal-startup-installer: installer binary is missing or not executable: {BINARY}")
print(f"portal-startup-installer: testing {BINARY}", flush=True)
SCRATCH = REPO / ".superpowers/k123"
SCRATCH.mkdir(parents=True, exist_ok=True)
ROOT = Path(tempfile.mkdtemp(prefix="portal-installer.", dir=SCRATCH)).resolve()
MARKER = ROOT / ".flea-test-sandbox"
MARKER.touch()


def run(env, args, terminal):
    if not terminal:
        out = subprocess.run([str(BINARY), *args], env=env,
                             stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=25)
        return out.returncode, out.stdout.decode()
    master, slave = pty.openpty()
    child = subprocess.Popen([str(BINARY), *args], env=env, stdout=slave, stderr=slave)
    os.close(slave)
    data = b""
    try:
        while True:
            try:
                chunk = os.read(master, 4096)
            except OSError as error:
                if error.errno != errno.EIO:
                    raise
                break
            if not chunk:
                break
            data += chunk
        return child.wait(timeout=25), data.decode().replace("\r", "")
    finally:
        os.close(master)
        if child.poll() is None:
            child.kill()
            child.wait()


def check(label, passed, detail):
    print(f"{'ok' if passed else 'FAIL'} {label}", flush=True)
    if not passed:
        raise AssertionError(detail)


try:
    for directory in ["home", "bin", "config/hypr", "data/applications", "data/dbus-1/services", "data/xdg-desktop-portal/portals"]:
        (ROOT / directory).mkdir(parents=True, exist_ok=True)
    (ROOT / "config/hypr/bindings.lua").write_text("-- stock bindings\n")
    (ROOT / "data/applications/com.thisisgm.flea.desktop").write_text("[Desktop Entry]\nName=Flea\nExec=flea\nType=Application\n")
    (ROOT / "data/xdg-desktop-portal/portals/flea.portal").write_text((REPO / "packaging/flea.portal").read_text())
    (ROOT / "data/dbus-1/services/com.thisisgm.flea.FileManager1.service").write_text(
        "[D-BUS Service]\nName=org.freedesktop.FileManager1\nExec=/bin/false\n")
    scripts = {
        "systemctl": '#!/bin/sh\nprintf "restart %s\\n" "$*" >> "$PORTAL_TEST_ROOT/calls"\nexit "$(cat "$PORTAL_TEST_ROOT/restart-status")"\n',
        "busctl": '#!/bin/sh\nprintf "probe %s\\n" "$*" >> "$PORTAL_TEST_ROOT/calls"\ncat "$PORTAL_TEST_ROOT/probe-reply"\nexit "$(cat "$PORTAL_TEST_ROOT/probe-status")"\n',
        "hyprctl": "#!/bin/sh\nexit 1\n",
        "xdg-mime": '#!/bin/sh\nif [ "$1" = query ]; then cat "$PORTAL_TEST_ROOT/mime"; else printf "%s\\n" "$2" > "$PORTAL_TEST_ROOT/mime"; fi\n',
    }
    for name, source in scripts.items():
        path = ROOT / "bin" / name
        path.write_text(source)
        path.chmod(0o700)
    env = dict(os.environ, HOME=str(ROOT / "home"), XDG_CONFIG_HOME=str(ROOT / "config"),
               XDG_DATA_HOME=str(ROOT / "data"), XDG_DATA_DIRS=str(ROOT / "empty"),
               XDG_CURRENT_DESKTOP="Hyprland", PATH=str(ROOT / "bin") + ":/usr/bin:/bin", PORTAL_TEST_ROOT=str(ROOT))
    for key in ["DBUS_SESSION_BUS_ADDRESS", "HYPRLAND_INSTANCE_SIGNATURE", "WAYLAND_DISPLAY", "DISPLAY"]:
        env.pop(key, None)
    (ROOT / "restart-status").write_text("0\n")
    (ROOT / "probe-status").write_text("0\n")
    (ROOT / "probe-reply").write_text("u 4\n")
    (ROOT / "mime").write_text("other.desktop\n")
    status, text = run(env, ["--picker"], False)
    check("piped picker gives the restart command without contacting services", status == 0 and "systemctl --user restart" in text and not (ROOT / "calls").exists(), text)
    for command in ["--picker", "--default"]:
        (ROOT / "calls").write_text("")
        status, text = run(env, [command], True)
        calls = (ROOT / "calls").read_text().splitlines()
        check(f"{command} reports success only after the frontend answers", status == 0 and "file dialogs follow now" in text and len(calls) == 2 and calls[0].startswith("restart ") and calls[1].startswith("probe "), (text, calls))
        check("readiness never activates another frontend", "--auto-start=no --timeout=5s get-property org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.portal.FileChooser version" in calls[1], calls)
    for reply, probe_status in [("u 0\n", "0"), ("u 4\n", "1"), ("", "0")]:
        (ROOT / "probe-reply").write_text(reply)
        (ROOT / "probe-status").write_text(probe_status)
        status, text = run(env, ["--picker"], True)
        check("a missing or unready frontend prints recovery commands", status == 0 and "file dialogs follow now" not in text and "systemctl --user restart xdg-desktop-portal" in text and "journalctl --user -u xdg-desktop-portal.service -b --no-pager" in text, text)
    (ROOT / "calls").write_text("")
    (ROOT / "restart-status").write_text("1")
    status, text = run(env, ["--picker"], True)
    check("a refused restart is never probed or called successful", status == 0 and "file dialogs follow now" not in text and len((ROOT / "calls").read_text().splitlines()) == 1, text)
    check("routing retains the gtk fallback", "org.freedesktop.impl.portal.FileChooser=flea;gtk" in (ROOT / "config/xdg-desktop-portal/portals.conf").read_text(), text)
    print("portal-startup-installer: all checks passed", flush=True)
finally:
    if not (ROOT.is_absolute() and ROOT.parent == SCRATCH.resolve() and ROOT != SCRATCH and MARKER.is_file()):
        raise RuntimeError("refusing to remove an unmarked installer sandbox")
    shutil.rmtree(ROOT)
