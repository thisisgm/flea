#!/usr/bin/env python3
"""Exercise activation and idle handover with the real backend on a private bus."""

import json
import os
from pathlib import Path
import signal
import shlex
import shutil
import subprocess
import sys
import tempfile
import threading
import time


REPO = Path(__file__).resolve().parent.parent
NAME = "org.freedesktop.impl.portal.desktop.flea"
OBJECT = "/org/freedesktop/portal/desktop"
IFACE = "org.freedesktop.impl.portal.FileChooser"
MARKER = ".flea-test-sandbox"
IDLE_BOUND_SEC = 1
# Allow 300 ms for second-timer coalescing and the activation/PID query round trips.
IDLE_TOLERANCE_SEC = 0.3
# Bound private-bus monitor setup so a stalled daemon cannot hang the lifetime checks.
MONITOR_SETUP_TIMEOUT_MS = 3000


def isolated():
    scratch = REPO / ".superpowers/k123"
    scratch.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="portal-lifetime.", dir=scratch)).resolve()
    (root / MARKER).touch()
    session = None
    try:
        for directory in ["home", "config/xdg-desktop-portal", "data/dbus-1/services", "run"]:
            (root / directory).mkdir(parents=True, exist_ok=True)
        (root / "run").chmod(0o700)
        (root / "config/xdg-desktop-portal/portals.conf").write_text(
            "[preferred]\norg.freedesktop.impl.portal.FileChooser=flea;gtk\n")
        (root / "backend").write_text(
            "#!/usr/bin/python3\nimport json, pathlib, runpy, subprocess, sys, time\n"
            "if len(sys.argv) == 2:\n"
            "    child = subprocess.Popen([sys.executable, __file__, sys.argv[1], 'serve'])\n"
            "    status = child.wait()\n"
            f"    record = pathlib.Path({str(root / 'run')!r}, 'exit-' + str(child.pid))\n"
            "    pending = record.with_suffix('.new')\n"
            "    pending.write_text(json.dumps({'status': status, 'at': time.monotonic()}))\n"
            "    pending.replace(record)\n"
            "    sys.exit(status)\n"
            "module = runpy.run_path(sys.argv[1])\n"
            f"module['main'].__globals__['IDLE_EXIT_SEC'] = {IDLE_BOUND_SEC}\n"
            "if sys.argv[2] == 'fail-export':\n"
            "    acquired = module['Service'].on_bus_acquired\n"
            "    class BrokenConnection:\n"
            "        def register_object(self, *args):\n"
            "            raise TypeError('simulated registration failure')\n"
            "    def fail_export(service, connection, name):\n"
            "        acquired(service, BrokenConnection(), name)\n"
            "    module['Service'].on_bus_acquired = fail_export\n"
            "sys.exit(module['main']())\n")
        (root / "backend").chmod(0o700)
        service = (REPO / f"packaging/{NAME}.service").read_text()
        command = " ".join(shlex.quote(str(p)) for p in [root / "backend", REPO / "tools/flea-portal"])
        service = "\n".join("Exec=" + command if line.startswith("Exec=") else line for line in service.splitlines()) + "\n"
        (root / f"data/dbus-1/services/{NAME}.service").write_text(service)
        portal = root / "data/xdg-desktop-portal/portals"
        portal.mkdir(parents=True)
        shutil.copyfile(REPO / "packaging/flea.portal", portal / "flea.portal")
        (root / "picker").write_text(
            "#!/usr/bin/python3\nimport os, pathlib, sys, time\n"
            "pathlib.Path(os.environ['XDG_RUNTIME_DIR'], 'picked').touch()\n"
            "time.sleep(1.5)\npathlib.Path(sys.argv[2]).write_text('{\"response\": 1}')\n")
        (root / "picker").chmod(0o700)
        env = dict(os.environ, HOME=str(root / "home"), XDG_CONFIG_HOME=str(root / "config"),
                   XDG_DATA_HOME=str(root / "data"), XDG_DATA_DIRS=str(root / "empty"),
                   XDG_RUNTIME_DIR=str(root / "run"), FLEA_BIN=str(root / "picker"),
                   FLEA_PORTAL_LIFETIME_ROOT=str(root))
        env.pop("DBUS_SESSION_BUS_ADDRESS", None)
        session = subprocess.Popen(["dbus-run-session", "--", sys.executable, __file__, str(root)],
                                   env=env, start_new_session=True)
        return session.wait(timeout=35)
    finally:
        if session is not None:
            try:
                os.killpg(session.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            session.wait(timeout=5)
        if not (root.is_absolute() and root != scratch and root.parent == scratch.resolve() and (root / MARKER).is_file()):
            raise RuntimeError("refusing to remove an unmarked lifetime sandbox")
        shutil.rmtree(root)


def exercise(root):
    import gi
    gi.require_version("Gio", "2.0")
    from gi.repository import Gio, GLib

    if not (root.is_absolute() and root.parent == (REPO / ".superpowers/k123").resolve()
            and (root / MARKER).is_file() and os.environ.get("FLEA_PORTAL_LIFETIME_ROOT") == str(root)):
        raise RuntimeError("the lifetime checks require their own marked private-bus sandbox")
    bus = Gio.bus_get_sync(Gio.BusType.SESSION, None)
    failures = []

    def check(label, passed, detail=""):
        print(f"{'ok' if passed else 'FAIL'} {label}: {detail}", flush=True)
        if not passed:
            failures.append(label)

    def call(name, interface, method, params=None):
        return bus.call_sync(name, OBJECT, interface, method, params, None,
                             Gio.DBusCallFlags.NONE, 3000, None).unpack()

    def daemon(method, params):
        return bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                             method, params, None, Gio.DBusCallFlags.NONE, 3000, None).unpack()

    def owner():
        try:
            return daemon("GetNameOwner", GLib.Variant("(s)", (NAME,)))[0]
        except GLib.Error:
            return ""

    def activate():
        start = time.monotonic()
        daemon("StartServiceByName", GLib.Variant("(su)", (NAME, 0)))
        held = owner()
        check("activation owns the name within 3 s", bool(held) and time.monotonic() - start < 3,
              f"{(time.monotonic() - start) * 1000:.1f} ms, {held}")
        return held

    def properties(name=NAME):
        start = time.monotonic()
        value = call(name, "org.freedesktop.DBus.Properties", "GetAll", GLib.Variant("(s)", (IFACE,)))[0]
        check("startup GetAll answers version promptly", value == {"version": 3} and time.monotonic() - start < 1,
              f"{(time.monotonic() - start) * 1000:.1f} ms, {value}")

    with (root / "export-failure.log").open("w") as log:
        broken = subprocess.Popen([str(root / "backend"), str(REPO / "tools/flea-portal"), "fail-export"], stderr=log)
        acquired_name = False
        deadline = time.monotonic() + 3
        while broken.poll() is None and time.monotonic() < deadline:
            acquired_name |= bool(owner())
            time.sleep(0.01)
        if broken.poll() is None:
            broken.kill()
        status = broken.wait(timeout=3)
    check("failed export never owns a name or leaves activation hanging", status == 1 and not acquired_name,
          f"exit={status}, acquired={acquired_name}")
    check("failed export explains the refusal", "could not export FileChooser" in (root / "export-failure.log").read_text())

    events = []
    event_lock = threading.Lock()

    def observe(connection, message, incoming, user_data):
        kind = message.get_message_type()
        if incoming and kind in (Gio.DBusMessageType.METHOD_CALL, Gio.DBusMessageType.SIGNAL):
            body = message.get_body().unpack() if message.get_body() is not None else ()
            event = None
            if (kind == Gio.DBusMessageType.METHOD_CALL and message.get_destination() == "org.freedesktop.DBus"
                    and message.get_path() == "/org/freedesktop/DBus"
                    and message.get_interface() == "org.freedesktop.DBus"
                    and message.get_member() == "ReleaseName" and body == (NAME,)):
                event = ("release", message.get_sender())
            elif (kind == Gio.DBusMessageType.SIGNAL and message.get_sender() == "org.freedesktop.DBus"
                    and message.get_interface() == "org.freedesktop.DBus"
                    and message.get_member() == "NameOwnerChanged" and len(body) == 3
                    and body[0] == body[1] and body[0].startswith(":") and body[2] == ""):
                event = ("closed", body[0])
            if event is not None:
                with event_lock:
                    events.append(event)
            # Consume mirrored calls so GDBus never replies on the monitor connection.
            return None
        return message

    monitor = Gio.DBusConnection.new_for_address_sync(
        os.environ["DBUS_SESSION_BUS_ADDRESS"],
        Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION,
        None, None)
    monitor.add_filter(observe, None)
    try:
        rules = ["type='method_call',interface='org.freedesktop.DBus',member='ReleaseName'",
                 "type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'"]
        monitor.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus.Monitoring",
                          "BecomeMonitor", GLib.Variant("(asu)", (rules, 0)), None,
                          Gio.DBusCallFlags.NONE, MONITOR_SETUP_TIMEOUT_MS, None)
        held = activate()
        start = time.monotonic()
        pid = daemon("GetConnectionUnixProcessID", GLib.Variant("(s)", (held,)))[0]
        exit_record = root / f"run/exit-{pid}"
        last_held = 0.0
        released = None
        ordered = ()
        deadline = start + 4
        while time.monotonic() < deadline:
            polled_at = time.monotonic() - start
            if owner() == held:
                last_held = polled_at
            elif released is None:
                # The last query sent while the bus still held the name bounds release from below, even after a prompt reap.
                released = last_held
            with event_lock:
                ordered = tuple(kind for kind, sender in events if sender == held)
            if released is not None and exit_record.exists() and "closed" in ordered:
                break
            time.sleep(0.01)
        record = json.loads(exit_record.read_text()) if exit_record.exists() else {}
        elapsed = record.get("at", time.monotonic()) - start
        minimum = IDLE_BOUND_SEC - IDLE_TOLERANCE_SEC
        explicit_release = "release" in ordered and "closed" in ordered and ordered.index("release") < ordered.index("closed")
        check("an unused activation releases its name and exits after the idle bound",
              explicit_release and released is not None and minimum <= released <= elapsed <= 4
              and elapsed >= minimum and record.get("status") == 0 and not owner(),
              f"release_after={released}, elapsed={elapsed:.3f} s, idle={IDLE_BOUND_SEC} s, "
              f"tolerance={IDLE_TOLERANCE_SEC} s, exit={record.get('status')}, pid={pid}, bus_order={ordered}")
    finally:
        monitor.close_sync(None)
    held = activate()
    properties()
    check("Get reads the interface version", call(NAME, "org.freedesktop.DBus.Properties", "Get",
          GLib.Variant("(ss)", (IFACE, "version"))) == (3,))
    check("startup introspection answers", IFACE in call(NAME, "org.freedesktop.DBus.Introspectable", "Introspect")[0])
    check("startup Ping answers", call(NAME, "org.freedesktop.DBus.Peer", "Ping") == ())
    check("startup never opens the picker", not (root / "run/picked").exists())
    for _ in range(6):
        time.sleep(0.4)
        properties()
    check("startup property reads renew the idle window", owner() == held, owner())

    # A frontend proxy can still address its cached unique owner while the well-known name is released.
    old = activate()
    properties()
    deadline = time.monotonic() + 3
    while owner() == old and time.monotonic() < deadline:
        time.sleep(0.01)
    check("an idle backend releases its well-known name", owner() != old)
    params = GLib.Variant("(osssa{sv})", ("/org/freedesktop/portal/desktop/request/late", "test.app", "", "Late call", {}))
    start = time.monotonic()
    try:
        answer = call(old, IFACE, "OpenFile", params)
        check("a call during idle handover finishes before the backend quits", answer == (1, {}),
              f"{(time.monotonic() - start) * 1000:.1f} ms, {answer}")
    except GLib.Error as error:
        check("a call during idle handover finishes before the backend quits", False, str(error))
    held = activate()
    properties()
    answer = call(NAME, IFACE, "OpenFile", params)
    check("a picker outlives the idle interval", answer == (1, {}) and owner() == held, str(answer))
    print(f"portal-startup-lifetime: {len(failures)} failures", flush=True)
    return int(bool(failures))


if __name__ == "__main__":
    sys.exit(exercise(Path(sys.argv[1])) if len(sys.argv) == 2 else isolated())
