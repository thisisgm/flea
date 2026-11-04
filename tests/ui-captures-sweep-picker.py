#!/usr/bin/env python3
"""Capture the candidate picker through the private public portal used by picker-native.py."""
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import tomllib


sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
from flea_ipc import Client
IPC = Client("qs")
IPC.export()
import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

REPO = Path(__file__).resolve().parent.parent
FRONTEND = "org.freedesktop.portal.Desktop"
BACKEND = "org.freedesktop.impl.portal.desktop.flea"
OBJECT = "/org/freedesktop/portal/desktop"
WAIT_SECONDS = 20
POLL_SECONDS = 0.1
CAPTURE_SETTLE_SECONDS = 0.4
CAPTURE_BODY_PX = 14
PICKER_WIDTH = 1040
PICKER_HEIGHT = 760
# Long enough to wrap the save collision line at the sweep's text size 14.
SAVE_COLLISION_NAME = "quarterly.report.june_2026.final.export.reviewed.pdf"
PROCESS_WAIT_SECONDS = 5
# Sample input: "hwkh1d5nbmt", the instance id Quickshell names its runtime directory by.
QS_INSTANCE_ID_CHARS = 11
# sockaddr_un.sun_path holds 108 bytes with its terminator.
SOCKET_PATH_MAX_BYTES = 107
# Hyprland's real signature is about 60 bytes, which would push its socket path past sockaddr_un in the private runtime.
HYPRLAND_ALIAS = "sweep"
root, theme_home, evidence = map(lambda value: Path(value).resolve(), sys.argv[1:])
run_root = Path(os.environ["FLEA_TEST_RUN_ROOT"]).resolve()
fixture_root = root.parent.parent
if not (fixture_root / ".flea-test-sandbox").is_file() or not root.is_relative_to(fixture_root):
    raise AssertionError("picker capture needs the suite's marked fixture root")
if not evidence.is_relative_to(run_root / "evidence") or not (run_root / ".flea-test-sandbox").is_file():
    raise AssertionError("picker captures must stay inside the suite evidence directory")
if not theme_home.is_relative_to(root.parent):
    raise AssertionError("picker theme HOME escaped the sweep fixture")
root.mkdir()
(root / ".flea-test-sandbox").write_text("private portal capture fixture\n")
drive_env = dict(os.environ)


def socket_fits(runtime):
    # Sample input: "<runtime>/quickshell/by-id/hwkh1d5nbmt/ipc.sock", the socket Quickshell binds for qs ipc.
    socket = f"{runtime}/quickshell/by-id/{'x' * QS_INSTANCE_ID_CHARS}/ipc.sock"
    return len(os.fsencode(socket)) <= SOCKET_PATH_MAX_BYTES


def private_runtime(parent):
    # The fixture root sits too deep for a socket path, so each picker run takes a private dir in the suite's run root.
    made = Path(tempfile.mkdtemp(prefix="picker-run-", dir=parent))
    if not socket_fits(made):
        raise AssertionError(f"picker runtime dir is too deep for an IPC socket: {made}")
    return made


def hyprland_environment(runtime, environment):
    # Quickshell looks for the compositor at $XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE, which the private runtime hides.
    instance = Path(environment["XDG_RUNTIME_DIR"]) / "hypr" / environment["HYPRLAND_INSTANCE_SIGNATURE"]
    if not instance.is_dir():
        raise AssertionError(f"the Hyprland instance directory is missing: {instance}")
    (runtime / "hypr").mkdir()
    (runtime / "hypr" / HYPRLAND_ALIAS).symlink_to(instance)
    return {"HYPRLAND_INSTANCE_SIGNATURE": HYPRLAND_ALIAS}


def picker_environment(runtime):
    # The picker's environment: private XDG dirs and runtime, the live Hyprland instance and display, the sweep's theme HOME.
    environment = dict(drive_env)
    for key, directory in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                           ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache")]:
        (root / directory).mkdir(mode=0o700)
        environment[key] = str(root / directory)
    environment["XDG_RUNTIME_DIR"] = str(runtime)
    environment.update(hyprland_environment(runtime, drive_env))
    environment["HOME"] = str(theme_home)
    environment["WAYLAND_DISPLAY"] = str(Path(drive_env["XDG_RUNTIME_DIR"]) / drive_env["WAYLAND_DISPLAY"])
    return environment


runtime = private_runtime(run_root)
picker_env = picker_environment(runtime)
picker_env["FLEA_BIN"] = str(Path(os.environ["FLEA_BIN"]).resolve(strict=True))
picker_env["FLEA_UI"] = str(REPO / "ui")
listing = root / "files"
listing.mkdir()
for name in ("alpha.txt", "beta.txt", "gamma.txt", SAVE_COLLISION_NAME):
    (listing / name).write_text(f"Sweep picker fixture: {name}\n")
portals = root / "portals"
portals.mkdir()
(portals / "flea.portal").write_text("[portal]\nDBusName=org.freedesktop.impl.portal.desktop.flea\nInterfaces=org.freedesktop.impl.portal.FileChooser;\n")
(portals / "portals.conf").write_text("[preferred]\norg.freedesktop.impl.portal.FileChooser=flea\n")
picker_env["XDG_DESKTOP_PORTAL_DIR"] = str(portals)
processes, logs, response = [], [], []
picker_pid = None
title = f"Flea sweep picker {os.getpid()}"


def fixture_foreground(home):
    # Sample input: foreground = "#DFE8E0" in the sweep's installed colors.toml.
    palette = tomllib.loads((home / ".local/state/omarchy/current/theme/colors.toml").read_text())
    return palette["foreground"].lower()


expected_foreground = fixture_foreground(theme_home)


def run(args, environment=drive_env, timeout=10):
    return subprocess.run([str(arg) for arg in args], env=environment, text=True,
                          capture_output=True, check=True, timeout=timeout).stdout.strip()


def start(args, name):
    log = (root / f"{name}.log").open("w")
    logs.append(log)
    process = subprocess.Popen([str(arg) for arg in args], env=picker_env, stdout=log,
                               stderr=subprocess.STDOUT, start_new_session=True)
    processes.append(process)
    return process


def wait(label, predicate):
    deadline = time.monotonic() + WAIT_SECONDS
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        observed = predicate()
        if observed:
            return observed
        time.sleep(POLL_SECONDS)
    raise AssertionError(f"picker sweep timed out: {label}")


def windows():
    # Sample input: [{"title":"Flea sweep picker 123","pid":456,"address":"0xabc"}].
    return json.loads(run(["hyprctl", "clients", "-j"]))


def owned_window():
    found = [window for window in windows() if window.get("title") == title and window.get("pid") == picker_pid]
    if len(found) != 1:
        raise AssertionError("owned picker window missing or ambiguous")
    # Sample input: b"HOME=/fixture/home\0FLEA_PICKER_REPLY=/fixture/run/reply\0".
    environment = dict(row.split(b"=", 1) for row in Path(f"/proc/{picker_pid}/environ").read_bytes().split(b"\0") if b"=" in row)
    reply = Path(os.fsdecode(environment.get(b"FLEA_PICKER_REPLY", b""))).resolve()
    if not reply.is_relative_to(runtime) or environment.get(b"HOME") != os.fsencode(theme_home):
        raise AssertionError("picker window has foreign reply or theme inputs")
    if environment.get(b"FLEA_BIN") != os.fsencode(picker_env["FLEA_BIN"]):
        raise AssertionError("picker window is not using the candidate binary")
    return found[0]


def state():
    owned_window()
    # Sample input: {"view":"grid","marksBusy":false,"marks":[],"themeLoaded":true,"themeForeground":"#dfe8e0"}.
    return json.loads(run([*IPC.command(), "--pid", picker_pid, "call", "fleapicker", "snapshot"], picker_env, 5))


def capture_state(mode):
    observed = {}

    def selected():
        nonlocal observed
        observed = state()
        return observed if not observed["marksBusy"] and len(observed["marks"]) == 1 else None

    try:
        wait(f"{mode} selected row", selected)
    except AssertionError as error:
        raise AssertionError(f"picker {mode} expected marksBusy=false and one mark; saw marksBusy={observed.get('marksBusy')} marks={observed.get('marks')}") from error
    if observed["body"] != CAPTURE_BODY_PX:
        raise AssertionError(f"picker effective font size is {observed['body']}, expected {CAPTURE_BODY_PX}")
    if not observed["themeLoaded"] or observed["themeForeground"].lower() != expected_foreground:
        raise AssertionError(f"picker theme expected ready foreground={expected_foreground}; saw themeLoaded={observed['themeLoaded']} themeForeground={observed['themeForeground']}")
    return observed


def press(*keys):
    owned_window()
    run(["omarchy-drive", "key", "--window", title, *keys])


def owner(name):
    return bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                         "NameHasOwner", GLib.Variant("(s)", (name,)), None,
                         Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]


def interrupted(signum, _frame):
    raise RuntimeError(f"picker sweep interrupted by signal {signum}")


for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
    signal.signal(signum, interrupted)


try:
    bus_log = (root / "bus.log").open("w")
    logs.append(bus_log)
    daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], env=picker_env,
                              stdout=subprocess.PIPE, stderr=bus_log, text=True, start_new_session=True)
    processes.append(daemon)
    address = daemon.stdout.readline().strip()
    if not address.startswith("unix:"):
        raise AssertionError("private session bus did not publish an address")
    picker_env["DBUS_SESSION_BUS_ADDRESS"] = address
    bus = Gio.DBusConnection.new_for_address_sync(address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT |
                                                 Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)
    start([sys.executable, REPO / "tools/flea-portal"], "backend")
    wait("candidate backend", lambda: owner(BACKEND))
    start(["/usr/lib/xdg-desktop-portal", "--verbose"], "frontend")
    wait("public frontend", lambda: owner(FRONTEND))
    version = bus.call_sync(FRONTEND, OBJECT, "org.freedesktop.DBus.Properties", "Get",
                           GLib.Variant("(ss)", ("org.freedesktop.portal.FileChooser", "version")),
                           None, Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
    if not isinstance(version, int) or version <= 0:
        raise AssertionError("public FileChooser interface unavailable")
    run([picker_env["FLEA_BIN"], "--ui-state", json.dumps({"keys": "default", "pickerView": "list",
         "display": {"textSize": {"mode": 14}}})], picker_env)
    token = "sweep"
    sender = bus.get_unique_name()[1:].replace(".", "_")
    handle = f"{OBJECT}/request/{sender}/{token}"
    subscription = bus.signal_subscribe(FRONTEND, "org.freedesktop.portal.Request", "Response", handle,
        None, Gio.DBusSignalFlags.NONE, lambda *args: response.append(args[-1].unpack()[0]))
    options = {"handle_token": GLib.Variant("s", token), "multiple": GLib.Variant("b", True),
               "current_folder": GLib.Variant("ay", os.fsencode(listing) + b"\0")}
    result = bus.call_sync(FRONTEND, OBJECT, "org.freedesktop.portal.FileChooser", "OpenFile",
                          GLib.Variant("(ssa{sv})", ("", title, options)), None,
                          Gio.DBusCallFlags.NONE, 10000, None)
    if result.unpack()[0] != handle:
        raise AssertionError("public portal returned a foreign request handle")
    client = wait("native picker", lambda: next((window for window in windows() if window.get("title") == title), None))
    picker_pid = client["pid"]
    client = owned_window()
    run(["omarchy-drive", "focus", title])
    wait("listing", lambda: state()["path"] == str(listing) and state()["total"] == 4 and state()["state"] != "loading")
    # Sample input: "0xabc123", an owned Hyprland window address.
    if not re.fullmatch(r"0x[0-9a-fA-F]+", client["address"]):
        raise AssertionError("invalid picker window address")
    if not client["floating"]:
        run(["omarchy-drive", "window", "float", title])
    answer = run(["bash", REPO / "tests/lib/hypr-dispatch.sh", "window_resize", client["address"], PICKER_WIDTH, PICKER_HEIGHT])
    if answer != "ok":
        raise AssertionError(f"picker resize refused: {answer}")
    run(["omarchy-drive", "window", "center", title])
    wait("picker viewport", lambda: state()["width"] == PICKER_WIDTH and state()["height"] == PICKER_HEIGHT)
    press("-k", "space")
    wait("marked first row", lambda: not state()["marksBusy"] and len(state()["marks"]) == 1)
    for mode, chord in (("list", "1"), ("grid", "3")):
        press("-M", "ctrl", "-k", chord, "-m", "ctrl")
        wait(mode, lambda: state()["view"] == mode)
        time.sleep(CAPTURE_SETTLE_SECONDS)
        observed = capture_state(mode)
        name = f"sweep-picker-{mode}-selected"
        png = evidence / f"{name}.png"
        if png.exists() or png.is_symlink():
            raise AssertionError(f"capture already exists: {png}")
        run(["omarchy-drive", "shot", png, title])
        if not png.is_file() or not png.stat().st_size:
            raise AssertionError(f"empty capture: {png}")
        dimensions = run(["magick", "identify", "-ping", "-format", "%wx%h", png])
        # Sample input: "1040x760", from magick identify -ping -format %wx%h.
        if not re.fullmatch(r"[1-9][0-9]*x[1-9][0-9]*", dimensions):
            raise AssertionError(f"invalid PNG dimensions: {dimensions}")
        (evidence / f"{name}.json").write_text(json.dumps(observed, indent=2))
        with (evidence / "manifest.tsv").open("a") as manifest:
            manifest.write(f"{name}\t{dimensions}\n")
        print(f"SWEEP {name} {dimensions}", flush=True)
    press("-k", "Escape")
    wait("cancel response", lambda: response)
    if response != [1]:
        raise AssertionError(f"picker cancellation answered {response}")
    wait("picker closed", lambda: not any(window.get("pid") == picker_pid for window in windows()))
    picker_pid = None
    bus.signal_unsubscribe(subscription)
    save_token = "sweepsave"
    save_sender = bus.get_unique_name()[1:].replace(".", "_")
    save_handle = f"{OBJECT}/request/{save_sender}/{save_token}"
    save_subscription = bus.signal_subscribe(FRONTEND, "org.freedesktop.portal.Request", "Response", save_handle,
        None, Gio.DBusSignalFlags.NONE, lambda *args: response.append(args[-1].unpack()[0]))
    save_options = {"handle_token": GLib.Variant("s", save_token),
                    "current_name": GLib.Variant("s", SAVE_COLLISION_NAME),
                    "current_folder": GLib.Variant("ay", os.fsencode(listing) + b"\0")}
    save_result = bus.call_sync(FRONTEND, OBJECT, "org.freedesktop.portal.FileChooser", "SaveFile",
                                GLib.Variant("(ssa{sv})", ("", title, save_options)), None,
                                Gio.DBusCallFlags.NONE, 10000, None)
    if save_result.unpack()[0] != save_handle:
        raise AssertionError("public portal returned a foreign save handle")
    client = wait("native save picker", lambda: next((window for window in windows() if window.get("title") == title), None))
    picker_pid = client["pid"]
    client = owned_window()
    run(["omarchy-drive", "focus", title])
    wait("save collision", lambda: state()["path"] == str(listing) and state()["saveName"] == SAVE_COLLISION_NAME
         and state()["collision"] and state()["state"] != "loading")
    answer = run(["bash", REPO / "tests/lib/hypr-dispatch.sh", "window_resize", client["address"], PICKER_WIDTH, PICKER_HEIGHT])
    if answer != "ok":
        raise AssertionError(f"picker save resize refused: {answer}")
    run(["omarchy-drive", "window", "center", title])
    wait("save picker viewport", lambda: state()["width"] == PICKER_WIDTH and state()["height"] == PICKER_HEIGHT)
    time.sleep(CAPTURE_SETTLE_SECONDS)
    observed = state()
    if observed["body"] != CAPTURE_BODY_PX:
        raise AssertionError(f"picker save effective font size is {observed['body']}, expected {CAPTURE_BODY_PX}")
    if not observed["themeLoaded"] or observed["themeForeground"].lower() != expected_foreground:
        raise AssertionError(f"picker save theme expected ready foreground={expected_foreground}; saw themeLoaded={observed['themeLoaded']} themeForeground={observed['themeForeground']}")
    name = "sweep-picker-save-collision"
    png = evidence / f"{name}.png"
    if png.exists() or png.is_symlink():
        raise AssertionError(f"capture already exists: {png}")
    run(["omarchy-drive", "shot", png, title])
    if not png.is_file() or not png.stat().st_size:
        raise AssertionError(f"empty capture: {png}")
    dimensions = run(["magick", "identify", "-ping", "-format", "%wx%h", png])
    # Sample input: "1040x760", from magick identify -ping -format %wx%h.
    if not re.fullmatch(r"[1-9][0-9]*x[1-9][0-9]*", dimensions):
        raise AssertionError(f"invalid PNG dimensions: {dimensions}")
    (evidence / f"{name}.json").write_text(json.dumps(observed, indent=2))
    with (evidence / "manifest.tsv").open("a") as manifest:
        manifest.write(f"{name}\t{dimensions}\n")
    print(f"SWEEP {name} {dimensions}", flush=True)
    press("-k", "Escape")
    wait("save cancel response", lambda: len(response) == 2)
    if response != [1, 1]:
        raise AssertionError(f"picker save cancellation answered {response}")
    wait("save picker closed", lambda: not any(window.get("pid") == picker_pid for window in windows()))
    picker_pid = None
    bus.signal_unsubscribe(save_subscription)
finally:
    # The private backend's process group owns its picker children, even after a failed IPC read.
    for process in reversed(processes):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        # Observe exit without reaping so the leader's PID stays reserved through SIGKILL.
        deadline = time.monotonic() + PROCESS_WAIT_SECONDS
        while os.waitid(os.P_PID, process.pid, os.WEXITED | os.WNOHANG | os.WNOWAIT) is None:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                break
            time.sleep(min(POLL_SECONDS, remaining))
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        process.wait(timeout=PROCESS_WAIT_SECONDS)
    for log in logs:
        log.close()
    for path in root.glob("*.log"):
        (evidence / f"picker-{path.name}").write_bytes(path.read_bytes())
