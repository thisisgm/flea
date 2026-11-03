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
PROCESS_WAIT_SECONDS = 5
# Seconds a hover or press gets to show in the reader, and the extra settle a hover or press shot waits beyond a keyboard one.
FLAG_WAIT_SECONDS = 3
POINTER_EXTRA_SECONDS = 0.3
# Sample input: "hwkh1d5nbmt", the instance id Quickshell names its runtime directory by.
QS_INSTANCE_ID_CHARS = 11
# sockaddr_un.sun_path holds 108 bytes with its terminator.
SOCKET_PATH_MAX_BYTES = 107
HYPRLAND_ALIAS = "cohesion"
root, theme_home, evidence = map(lambda value: Path(value).resolve(), sys.argv[1:4])
coh_theme = sys.argv[4]
# Keyboard stops one walk takes at most, pointer targets at most, and how far a held press moves off its control.
COH_TAB_LIMIT = 14
COH_TARGET_LIMIT = 8
COH_AWAY_PX = 150
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
picker_env = dict(drive_env)
for key, directory in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                       ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache")]:
    (root / directory).mkdir(mode=0o700)
    picker_env[key] = str(root / directory)


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


runtime = private_runtime(run_root)
picker_env["XDG_RUNTIME_DIR"] = str(runtime)
# Quickshell finds the compositor under the runtime dir, which the private one hides; a short alias keeps the socket path inside sockaddr_un.
instance = Path(drive_env["XDG_RUNTIME_DIR"]) / "hypr" / drive_env["HYPRLAND_INSTANCE_SIGNATURE"]
if not instance.is_dir():
    raise AssertionError(f"the Hyprland instance directory is missing: {instance}")
(runtime / "hypr").mkdir()
(runtime / "hypr" / HYPRLAND_ALIAS).symlink_to(instance)
picker_env["HYPRLAND_INSTANCE_SIGNATURE"] = HYPRLAND_ALIAS
picker_env["HOME"] = str(theme_home)
picker_env["WAYLAND_DISPLAY"] = str(Path(drive_env["XDG_RUNTIME_DIR"]) / drive_env["WAYLAND_DISPLAY"])
picker_env["FLEA_BIN"] = str(Path(os.environ["FLEA_BIN"]).resolve(strict=True))
picker_env["FLEA_UI"] = str(Path(os.environ["FLEA_UI"]).resolve(strict=True))
listing = root / "files"
listing.mkdir()
for name in ("alpha.txt", "beta.txt", "gamma.txt"):
    (listing / name).write_text(f"Cohesion picker fixture: {name}\n")
portals = root / "portals"
portals.mkdir()
(portals / "flea.portal").write_text("[portal]\nDBusName=org.freedesktop.impl.portal.desktop.flea\nInterfaces=org.freedesktop.impl.portal.FileChooser;\n")
(portals / "portals.conf").write_text("[preferred]\norg.freedesktop.impl.portal.FileChooser=flea\n")
picker_env["XDG_DESKTOP_PORTAL_DIR"] = str(portals)
processes, logs, response, missed = [], [], [], []
picker_pid = None
title = f"Flea cohesion picker {os.getpid()}"


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
    wait("listing", lambda: state()["path"] == str(listing) and state()["total"] == 3 and state()["state"] != "loading")
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
    capture_state("cohesion")
    # The picker's keyboard stops, then one control per look hovered and held.
    ydotool_env = dict(drive_env, YDOTOOL_SOCKET=f"{os.environ['XDG_RUNTIME_DIR']}/.ydotool_socket")
    manifest = (evidence / "coh-manifest.tsv").open("a")

    def ydo(*args, strict=True):
        return subprocess.run(["ydotool", *args], env=ydotool_env, capture_output=True, timeout=5, check=strict).returncode == 0

    # Sample input: {"focus":{"kind":"DialogButton","name":"Open","x":900,"y":700,"via":"owner"},"controls":[{"kind":"DialogButton","name":"Open","x":900,"y":700,"w":80,"h":30,"hovered":true,"hoverable":true}]}
    def controls():
        owned_window()
        return json.loads(run([*IPC.command(), "--pid", picker_pid, "call", "fleapicker", "cohesionControls"], picker_env, 5))

    def sig(entry):
        return f"{entry['kind']}|{entry['name']}|{entry['x']}|{entry['y']}" if entry else "none"

    def pointer_to(x, y):
        wx, wy = owned_window()["at"]
        run(["bash", "-c", 'source "$1"; pointer_glide "$2" "$3"', "capture-pointer",
             REPO / "tests/lib/pointer-glide.sh", wx + x, wy + y])

    def park():
        pointer_to(6, PICKER_HEIGHT - 6)
        time.sleep(CAPTURE_SETTLE_SECONDS)

    def flag_seen(entry, flag):
        # A kind with no hovered or pressed property has no flag to wait for.
        if not entry["hoverable" if flag == "hovered" else "pressable"]:
            return True
        deadline = time.monotonic() + FLAG_WAIT_SECONDS
        while time.monotonic() < deadline:
            if any(each["kind"] == entry["kind"] and each["name"] == entry["name"] and each[flag]
                   and each["x"] <= entry["targetX"] < each["x"] + each["w"]
                   and each["y"] <= entry["targetY"] < each["y"] + each["h"]
                   for each in controls().get("controls", [])):
                return True
            time.sleep(POLL_SECONDS)
        print(f"COH_MISS {coh_theme} picker {sig(entry)} never reported {flag}", flush=True)
        missed.append(f"{sig(entry)} {flag}")
        return False

    def coh_shot(name, phase, index, target):
        png = evidence / f"{name}.png"
        if png.exists() or png.is_symlink():
            raise AssertionError(f"capture already exists: {png}")
        sidecar = json.dumps(controls())
        run(["omarchy-drive", "shot", png, title])
        (evidence / f"{name}.json").write_text(sidecar)
        manifest.write(f"{name}\t{coh_theme}\tpicker\t{phase}\t{index}\t{target}\n")
        manifest.flush()
        print(f"COH_PICKER {name}", flush=True)

    first = None
    for step in range(COH_TAB_LIMIT):
        here = sig(controls().get("focus"))
        if step > 0 and here == first:
            break
        if step == 0:
            first = here
        park()
        coh_shot(f"cohesion-{coh_theme}-picker-focus-{step}", "focus", step, here)
        press("-k", "Tab")
        time.sleep(CAPTURE_SETTLE_SECONDS)
    looks, targets = set(), []
    for entry in controls().get("controls", []):
        look = (entry["kind"], entry["primary"], entry["destructive"], entry["member"], entry["value"], entry["enabled"])
        if entry["exposed"] and entry["w"] > 2 and entry["h"] > 2 and look not in looks:
            looks.add(look)
            targets.append(entry)
    closed = False
    for index, entry in enumerate(targets[:COH_TARGET_LIMIT]):
        pointer_to(entry["targetX"], entry["targetY"])
        if not entry["enabled"]:
            coh_shot(f"cohesion-{coh_theme}-picker-disabled-{index}", "disabled", index, sig(entry))
            park()
            continue
        if not flag_seen(entry, "hovered"):
            park()
            continue
        time.sleep(CAPTURE_SETTLE_SECONDS + POINTER_EXTRA_SECONDS)
        coh_shot(f"cohesion-{coh_theme}-picker-hover-{index}", "hover", index, sig(entry))
        # Rows take hover only, as in the window walk, so a press cannot move the picker away.
        if entry["kind"] in ("MenuRow", "SidebarRow"):
            park()
            continue
        if not entry["pressLive"]:
            park()
            continue
        # The release always follows the press, off the control, whatever fails in between.
        try:
            ydo("click", "0x40")
            if flag_seen(entry, "pressed"):
                time.sleep(CAPTURE_SETTLE_SECONDS + POINTER_EXTRA_SECONDS)
                coh_shot(f"cohesion-{coh_theme}-picker-press-{index}", "press", index, sig(entry))
        finally:
            try:
                pointer_to(6, PICKER_HEIGHT - 6)
            finally:
                ydo("click", "0x80")
        time.sleep(CAPTURE_SETTLE_SECONDS)
        if not any(window.get("pid") == picker_pid for window in windows()):
            print(f"COH_PICKER closed after {sig(entry)}", flush=True)
            closed = True
            break
    manifest.close()
    if not closed:
        press("-k", "Escape")
        wait("cancel response", lambda: response)
        wait("picker closed", lambda: not any(window.get("pid") == picker_pid for window in windows()))
    picker_pid = None
    bus.signal_unsubscribe(subscription)
    if missed:
        raise AssertionError(f"picker never reached {len(missed)} hover or press states: {missed}")
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
        try:
            process.wait(timeout=PROCESS_WAIT_SECONDS)
        except subprocess.TimeoutExpired:
            print(f"COH_CLEANUP {coh_theme} process {process.pid} outlived SIGKILL", flush=True)
    for log in logs:
        log.close()
    for path in root.glob("*.log"):
        (evidence / f"picker-{coh_theme}-{path.name}").write_bytes(path.read_bytes())
