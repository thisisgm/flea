#!/usr/bin/env python3
"""Candidate FileChooser proof through a private public portal and real native input."""
import hashlib
import fcntl
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time

import gi
gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

REPO = Path(__file__).resolve().parent.parent
BIN = Path(os.environ.get("FLEA_BIN", REPO / "target/release/flea")).resolve(strict=True)
UI = Path(os.environ.get("FLEA_UI", REPO / "ui")).resolve(strict=True)
FRONTEND = "org.freedesktop.portal.Desktop"
BACKEND = "org.freedesktop.impl.portal.desktop.flea"
OBJECT = "/org/freedesktop/portal/desktop"
DEADLINE = 20
checks = 0
processes = []
current = None


def run(args, env=None):
    return subprocess.run([str(arg) for arg in args], env=env, text=True, capture_output=True, check=True, timeout=30).stdout.strip()


def check(label, condition, observed=None):
    global checks
    if not condition:
        raise AssertionError(f"{label}: {observed!r}")
    checks += 1
    print(f"PICKER_CHECK {checks} {label} {json.dumps(observed)}", flush=True)


def wait(label, predicate):
    deadline = time.monotonic() + DEADLINE
    while time.monotonic() < deadline:
        while GLib.MainContext.default().pending():
            GLib.MainContext.default().iteration(False)
        value = predicate()
        if value:
            return value
        time.sleep(0.05)
    raise AssertionError(f"timed out: {label}")


def drive(*args):
    return run(["omarchy-drive", *args], drive_env)


def windows():
    # The normalized driver payload omits PID; Hyprland supplies the process identity used before every mutation.
    result = json.loads(run(["hyprctl", "clients", "-j"], drive_env))
    if not isinstance(result, list):
        raise AssertionError(f"invalid Hyprland window inventory: {result}")
    return result


def guard(path):
    path = Path(path)
    if not path.is_absolute() or not str(path) or not (root / ".flea-test-sandbox").is_file():
        raise AssertionError(f"invalid sandbox path: {path}")
    if not path.resolve().is_relative_to(root) or path.resolve() == root:
        raise AssertionError(f"outside owned sandbox: {path}")
    return path


def write(path, data):
    guard(path).write_text(data)


def move(source, target):
    guard(source).rename(guard(target))


def start(args, name, environment):
    log = guard(root / f"{name}.log").open("w")
    child = subprocess.Popen([str(arg) for arg in args], env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
    log.close()
    processes.append(child)
    return child


def owner(name):
    answer = bus.call_sync("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "NameHasOwner",
                          GLib.Variant("(s)", (name,)), None, Gio.DBusCallFlags.NONE, 5000, None)
    return answer.unpack()[0]


class Request:
    def __init__(self, name, method="OpenFile", folder=None, **options):
        global current
        self.name = name
        self.title = f"Flea picker {os.getpid()} {name}"
        self.result = None
        self.responses = 0
        self.pid = None
        guard(root / "run")
        token = f"case{len(list(root.glob('request-*.json')))}"
        sender = bus.get_unique_name()[1:].replace(".", "_")
        self.handle = f"{OBJECT}/request/{sender}/{token}"
        self.subscription = bus.signal_subscribe(FRONTEND, "org.freedesktop.portal.Request", "Response", self.handle,
                                                None, Gio.DBusSignalFlags.NONE, self.response)
        options = {"handle_token": GLib.Variant("s", token),
                   "current_folder": GLib.Variant("ay", os.fsencode(folder or fixture) + b"\0"), **options}
        write(root / f"request-{name}.json", json.dumps({"method": method, "options": {key: value.unpack() for key, value in options.items()}}))
        answer = bus.call_sync(FRONTEND, OBJECT, "org.freedesktop.portal.FileChooser", method,
                              GLib.Variant("(ssa{sv})", ("", self.title, options)), None, Gio.DBusCallFlags.NONE, 10000, None)
        check(f"{name}: public portal handle", answer.unpack()[0] == self.handle, answer.unpack()[0])
        current = self

    def response(self, connection, sender, path, interface, signal_name, params):
        self.responses += 1
        code, results = params.unpack()
        self.result = {"response": code, "results": results}
        write(root / f"response-{self.name}.json", json.dumps(self.result))

    def opened(self):
        def locate():
            found = [window for window in windows() if window.get("title") == self.title]
            if len(found) > 1:
                raise AssertionError("ambiguous owned picker window")
            return found[0] if found else None
        window = wait(f"{self.name} window", locate)
        self.pid = window["pid"]
        environ = dict(item.split(b"=", 1) for item in Path(f"/proc/{self.pid}/environ").read_bytes().split(b"\0") if b"=" in item)
        check(f"{self.name}: candidate binary", environ.get(b"FLEA_BIN", b"").decode() == str(BIN), environ.get(b"FLEA_BIN", b"").decode())
        check(f"{self.name}: candidate UI", environ.get(b"FLEA_UI", b"").decode() == str(UI), environ.get(b"FLEA_UI", b"").decode())
        self.reply_path = guard(environ[b"FLEA_PICKER_REPLY"].decode())
        command = Path(f"/proc/{self.pid}/cmdline").read_bytes().replace(b"\0", b" ").decode()
        check(f"{self.name}: native config", str(UI / "picker.qml") in command, command)
        drive("focus", self.title)
        self.until("ready listing", lambda state: state["state"] != "loading" and not state["marksBusy"] and (state["total"] == 0 or state["rows"]))
        children = Path(f"/proc/{self.pid}/task/{self.pid}/children").read_text().split()
        executables = [str(Path(f"/proc/{pid}/exe").resolve()) for pid in children]
        check(f"{self.name}: candidate backend running", str(BIN) in executables, executables)
        self.resize(800, 410)
        return self

    def resize(self, width, height):
        self.window()
        clients = json.loads(run(["hyprctl", "-j", "clients"], drive_env))
        found = [client for client in clients if client["pid"] == self.pid and client["title"] == self.title]
        if len(found) != 1 or not re.fullmatch(r"0x[0-9a-fA-F]+", found[0]["address"]):
            raise AssertionError("cannot identify owned picker for resize")
        if not found[0]["floating"]:
            drive("window", "float", self.title)
        answer = run(["hyprctl", "dispatch", f'hl.dsp.window.resize({{ x = {width}, y = {height}, exact = true, window = "address:{found[0]["address"]}" }})'], drive_env)
        check(f"{self.name}: compositor accepted resize", answer.strip() == "ok", answer)
        drive("window", "center", self.title)
        self.until(f"viewport {width}x{height}", lambda state: state["width"] == width and state["height"] == height)

    def state(self):
        return json.loads(run(["qs", "ipc", "--pid", self.pid, "call", "fleapicker", "snapshot"], picker_env))

    def window(self):
        found = [window for window in windows() if window.get("pid") == self.pid and window.get("title") == self.title]
        if len(found) != 1:
            raise AssertionError("owned picker window is missing or ambiguous")
        environment = Path(f"/proc/{self.pid}/environ").read_bytes().split(b"\0")
        if b"FLEA_PICKER_REPLY=" + os.fsencode(self.reply_path) not in environment:
            raise AssertionError("picker window no longer belongs to this request")
        return found[0]

    def until(self, label, predicate):
        try:
            result = wait(f"{self.name}: {label}", lambda: (state if predicate(state := self.state()) else None))
        except AssertionError:
            write(root / (self.name + "-failed-state.json"), json.dumps(self.state(), indent=2))
            raise
        check(f"{self.name}: {label}", True)
        return result

    def key(self, *keys):
        self.window()
        drive("key", "--window", self.title, *keys)

    def control(self, name):
        found = [item for item in self.state()["controls"] if item["name"] == name and item["visible"]]
        if not found:
            raise AssertionError(f"{name} is not a visible control")
        return found[-1]

    def focus_control(self, name):
        def focus(state):
            return [state["listFocus"], state["railFocus"], *[item["focused"] for item in state["controls"]]]
        state = self.state()
        for _ in range(len(state["controls"]) + 3):
            if self.control(name)["focused"]:
                check(f"{self.name}: native Tab focuses {name}", True)
                return state
            previous = focus(state)
            self.key("-k", "Tab")
            state = self.until("Tab advances focus", lambda state: focus(state) != previous)
        raise AssertionError(f"native Tab never reached {name}")

    def revealed(self, name):
        state = self.state()
        control = self.control(name)
        x, y, width, height = control["bounds"]
        left, top, viewport_width, viewport_height = state["geometry"]["saveViewport"]
        check(f"{self.name}: focused {name} is fully inside the scrolling form",
              control["focused"] and left <= x and top <= y and x + width <= left + viewport_width
              and y + height <= top + viewport_height,
              {"control": control, "viewport": state["geometry"]["saveViewport"], "scroll": state["geometry"]["saveScroll"]})
        check(f"{self.name}: scrolling form stays above footer",
              0 <= left and 0 <= top and left + viewport_width <= state["width"]
              and top + viewport_height <= state["height"] - state["geometry"]["footer"], state["geometry"])
        return state

    def point(self, centre, button="left"):
        x, y = map(int, centre.split())
        window = self.window()
        check(f"{self.name}: pointer stays inside owned window", 0 <= x < window["size"][0] and 0 <= y < window["size"][1], [x, y])
        drive("click", window["at"][0] + x, window["at"][1] + y, button)

    def click(self, name):
        self.point(self.control(name)["centre"])

    def row(self, name):
        self.key("-k", "Home")
        count = self.state()["total"]
        for _ in range(count + 1):
            state = self.state()
            if state["cursorName"] == name:
                return state["cursor"]
            self.key("-k", "Down")
            self.until("cursor row loaded", lambda state: bool(state["cursorName"]))
        raise AssertionError(f"no row {name}")

    def mark(self, name):
        self.row(name)
        self.key("-k", "space")
        self.until("selection checked", lambda state: not state["marksBusy"])

    def capture(self, label):
        self.window()
        output = guard(root / f"{self.name}-{label}.png")
        if output.exists():
            raise AssertionError(f"screenshot would reuse {output}")
        drive("shot", output, self.title)
        check(f"{self.name}: fresh native screenshot", output.is_file() and output.stat().st_size > 0, str(output))
        write(root / f"{self.name}-{label}.json", json.dumps(self.state()))

    def answered(self, code, uris=None):
        result = wait(f"{self.name} callback", lambda: self.result)
        check(f"{self.name}: response", result["response"] == code, result)
        if code:
            # The public frontend may add an empty URI array; portal.sh checks the backend's empty dictionary.
            check(f"{self.name}: cancelled/fault frontend returns no selection", result["results"] in ({}, {"uris": []}), result)
        else:
            check(f"{self.name}: retained URIs", result["results"].get("uris") == uris, result)
        wait(f"{self.name} native window closed", lambda: not any(window.get("title") == self.title for window in windows()))
        check(f"{self.name}: exactly one callback", self.responses == 1, self.responses)
        bus.signal_unsubscribe(self.subscription)
        return result

    def cancel(self):
        self.key("-k", "Escape")
        self.answered(1)


def test_single():
    single = Request("SP01-single").opened()
    check("SP01 zero checked Open disabled", not single.state()["canAccept"], single.state())
    single.row("alpha.txt")
    single.key("-k", "Return")
    check("SP01 zero checked Enter stays open", single.result is None and single.state()["marks"] == [])
    single.mark("alpha.txt")
    single.mark("beta.txt")
    single.until("single mark replaced", lambda state: [mark["path"] for mark in state["marks"]] == [str(fixture / "beta.txt")])
    single.row("folder")
    single.key("-k", "Return")
    single.until("Enter navigated", lambda state: state["path"] == str(fixture / "folder"))
    single.click("Back")
    single.until("Back retained mark and restored listing focus", lambda state: state["path"] == str(fixture)
                 and len(state["marks"]) == 1 and state["listFocus"])
    single.capture("checked")
    single.row("alpha.txt")
    single.key("-k", "Return")
    single.answered(0, [(fixture / "beta.txt").as_uri()])


def test_multiple():
    multiple = Request("SP02-multiple", multiple=GLib.Variant("b", True), accept_label=GLib.Variant("s", "Send")).opened()
    multiple.mark("alpha.txt")
    index = multiple.row("beta.txt")
    centre = run(["qs", "ipc", "--pid", multiple.pid, "call", "fleapicker", "rowCentre", index], picker_env)
    # Double-clicking the row marks it and must not submit it.
    x, y = map(int, centre.split())
    window = multiple.window()
    drive("click", window["at"][0] + x, window["at"][1] + y, "--double")
    multiple.until("double-click marks without sending", lambda state: len(state["marks"]) == 2 and not state["marksBusy"])
    check("SP02 double-click left request open", multiple.result is None)
    check("SP02 selected byte total", sum(mark["bytes"] for mark in multiple.state()["marks"]) == 7)
    multiple.capture("multiple")
    multiple.click("Send 2")
    multiple.answered(0, [(fixture / name).as_uri() for name in ["alpha.txt", "beta.txt"]])


def test_directory():
    directory = Request("SP03-directory", directory=GLib.Variant("b", True)).opened()
    directory.mark("folder")
    directory.key("-k", "Return")
    directory.until("marked directory navigated", lambda state: state["path"] == str(fixture / "folder"))
    directory.click("Parent folder")
    directory.until("Parent retained directory URI", lambda state: state["path"] == str(fixture) and len(state["marks"]) == 1)
    directory.capture("directory")
    directory.click("Choose folder")
    directory.answered(0, [(fixture / "folder").as_uri()])


def test_filters():
    filters = [("Images", [(0, "*.png")]), ("Text", [(1, "text/plain")])]
    filtered = Request("SP04-filters", folder=large, filters=GLib.Variant("a(sa(us))", filters), current_filter=GLib.Variant("(sa(us))", filters[0])).opened()
    filtered.until("glob reaches beyond original window", lambda state: state["total"] == 1 and state["cursorName"] == "zz-last.png")
    filtered.click("Text")
    filtered.until("MIME filter applies", lambda state: state["total"] == 150)
    filtered.click("All files")
    filtered.until("All files restores listing", lambda state: state["total"] == 151)
    check("SP04 filter chip owns focus", filtered.control("All files")["focused"])
    filtered.key("-k", "Tab")
    filtered.until("Tab leaves filters for rail", lambda state: state["railFocus"])
    filtered.key("-k", "Tab")
    filtered.until("Tab enters listing", lambda state: state["listFocus"])
    filtered.key("-k", "End")
    filtered.until("last window loads", lambda state: state["cursor"] == 150 and state["held"] > 0 and state["cursorName"] == "zz-last.png")
    filtered.key("-k", "space")
    filtered.until("last window checked", lambda state: not state["marksBusy"] and len(state["marks"]) == 1)
    filtered.click("Open")
    result = filtered.answered(0, [(large / "zz-last.png").as_uri()])
    check("SP04 selected filter returned", result["results"].get("current_filter") == ("All files", [(0, "*")]), result)


def test_changed():
    changed = Request("SP05-changed", multiple=GLib.Variant("b", True)).opened()
    changed.mark("alpha.txt")
    move(fixture / "alpha.txt", fixture / "alpha-retained")
    write(fixture / "alpha.txt", "replacement")
    changed.click("Open")
    state = changed.until("replacement removed with persistent error", lambda state: not state["marks"] and state["messageError"])
    check("SP05 one changed identity uses singular wording", state["message"] == "1 selected item moved or changed; select it again.", state["message"])
    check("SP05 replacement never returned", changed.result is None)
    changed.capture("changed-identity")
    changed.cancel()

    missing = Request("SP05-missing").opened()
    missing.mark("beta.txt")
    move(fixture / "beta.txt", fixture / "beta-retained")
    missing.click("Open")
    missing.until("missing selection removed", lambda state: not state["marks"] and state["messageError"])
    missing.cancel()
    move(fixture / "beta-retained", fixture / "beta.txt")

    guard(fixture / "linked-folder").symlink_to(fixture / "folder")
    linked = Request("SP05-directory-link", directory=GLib.Variant("b", True)).opened()
    linked.mark("linked-folder")
    linked.click("Choose folder")
    linked.answered(0, [(fixture / "linked-folder").as_uri()])

    navigation = Request("SP05-filtered-directory-link", filters=GLib.Variant("a(sa(us))", [("Images", [(0, "*.png")])])).opened()
    navigation.row("linked-folder")
    navigation.key("-k", "Return")
    navigation.until("filtered directory link navigates with link path", lambda state: state["path"] == str(fixture / "linked-folder"))
    navigation.cancel()


def test_save():
    directory = Request("SP06-save-directory", "SaveFile", current_name=GLib.Variant("s", "folder")).opened()
    directory.until("directory is not offered as a replaceable file", lambda state: not state["canAccept"] and not state["collision"] and "is a directory" in state["saveError"])
    directory.cancel()
    save = Request("SP06-save", "SaveFile", current_file=GLib.Variant("ay", os.fsencode(large / "zz-last.png") + b"\0")).opened()
    save.until("current_file collision outside initial window", lambda state: state["saveReady"] and state["collision"] and state["saveName"] == "zz-last.png")
    save.click("Filename")
    save.key("-M", "ctrl", "-k", "a", "-m", "ctrl", "../invalid")
    save.until("invalid save name refused", lambda state: not state["canAccept"] and state["saveName"] == "../invalid")
    save.key("-M", "ctrl", "-k", "a", "-m", "ctrl", "zz-last.png")
    save.until("collision review restored", lambda state: state["saveReady"] and state["collision"])
    save.key("-k", "Return")
    save.until("collision Enter focuses Cancel", lambda state: any(control["name"] == "Cancel" and control["focused"] for control in state["controls"]))
    save.capture("collision")
    save.click("Use this location")
    save.answered(0, [(large / "zz-last.png").as_uri()])
    check("SP07 picker did not overwrite", (large / "zz-last.png").read_text() == "image")
    write(large / "zz-last.png", "caller write")
    check("SP07 caller owns write after callback", (large / "zz-last.png").read_text() == "caller write")

    absent = Request("SP07-save-absent", "SaveFile", current_name=GLib.Variant("s", "new.txt")).opened()
    absent.until("absent destination reviewed", lambda state: state["saveReady"] and not state["collision"])
    absent.click("Save")
    absent.answered(0, [(fixture / "new.txt").as_uri()])
    check("SP07 SaveFile created nothing", not (fixture / "new.txt").exists())


def test_cancel():
    for name, action in [("escape", "escape"), ("pointer", "pointer"), ("close", "close")]:
        cancel = Request(f"SP08-{name}").opened()
        if action == "escape": cancel.key("-k", "Escape")
        elif action == "pointer": cancel.click("Cancel")
        else: drive("window", "close", cancel.title)
        cancel.answered(1)


def test_failure():
    parent = guard(root / "permission-parent")
    parent.mkdir()
    folder = guard(parent / "listing")
    folder.mkdir()
    target = guard(folder / "retained.txt")
    write(target, "retained picker contents")
    denied = Request("SP09-permission", folder=folder).opened()
    denied.row(target.name)
    guard(parent).chmod(0)
    try:
        denied.key("-k", "space")
        expected = f"Could not inspect {target}: permission denied"
        refused = denied.until("kernel permission refusal is plain and retains the picker", lambda state:
                               not state["marksBusy"] and state["messageError"] and state["message"] == expected)
        check("refused selection has no accepted URI", not refused["marks"] and not refused["canAccept"], refused["marks"])
        denied.capture("permission-denied")
    finally:
        guard(parent).chmod(0o700)
    denied.mark(target.name)
    denied.until("restored permissions allow the same item", lambda state: state["canAccept"] and len(state["marks"]) == 1)
    denied.click("Open")
    denied.answered(0, [target.as_uri()])
    check("permission recovery keeps the original file contents", target.read_text() == "retained picker contents")

    for mode in ["open", "save"]:
        lost = Request(f"SP09-backend-{mode}", "SaveFile" if mode == "save" else "OpenFile",
                       **({"current_name": GLib.Variant("s", "draft.txt")} if mode == "save" else {})).opened()
        if mode == "open": lost.mark("alpha.txt")
        else: lost.until("draft reviewed before backend loss", lambda state: state["saveReady"])
        before = lost.state()
        children = Path(f"/proc/{lost.pid}/task/{lost.pid}/children").read_text().split()
        backends = [int(pid) for pid in children if Path(f"/proc/{pid}/exe").resolve() == BIN]
        check("backend failure targets only owned candidate child", len(backends) == 1, backends)
        # Stop only this backend to make the UI's outstanding check observable before killing it.
        os.kill(backends[0], signal.SIGSTOP)
        try:
            lost.click("Save" if mode == "save" else "Open")
            lost.until("acceptance waits on the real stopped backend", lambda state: state["submitting"])
            lost.until("submission keeps enabled Cancel focused", lambda state: any(
                control["name"] == "Cancel" and control["focused"] and control["enabled"] for control in state["controls"]))
        finally:
            os.kill(backends[0], signal.SIGKILL)
        after = lost.until("lost backend clears checks and disables acceptance", lambda state: state["backendUnavailable"] and not state["submitting"] and not state["marksBusy"] and not state["saveBusy"] and not state["canAccept"] and state["messageError"])
        check("backend loss retains selected identities and draft", after["marks"] == before["marks"] and after["saveName"] == before["saveName"])
        check("backend loss advertises only cancellation", after["hints"] == "Esc cancel", after["hints"])
        check("backend loss keeps enabled Cancel focused", any(
            control["name"] == "Cancel" and control["focused"] and control["enabled"] for control in after["controls"]), after["controls"])
        lost.capture("unavailable")
        if mode == "open": lost.key("-k", "Escape")
        else: lost.click("Cancel")
        lost.answered(1)

    died = Request("SP09-died").opened()
    os.kill(died.pid, signal.SIGKILL)
    died.answered(2)

    failed_reply = Request("SP09-reply-write").opened()
    runtime = failed_reply.reply_path.parent
    move(runtime, root / "retained-reply-runtime")
    write(runtime, "reply parent is now a file")
    failed_reply.key("-k", "Escape")
    failed_reply.answered(2)
    guard(runtime).unlink()

    withdrawn = Request("SP09-withdrawn").opened()
    bus.call_sync(FRONTEND, withdrawn.handle, "org.freedesktop.portal.Request", "Close", None, None, Gio.DBusCallFlags.NONE, 5000, None)
    wait("withdrawn window removed", lambda: not any(window.get("pid") == withdrawn.pid for window in windows()))
    check("SP09 withdrawn request has no Response", withdrawn.result is None)
    bus.signal_unsubscribe(withdrawn.subscription)


def test_keys():
    for preset in ["default", "vim", "mac", "windows"]:
        write(state_file, json.dumps({"keys": preset}))
        keys = Request(f"SP10-{preset}").opened()
        keys.until("preset loaded", lambda state: state["preset"] == preset)
        keys.key("-k", "Tab")
        keys.until("Tab enters chrome", lambda state: any(control["focused"] for control in state["controls"]))
        keys.key("-M", "shift", "-k", "Tab", "-m", "shift")
        keys.until("Shift Tab restores list", lambda state: state["listFocus"])
        if preset == "vim":
            keys.key("j")
            keys.until("vim j moves cursor", lambda state: state["cursor"] == 1)
            keys.key("k")
            keys.until("vim k moves cursor", lambda state: state["cursor"] == 0)
        keys.capture("focus")
        keys.cancel()

    for size in [9, 10, 11, 12, 14, 16, 20]:
        write(state_file, json.dumps({"display": {"textSize": {"mode": size}}}))
        small = Request(f"SP10-size-{size}", "SaveFile", current_name=GLib.Variant("s", "beta.txt")).opened()
        small.until("collision review ready", lambda state: state["saveReady"] and state["collision"])
        small.resize(420, 280)
        state = small.state()
        check(f"SP10 size {size} listing remains inside viewport", state["geometry"]["list"] >= 0 and state["geometry"]["save"] >= 0, state["geometry"])
        small.capture("small")
        initial_scroll = state["geometry"]["saveScroll"]
        use_bounds = small.control("Use this location")["bounds"]
        viewport = state["geometry"]["saveViewport"]
        lower_clipped = use_bounds[1] + use_bounds[3] > viewport[1] + viewport[3]
        small.focus_control("Filename")
        small.revealed("Filename")
        small.focus_control("Output URI")
        small.revealed("Output URI")
        small.key("-k", "End")
        uri = small.until("End reveals the Output URI tail", lambda state: state["outputUri"]["offset"] == state["outputUri"]["maximum"])
        check(f"SP10 size {size} Output URI identity unchanged", uri["outputUri"]["text"] == (fixture / "beta.txt").as_uri(), uri["outputUri"])
        small.capture("uri-end")
        small.key("-k", "Home")
        small.until("Home restores the Output URI prefix", lambda state: state["outputUri"]["offset"] == 0)
        if uri["outputUri"]["maximum"] > 0:
            small.key("-k", "Right")
            small.until("Right pans the Output URI", lambda state: 0 < state["outputUri"]["offset"] <= state["outputUri"]["maximum"])
            small.key("-k", "Left")
            small.until("Left restores the Output URI prefix", lambda state: state["outputUri"]["offset"] == 0)
            small.click("Output URI")
            small.until("pointer focuses Output URI", lambda state: small.control("Output URI")["focused"])
            small.window()
            drive("scroll", "right", "1")
            small.until("horizontal wheel pans Output URI", lambda state: state["outputUri"]["offset"] > 0)
            small.key("-k", "Home")
            small.until("Home restores URI after wheel input", lambda state: state["outputUri"]["offset"] == 0)
        small.focus_control("Cancel")
        small.revealed("Cancel")
        small.focus_control("Use this location")
        revealed = small.revealed("Use this location")
        if lower_clipped:
            check(f"SP10 size {size} native focus scrolls clipped collision controls into view",
                  revealed["geometry"]["saveScroll"] > initial_scroll, [initial_scroll, revealed["geometry"]["saveScroll"]])
        small.capture("collision-focus")
        for target in ["Cancel", "Output URI", "Filename"]:
            small.key("-M", "shift", "-k", "Tab", "-m", "shift")
            small.until(f"Shift Tab restores {target}", lambda state: small.control(target)["focused"])
            small.revealed(target)
        small.cancel()


def main():
    groups = {"single": test_single, "multiple": test_multiple, "directory": test_directory,
              "filters": test_filters, "changed": test_changed, "save": test_save,
              "cancel": test_cancel, "failure": test_failure, "keys": test_keys}
    selected = sys.argv[1:] or list(groups)
    for name in selected:
        if name not in groups:
            raise AssertionError("unknown picker group: " + name)
        groups[name]()


root = Path(tempfile.mkdtemp(prefix="flea-picker-native-", dir="/tmp")).resolve()
(root / ".flea-test-sandbox").write_text("owned FileChooser native fixture\n")
print(f"PICKER_EVIDENCE {root}", flush=True)
try:
    drive_env = dict(os.environ)
    drive_env["PATH"] = str(Path.home() / ".local/bin") + os.pathsep + drive_env["PATH"]
    values = run(["omarchy-drive", "env"], drive_env).splitlines()
    expected = {"XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "YDOTOOL_SOCKET", "OMARCHY_PATH", "QT_LINUX_ACCESSIBILITY_ALWAYS_ON"}
    seen = set()
    for value in values:
        if not value.startswith("export ") or "=" not in value:
            raise AssertionError(f"invalid session environment: {value}")
        key, value = value[7:].split("=", 1)
        if key not in expected or key in seen or not value or any(char.isspace() or char in "'\"\\`$;" for char in value):
            raise AssertionError(f"unexpected session variable: {key}")
        drive_env[key] = value
        seen.add(key)
    check("all native session variables present", seen == expected)
    check("strict native runtime", drive_env["XDG_RUNTIME_DIR"] == "/run/user/1000")
    check("strict Wayland display", re.fullmatch(r"wayland-[0-9]+", drive_env["WAYLAND_DISPLAY"]) is not None)
    check("strict compositor identity", re.fullmatch(r"[A-Za-z0-9_.-]+", drive_env["HYPRLAND_INSTANCE_SIGNATURE"]) is not None)
    check("strict input socket", drive_env["YDOTOOL_SOCKET"] == drive_env["XDG_RUNTIME_DIR"] + "/.ydotool_socket")
    check("strict OEM path", drive_env["OMARCHY_PATH"] == "/usr/share/omarchy")
    check("native accessibility enabled", drive_env["QT_LINUX_ACCESSIBILITY_ALWAYS_ON"] == "1")
    display_lock = open(Path(drive_env["XDG_RUNTIME_DIR"]) / "flea-display.lock", "a")
    fcntl.flock(display_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    if not drive_env.get("QT_QPA_PLATFORMTHEME"):
        session = run(["systemctl", "--user", "show-environment"], drive_env)
        for line in session.splitlines():
            if line.startswith("QT_QPA_PLATFORMTHEME="):
                drive_env["QT_QPA_PLATFORMTHEME"] = line.split("=", 1)[1]
    check("no foreign Flea windows", not any("flea" in str(window.get("class", "")).lower() for window in windows()))
    head = run(["git", "-C", REPO, "rev-parse", "HEAD"])
    check("expected candidate HEAD", os.environ.get("FLEA_EXPECTED_SHA") == head, head)
    dirty = run(["git", "-C", REPO, "status", "--porcelain"])
    check("candidate source clean", not dirty, dirty)
    check("candidate UI belongs to source tree", UI == REPO / "ui", str(UI))
    inputs = [*REPO.glob("src/**/*.rs"), REPO / "Cargo.toml", REPO / "Cargo.lock", REPO / "keys.toml"]
    if (REPO / "build.rs").is_file(): inputs.append(REPO / "build.rs")
    newest = max(path.stat().st_mtime_ns for path in inputs)
    check("candidate binary newer than build inputs", BIN.stat().st_mtime_ns >= newest)
    manifest = {"head": head, "dirty": dirty,
                "binary": str(BIN), "binarySha256": hashlib.sha256(BIN.read_bytes()).hexdigest(), "ui": str(UI), "session": {key: drive_env[key] for key in expected}}
    write(root / "candidate.json", json.dumps(manifest, indent=2))
    picker_env = dict(drive_env)
    for key, name in [("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_STATE_HOME", "state"), ("XDG_CACHE_HOME", "cache"), ("XDG_RUNTIME_DIR", "run")]:
        guard(root / name).mkdir(mode=0o700)
        picker_env[key] = str(root / name)
    picker_env.update(FLEA_BIN=str(BIN), FLEA_UI=str(UI), WAYLAND_DISPLAY=str(Path(drive_env["XDG_RUNTIME_DIR"]) / drive_env["WAYLAND_DISPLAY"]))
    fixture, large = root / "files", root / "large"
    for directory in [fixture, fixture / "folder", large, root / "portals", root / "state/flea"]:
        guard(directory).mkdir(parents=True, exist_ok=True)
    write(fixture / "alpha.txt", "one")
    write(fixture / "beta.txt", "four")
    for index in range(150): write(large / f"file-{index:03}.txt", "text")
    write(large / "zz-last.png", "image")
    state_file = root / "state/flea/ui.json"
    write(root / "portals/flea.portal", "[portal]\nDBusName=org.freedesktop.impl.portal.desktop.flea\nInterfaces=org.freedesktop.impl.portal.FileChooser;\n")
    write(root / "portals/portals.conf", "[preferred]\norg.freedesktop.impl.portal.FileChooser=flea\n")
    picker_env["XDG_DESKTOP_PORTAL_DIR"] = str(root / "portals")
    daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], env=picker_env, stdout=subprocess.PIPE, stderr=guard(root / "bus.log").open("w"), text=True, start_new_session=True)
    processes.append(daemon)
    address = daemon.stdout.readline().strip()
    check("private bus address", address.startswith("unix:"), address)
    picker_env["DBUS_SESSION_BUS_ADDRESS"] = address
    bus = Gio.DBusConnection.new_for_address_sync(address, Gio.DBusConnectionFlags.AUTHENTICATION_CLIENT | Gio.DBusConnectionFlags.MESSAGE_BUS_CONNECTION, None, None)
    start([sys.executable, REPO / "tools/flea-portal"], "backend", picker_env)
    wait("candidate backend owns name", lambda: owner(BACKEND))
    start(["/usr/lib/xdg-desktop-portal", "--verbose"], "frontend", picker_env)
    wait("public frontend owns name", lambda: owner(FRONTEND))
    version = bus.call_sync(FRONTEND, OBJECT, "org.freedesktop.DBus.Properties", "Get",
                           GLib.Variant("(ss)", ("org.freedesktop.portal.FileChooser", "version")),
                           GLib.VariantType.new("(v)"), Gio.DBusCallFlags.NONE, 5000, None).unpack()[0]
    check("public FileChooser interface exported", isinstance(version, int) and version > 0, version)
    main()
    print(f"picker-native: {checks} checks, 0 failed; native screenshots require inspection: {root}", flush=True)
finally:
    if current is not None and current.pid is not None and Path(f"/proc/{current.pid}").exists():
        environment = Path(f"/proc/{current.pid}/environ").read_bytes().split(b"\0")
        if any(row.startswith(b"FLEA_PICKER_REPLY=" + os.fsencode(root) + b"/") for row in environment):
            os.kill(current.pid, signal.SIGTERM)
    for process in reversed(processes):
        if process.poll() is None:
            os.killpg(process.pid, signal.SIGTERM)
            try: process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    # Evidence and its marker deliberately remain for controller inspection and candidate attribution.
