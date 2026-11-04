#!/usr/bin/env python3
"""Shoot the picker states the button look capture needs, through the private public portal ui-captures-cohesion-picker.py uses."""
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
SETTLE_SECONDS = 0.5
PICKER_WIDTH = 1040
PICKER_HEIGHT = 760
PROCESS_WAIT_SECONDS = 5
# Keyboard stops a walk takes at most before it gives up on a control, and the text size every shot is taken at.
TAB_LIMIT = 16
CAPTURE_BODY_PX = 14
# Seconds a hover shot waits for the reader to show the hover.
FLAG_WAIT_SECONDS = 3
# Sample input: "hwkh1d5nbmt", the instance id Quickshell names its runtime directory by.
QS_INSTANCE_ID_CHARS = 11
# sockaddr_un.sun_path holds 108 bytes with its terminator.
SOCKET_PATH_MAX_BYTES = 107
HYPRLAND_ALIAS = "b72"
root, theme_home, evidence = map(lambda value: Path(value).resolve(), sys.argv[1:4])
theme = sys.argv[4]
run_root = Path(os.environ["FLEA_TEST_RUN_ROOT"]).resolve()
fixture_root = root.parent.parent
if not (fixture_root / ".flea-test-sandbox").is_file() or not root.is_relative_to(fixture_root):
    raise AssertionError("b72 picker capture needs the suite's marked fixture root")
if not evidence.is_relative_to(run_root / "evidence") or not (run_root / ".flea-test-sandbox").is_file():
    raise AssertionError("b72 picker captures must stay inside the suite evidence directory")
if not theme_home.is_relative_to(root.parent):
    raise AssertionError("b72 picker theme HOME escaped the fixture")
root.mkdir()
(root / ".flea-test-sandbox").write_text("private portal capture fixture\n")
drive_env = dict(os.environ)
picker_env = dict(drive_env)
for key, directory in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"),
                       ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache")]:
    (root / directory).mkdir(mode=0o700)
    picker_env[key] = str(root / directory)
runtime = Path(tempfile.mkdtemp(prefix="picker-run-", dir=run_root))
if len(os.fsencode(f"{runtime}/quickshell/by-id/{'x' * QS_INSTANCE_ID_CHARS}/ipc.sock")) > SOCKET_PATH_MAX_BYTES:
    raise AssertionError(f"picker runtime dir is too deep for an IPC socket: {runtime}")
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
    (listing / name).write_text(f"b72 picker fixture: {name}\n")
subprocess.run(["magick", "-size", "64x64", "xc:#336699", str(listing / "photo.png")], env=picker_env, check=True)
portals = root / "portals"
portals.mkdir()
(portals / "flea.portal").write_text("[portal]\nDBusName=org.freedesktop.impl.portal.desktop.flea\nInterfaces=org.freedesktop.impl.portal.FileChooser;\n")
(portals / "portals.conf").write_text("[preferred]\norg.freedesktop.impl.portal.FileChooser=flea\n")
picker_env["XDG_DESKTOP_PORTAL_DIR"] = str(portals)
processes, logs, missed = [], [], []
manifest = (evidence / "b72-manifest.tsv").open("a")
# Sample input: foreground = "#DFE8E0" in the fixture's installed colors.toml.
expected_foreground = tomllib.loads((theme_home / ".local/state/omarchy/current/theme/colors.toml").read_text())["foreground"].lower()


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
    raise AssertionError(f"b72 picker timed out: {label}")


def windows():
    # Sample input: [{"title":"Flea b72 picker 123 open","pid":456,"address":"0xabc"}].
    return json.loads(run(["hyprctl", "clients", "-j"]))


def owner(name):
    return bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
                         "NameHasOwner", GLib.Variant("(s)", (name,)), None,
                         Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]


class Request:
    def __init__(self, token, method, **options):
        self.title = f"Flea b72 picker {os.getpid()} {token}"
        self.response = []
        sender = bus.get_unique_name()[1:].replace(".", "_")
        handle = f"{OBJECT}/request/{sender}/{token}"
        self.subscription = bus.signal_subscribe(FRONTEND, "org.freedesktop.portal.Request", "Response", handle,
            None, Gio.DBusSignalFlags.NONE, lambda *args: self.response.append(args[-1].unpack()[0]))
        options = {"handle_token": GLib.Variant("s", token),
                   "current_folder": GLib.Variant("ay", os.fsencode(listing) + b"\0"), **options}
        result = bus.call_sync(FRONTEND, OBJECT, "org.freedesktop.portal.FileChooser", method,
                               GLib.Variant("(ssa{sv})", ("", self.title, options)), None,
                               Gio.DBusCallFlags.NONE, 10000, None)
        if result.unpack()[0] != handle:
            raise AssertionError("public portal returned a foreign request handle")
        client = wait("native picker", lambda: next((window for window in windows() if window.get("title") == self.title), None))
        self.pid = client["pid"]
        client = self.window()
        run(["omarchy-drive", "focus", self.title])
        wait("listing", lambda: self.state()["path"] == str(listing) and self.state()["state"] != "loading")
        # Sample input: "0xabc123", an owned Hyprland window address.
        if not re.fullmatch(r"0x[0-9a-fA-F]+", client["address"]):
            raise AssertionError("invalid picker window address")
        if not client["floating"]:
            run(["omarchy-drive", "window", "float", self.title])
        answer = run(["bash", REPO / "tests/lib/hypr-dispatch.sh", "window_resize", client["address"], PICKER_WIDTH, PICKER_HEIGHT])
        if answer != "ok":
            raise AssertionError(f"picker resize refused: {answer}")
        run(["omarchy-drive", "window", "center", self.title])
        wait("picker viewport", lambda: self.state()["width"] == PICKER_WIDTH and self.state()["height"] == PICKER_HEIGHT)
        observed = self.state()
        if observed["body"] != CAPTURE_BODY_PX:
            raise AssertionError(f"picker effective font size is {observed['body']}, expected {CAPTURE_BODY_PX}")
        if not observed["themeLoaded"] or observed["themeForeground"].lower() != expected_foreground:
            raise AssertionError(f"picker theme expected {expected_foreground}, saw {observed['themeForeground']}")

    def window(self):
        found = [window for window in windows() if window.get("title") == self.title and window.get("pid") == self.pid]
        if len(found) != 1:
            raise AssertionError("owned picker window missing or ambiguous")
        # Sample input: b"HOME=/fixture/home\0FLEA_PICKER_REPLY=/fixture/run/reply\0".
        environment = dict(row.split(b"=", 1) for row in Path(f"/proc/{self.pid}/environ").read_bytes().split(b"\0") if b"=" in row)
        reply = Path(os.fsdecode(environment.get(b"FLEA_PICKER_REPLY", b""))).resolve()
        if not reply.is_relative_to(runtime) or environment.get(b"HOME") != os.fsencode(theme_home):
            raise AssertionError("picker window has foreign reply or theme inputs")
        if environment.get(b"FLEA_BIN") != os.fsencode(picker_env["FLEA_BIN"]):
            raise AssertionError("picker window is not using the candidate binary")
        return found[0]

    def ipc(self, call):
        self.window()
        return json.loads(run([*IPC.command(), "--pid", self.pid, "call", "fleapicker", call], picker_env, 5))

    def state(self):
        return self.ipc("snapshot")

    def press(self, *keys):
        self.window()
        run(["omarchy-drive", "key", "--window", self.title, *keys])
        time.sleep(SETTLE_SECONDS)

    def control(self, name):
        found = [item for item in self.state()["controls"] if item["name"] == name and item["visible"]]
        if not found:
            found = [dict(item, centre=f"{item['targetX']} {item['targetY']}")
                     for item in self.ipc("cohesionControls")["controls"] if item["name"] == name and item["exposed"]]
        if not found:
            raise AssertionError(f"picker shows no {name}")
        return found[-1]

    def pointer(self, name, click=False):
        x, y = map(int, self.control(name)["centre"].split())
        wx, wy = self.window()["at"]
        run(["bash", "-c", 'source "$1"; pointer_glide "$2" "$3"', "capture-pointer",
             REPO / "tests/lib/pointer-glide.sh", wx + x, wy + y])
        if click:
            run(["omarchy-drive", "click", wx + x, wy + y, "left"])
        time.sleep(SETTLE_SECONDS)

    def park(self):
        wx, wy = self.window()["at"]
        run(["bash", "-c", 'source "$1"; pointer_glide "$2" "$3"', "capture-pointer",
             REPO / "tests/lib/pointer-glide.sh", wx + 6, wy + PICKER_HEIGHT - 6])
        time.sleep(SETTLE_SECONDS)

    # Sample input: {"focus":{"kind":"DialogButton","name":"Cancel"},"controls":[{"name":"Cancel","hoverable":true,"hovered":true}]}
    def hover(self, name):
        self.pointer(name)
        deadline = time.monotonic() + FLAG_WAIT_SECONDS
        entries = []
        while time.monotonic() < deadline:
            entries = [entry for entry in self.ipc("cohesionControls")["controls"] if entry["name"] == name]
            if entries and (not any(entry["hoverable"] for entry in entries) or any(entry["hovered"] for entry in entries)):
                return True
            time.sleep(POLL_SECONDS)
        print(f"B72_MISS {theme} picker {name} {'is not reported' if not entries else 'never reported hovered'}", flush=True)
        missed.append(f"{name} hover")
        return False

    def tab_to(self, name):
        for _ in range(TAB_LIMIT):
            if self.control(name)["focused"]:
                return True
            self.press("-k", "Tab")
        print(f"B72_MISS {theme} picker Tab never reached {name}", flush=True)
        missed.append(f"Tab to {name}")
        return False

    def shot(self, item, surface, state):
        name = f"b72-{item}-{surface}-{state}-{theme}"
        png = evidence / f"{name}.png"
        if png.exists() or png.is_symlink():
            raise AssertionError(f"capture already exists: {png}")
        run(["omarchy-drive", "shot", png, self.title])
        (evidence / f"{name}.json").write_text(json.dumps(self.ipc("cohesionControls")))
        manifest.write(f"{name}\t{theme}\t{item}\t{surface}\t{state}\n")
        manifest.flush()
        print(f"B72_PICKER {name}", flush=True)

    def backends(self):
        # Sample input: "4321 4322", the children of one thread; a Process may start from any thread of the picker.
        children = []
        for task in Path(f"/proc/{self.pid}/task").iterdir():
            try:
                children += (task / "children").read_text().split()
            except FileNotFoundError:
                # The thread ended after the listing, so it has no children to hold.
                continue
        binary = Path(picker_env["FLEA_BIN"])
        return [int(pid) for pid in children if Path(f"/proc/{pid}/exe").resolve() == binary]

    def closed(self, code):
        wait("picker answer", lambda: self.response)
        if self.response[0] != code:
            raise AssertionError(f"picker answered {self.response[0]}, expected {code}")
        wait("picker closed", lambda: not any(window.get("pid") == self.pid for window in windows()))
        bus.signal_unsubscribe(self.subscription)


def open_states():
    filters = [("Text", [(0, "*.txt")]), ("Images", [(0, "*.png")])]
    ask = Request("b72open", "OpenFile", multiple=GLib.Variant("b", True),
                  filters=GLib.Variant("a(sa(us))", filters), current_filter=GLib.Variant("(sa(us))", filters[0]))
    wait("three text rows", lambda: ask.state()["total"] == 3)
    ask.press("-k", "space")
    wait("first row marked", lambda: not ask.state()["marksBusy"] and len(ask.state()["marks"]) == 1)
    ask.park()
    ask.shot("L1", "picker-list", "marked-cursor")
    ask.shot("L2", "picker-filter", "rest")
    ask.shot("L3", "picker-strip", "rest")
    ask.press("-k", "Down")
    ask.press("-k", "Down")
    ask.shot("L1", "picker-list", "marked-and-unmarked")
    ask.pointer("Grid view", click=True)
    wait("grid view", lambda: ask.state()["view"] == "grid")
    ask.park()
    ask.shot("L1", "picker-grid", "marked-and-unmarked")
    ask.pointer("List view", click=True)
    wait("list view", lambda: ask.state()["view"] == "list")
    ask.park()
    if ask.hover("Cancel"):
        ask.shot("L3", "picker-strip", "cancel-hover")
    ask.park()
    if ask.tab_to("Cancel"):
        ask.shot("L3", "picker-strip", "cancel-focus")
        # The accept answer follows Cancel in the walk, whatever the request names it.
        ask.press("-k", "Tab")
        accept = ask.ipc("cohesionControls")["focus"]
        if accept and accept["kind"] == "DialogButton" and accept["name"] != "Cancel":
            ask.shot("L3", "picker-strip", "accept-focus")
        else:
            print(f"B72_MISS {theme} picker the Tab after Cancel landed on {accept}", flush=True)
            missed.append("accept focus")
    # PickerFilter.qml replaces the old chips with one trigger and a menu containing the same choices.
    if not ask.tab_to("Filter"):
        raise AssertionError("picker never focused its filter trigger")
    ask.shot("L2", "picker-filter", "trigger-focus")
    ask.press("-k", "Return")
    wait("chosen filter focused", lambda: ask.control("Text")["focused"])
    ask.shot("L2", "picker-filter", "chosen-focus")
    ask.press("-k", "Down")
    wait("unchosen filter focused", lambda: ask.control("Images")["focused"])
    ask.shot("L2", "picker-filter", "unchosen-focus")
    if not ask.hover("All files"):
        raise AssertionError("picker never hovered its All files choice")
    ask.shot("L2", "picker-filter", "all-files-hover")
    if ask.state()["filter"] != 0:
        raise AssertionError("hover changed the chosen filter")
    ask.park()
    ask.press("-k", "Up")
    wait("Images focused again", lambda: ask.control("Images")["focused"])
    ask.press("-k", "Return")
    wait("Images chosen", lambda: ask.state()["filter"] == 1 and ask.state()["total"] == 1)
    ask.shot("L2", "picker-filter", "chosen-after-enter-focus")
    ask.press("-k", "Escape")
    ask.closed(1)


def save_states():
    ask = Request("b72save", "SaveFile", current_name=GLib.Variant("s", "alpha.txt"))
    wait("collision reviewed", lambda: ask.state()["saveReady"] and ask.state()["collision"])
    ask.park()
    ask.shot("L9", "picker-save", "collision-rest")
    ask.pointer("Save", click=True)
    wait("collision review shown", lambda: ask.state()["collisionReview"] and not ask.state()["submitting"])
    ask.park()
    ask.shot("L9", "picker-save", "collision-review")
    stopped = []
    try:
        held = ask.backends()
        if not held:
            raise AssertionError("no picker child runs the backend binary, so a commit cannot be held")
        for backend in held:
            os.kill(backend, signal.SIGSTOP)
            stopped.append(backend)
        ask.pointer("Replace", click=True)
        wait("submission held", lambda: ask.state()["submitting"])
        ask.park()
        ask.shot("L9", "picker-save", "submitting")
    finally:
        for backend in stopped:
            try:
                os.kill(backend, signal.SIGCONT)
            except ProcessLookupError:
                pass
    ask.closed(0)


def interrupted(signum, _frame):
    raise RuntimeError(f"b72 picker interrupted by signal {signum}")


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
    run([picker_env["FLEA_BIN"], "--ui-state", json.dumps({"keys": "default", "pickerView": "list",
         "display": {"textSize": {"mode": 14}}})], picker_env)
    open_states()
    save_states()
    if missed:
        raise AssertionError(f"b72 picker never reached {len(missed)} states: {missed}")
finally:
    manifest.close()
    # The private backend's process group owns its picker children, even after a failed IPC read.
    for process in reversed(processes):
        try:
            os.killpg(process.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
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
            print(f"B72_CLEANUP {theme} process {process.pid} outlived SIGKILL", flush=True)
    for log in logs:
        log.close()
    for path in root.glob("*.log"):
        (evidence / f"b72-picker-{theme}-{path.name}").write_bytes(path.read_bytes())
