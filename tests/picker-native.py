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


sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "tools"))
from flea_ipc import Client
IPC = Client("qs")
IPC.export()
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
# A refused sort draws nothing to wait for, and a key that lands later still changes the order the next Back checks.
NO_EVENT_WAIT_S = 0.5
# Hyprland reports a window's goal geometry while it still animates there, so a capture shoots until a shot repeats.
SETTLE_SHOT_INTERVAL_S = 0.1
SETTLE_MAX_SHOTS = 10
# A moving window never repeats a shot and a caret only alternates two, so a still window matches by its third shot whatever a shot costs.
SETTLE_MEMORY_SHOTS = 2
checks = 0
processes = []
current = None


# The files the binary was built from, as cargo recorded them, so a #[cfg(test)] module a release build never reads cannot make it look stale.
# Sample input, target/release/flea.d: "/repo/target/release/flea: /repo/keys.toml /repo/src/main.rs /repo/src/my\ dir/x.rs"
def build_inputs(binary, repo=REPO):
    glob = [*repo.glob("src/**/*.rs"), repo / "keys.toml"]
    dep_info = binary.with_name(binary.name + ".d")
    if not dep_info.is_file():
        return glob, "source glob, no dep-info"
    _, sep, listed = dep_info.read_text().partition(": ")
    names, word, escaped = [], "", False
    for char in listed.split("\n", 1)[0].strip():
        if escaped:
            word, escaped = word + char, False
        elif char == "\\":
            escaped = True
        elif char == " ":
            names, word = (names + [word]) if word else names, ""
        else:
            word += char
    if word:
        names.append(word)
    paths = [Path(name) for name in names]
    if not sep or not any(path.suffix == ".rs" for path in paths):
        return glob, "source glob, dep-info cut short"
    foreign = [path for path in paths if not in_tree(path, repo)]
    if foreign:
        return foreign, "another tree"
    return paths, "dep-info"

# Under repo and in no checkout nested inside it, since a worktree under .superpowers/ shares the prefix but is another tree.
def in_tree(path, repo):
    if not path.is_relative_to(repo):
        return False
    return not any((parent / ".git").exists() for parent in path.parents if parent != repo and parent.is_relative_to(repo))

# The newest input and the listed inputs that are gone, which make any binary stale since the build that listed them.
def newest_input(inputs):
    missing = [path for path in inputs if not path.exists()]
    present = [path for path in inputs if path.exists()]
    newest = max(present, key=lambda path: path.stat().st_mtime_ns, default=None)
    return newest, missing

# Whether the binary is at least as new as every input, and the detail a failure names.
def fresh(binary, inputs, how):
    newest, missing = newest_input(inputs)
    detail = {"inputs": how, "newest": str(newest), "missing": [str(path) for path in missing]}
    return not missing and newest is not None and binary.stat().st_mtime_ns >= newest.stat().st_mtime_ns, detail

# The freshness gate's own adversarial cases on a scratch tree: escaped space, test-only file, file gone, cut short, no source, another tree.
def check_build_inputs():
    with tempfile.TemporaryDirectory() as scratch:
        repo = Path(scratch) / "repo"
        (repo / "src" / "my dir").mkdir(parents=True)
        (repo / "target" / "release").mkdir(parents=True)
        main, spaced, test_only = repo / "src" / "main.rs", repo / "src" / "my dir" / "x.rs", repo / "src" / "only_tests.rs"
        for path in (main, spaced, repo / "keys.toml"):
            path.write_text("")
        binary = repo / "target" / "release" / "flea"
        binary.write_text("")
        dep_info = binary.with_name("flea.d")
        dep_info.write_text(f"{binary}: {repo}/keys.toml {main} {repo}/src/my\\ dir/x.rs\n")
        check("dep-info parses an escaped space", build_inputs(binary, repo) == ([repo / "keys.toml", main, spaced], "dep-info"))
        ns_per_second, base_stamp = 10**9, 10**18
        for age, path in enumerate((repo / "keys.toml", spaced, main)):
            os.utime(path, ns=(base_stamp + age * ns_per_second,) * 2)
        os.utime(binary, ns=(base_stamp + 2 * ns_per_second,) * 2)
        check("the newest listed input is the one judged", newest_input([repo / "keys.toml", main, spaced])[0] == main)
        check("a binary exactly as new as its newest input is fresh", fresh(binary, build_inputs(binary, repo)[0], "dep-info")[0])
        os.utime(spaced, ns=(base_stamp + 3 * ns_per_second,) * 2)
        check("an input newer than the binary makes it stale", not fresh(binary, build_inputs(binary, repo)[0], "dep-info")[0])
        test_only.write_text("")
        check("a test-only file cargo never listed does not count", test_only not in build_inputs(binary, repo)[0])
        spaced.unlink()
        gone_fresh, gone_detail = fresh(binary, build_inputs(binary, repo)[0], "dep-info")
        check("a listed input that is gone counts as stale", not gone_fresh and gone_detail["missing"] == [str(spaced)])
        glob = sorted([*repo.glob("src/**/*.rs"), repo / "keys.toml"])
        for label, text in (("cut short", f"{binary}"), ("with no Rust source", f"{binary}: {repo}/keys.toml\n")):
            dep_info.write_text(text)
            listed, how = build_inputs(binary, repo)
            check(f"dep-info {label} falls back to the whole source glob", (sorted(listed), how) == (glob, "source glob, dep-info cut short"))
        dep_info.unlink()
        listed, how = build_inputs(binary, repo)
        check("no dep-info falls back to the whole source glob", (sorted(listed), how) == (glob, "source glob, no dep-info"))
        dep_info.write_text(f"{binary}: /elsewhere/src/main.rs\n")
        check("dep-info naming another tree is a foreign binary", build_inputs(binary, repo) == ([Path("/elsewhere/src/main.rs")], "another tree"))
        nested = repo / "wt" / "cand"
        (nested / "src").mkdir(parents=True)
        (nested / ".git").write_text("gitdir: elsewhere\n")
        dep_info.write_text(f"{binary}: {nested}/src/main.rs\n")
        check("dep-info from a checkout nested in the tree is a foreign binary", build_inputs(binary, repo)[1] == "another tree")

def run(args, env=None):
    return subprocess.run([str(arg) for arg in args], env=env, text=True, capture_output=True, check=True, timeout=30, close_fds=True).stdout.strip()


def check(label, condition, observed=None):
    global checks
    if not condition:
        raise AssertionError(f"{label}: {observed!r}")
    checks += 1
    print(f"PICKER_CHECK {checks} {label} {json.dumps(observed)}", flush=True)


def kill_if_alive(pid):
    # Cleanup only, so a child that already died cannot mask the assertion that brought us here.
    try:
        os.kill(pid, signal.SIGKILL)
    except ProcessLookupError:
        pass


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


def settled_picture(name, shoot, sleep=time.sleep):
    recent = []
    for _ in range(SETTLE_MAX_SHOTS):
        picture = shoot()
        if picture in recent:
            return picture
        recent = (recent + [picture])[-SETTLE_MEMORY_SHOTS:]
        sleep(SETTLE_SHOT_INTERVAL_S)
    raise AssertionError(f"{name}: the picture kept changing across {SETTLE_MAX_SHOTS} shots")


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
    child = subprocess.Popen([str(arg) for arg in args], env=environment, stdout=log, stderr=subprocess.STDOUT, start_new_session=True, close_fds=True)
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
        check(f"{self.name}: native config", str(UI / "boot" / "picker.qml") in command, command)
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
        answer = run(["bash", REPO / "tests/lib/hypr-dispatch.sh", "window_resize", found[0]["address"], width, height], drive_env)
        check(f"{self.name}: compositor accepted resize", answer.strip() == "ok", answer)
        drive("window", "center", self.title)
        self.until(f"viewport {width}x{height}", lambda state: state["width"] == width and state["height"] == height)

    def state(self):
        # One reader answers path, field flag and Jump together, so a poll reads one instant.
        return json.loads(run([*IPC.command(), "--pid", self.pid, "call", "fleapicker", "snapshot"], picker_env))

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
        # An empty folder draws its hero beats after the listing is ready, so the shot waits for the rest first.
        if self.state()["emptyHero"]:
            self.until("empty hero is at rest", lambda state: state["emptyRest"])
        output = guard(root / f"{self.name}-{label}.png")
        if output.exists():
            raise AssertionError(f"screenshot would reuse {output}")
        scratch = guard(root / f"{self.name}-{label}.settling.png")
        def shoot():
            scratch.unlink(missing_ok=True)
            drive("shot", scratch, self.title)
            return scratch.read_bytes()
        settled_picture(f"{self.name}-{label}", shoot)
        move(scratch, output)
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
    single.row("folder")
    state = single.state()
    check("SP01 folder cursor disables Open", not state["canAccept"] and not single.control("Open")["enabled"], state)
    check("SP01 one-file hints advertise cursor choice", state["hints"] == "Enter open · Esc cancel", state["hints"])
    single.key("-k", "Return")
    single.until("Enter navigated", lambda state: state["path"] == str(fixture / "folder") and state["state"] != "loading")
    single.click("Back")
    single.until("Back restored listing focus without marks", lambda state: state["path"] == str(fixture)
                 and state["state"] == "ready" and state["marks"] == [] and state["listFocus"])
    single.row("alpha.txt")
    state = single.state()
    check("SP01 file cursor enables Open without marks", state["canAccept"] and state["marks"] == []
          and single.control("Open")["enabled"], state)
    single.key("-k", "space")
    time.sleep(NO_EVENT_WAIT_S)
    state = single.state()
    check("SP01 Space leaves cursor choice unmarked", state["marks"] == [] and not state["marksBusy"]
          and state["cursorName"] == "alpha.txt" and state["canAccept"] and single.result is None, state)
    single.capture("cursor-choice")
    single.key("-k", "Return")
    single.answered(0, [(fixture / "alpha.txt").as_uri()])


def test_multiple():
    multiple = Request("SP02-multiple", multiple=GLib.Variant("b", True), accept_label=GLib.Variant("s", "Send")).opened()
    multiple.mark("alpha.txt")
    index = multiple.row("beta.txt")
    centre = run([*IPC.command(), "--pid", multiple.pid, "call", "fleapicker", "rowCentre", index], picker_env)
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
    directory = Request("SP03-directory", directory=GLib.Variant("b", True), multiple=GLib.Variant("b", True)).opened()
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
    # The filter is a dropdown in the answer row: Return opens its menu on the current filter, Down moves, Return chooses.
    filtered.focus_control("Filter")
    for step, total in (("MIME filter applies", 150), ("All files restores listing", 151)):
        filtered.key("-k", "Return")
        filtered.key("-k", "Down")
        filtered.key("-k", "Return")
        filtered.until(step, lambda state, total=total: state["total"] == total)
    check("SP04 filter trigger owns focus", filtered.control("Filter")["focused"])
    # Tab runs top to bottom, left to right: chrome, rail, listing, then the answer row's Filter, Cancel, Open, and round to the chrome again.
    live = {item["name"]: item["enabled"] for item in filtered.state()["controls"]}
    for name in [name for name in ("Cancel", "Open", "Back", "Parent folder") if live[name]]:
        filtered.key("-k", "Tab")
        filtered.until(f"Tab reaches {name}", lambda state, name=name: any(item["name"] == name and item["focused"] for item in state["controls"]))
    filtered.key("-k", "Tab")
    filtered.until("Tab reaches the rail", lambda state: state["railFocus"])
    filtered.key("-k", "Tab")
    filtered.until("Tab enters listing", lambda state: state["listFocus"])
    filtered.key("-k", "End")
    filtered.until("last window loads", lambda state: state["cursor"] == 150 and state["held"] > 0 and state["cursorName"] == "zz-last.png")
    filtered.until("last window cursor choice enables Open without marks", lambda state: state["canAccept"] and state["marks"] == [])
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

    missing = Request("SP05-missing", multiple=GLib.Variant("b", True)).opened()
    missing.mark("beta.txt")
    move(fixture / "beta.txt", fixture / "beta-retained")
    missing.click("Open")
    missing.until("missing selection removed", lambda state: not state["marks"] and state["messageError"])
    missing.cancel()
    move(fixture / "beta-retained", fixture / "beta.txt")

    guard(fixture / "linked-folder").symlink_to(fixture / "folder")
    linked = Request("SP05-directory-link", directory=GLib.Variant("b", True), multiple=GLib.Variant("b", True)).opened()
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
    directory.until("directory is not offered as a replaceable file", lambda state: not state["canAccept"] and not state["collision"] and "A folder has this name; choose another name." in state["saveError"])
    directory.cancel()
    save = Request("SP06-save", "SaveFile", current_file=GLib.Variant("ay", os.fsencode(large / "zz-last.png") + b"\0")).opened()
    save.until("current_file collision outside initial window", lambda state: state["saveReady"] and state["collision"] and state["saveName"] == "zz-last.png")
    save.click("Filename")
    save.key("-M", "ctrl", "-k", "a", "-m", "ctrl", "../invalid")
    save.until("invalid save name refused", lambda state: not state["canAccept"] and state["saveName"] == "../invalid")
    save.key("-M", "ctrl", "-k", "a", "-m", "ctrl", "zz-last.png")
    save.until("collision review restored", lambda state: state["saveReady"] and state["collision"])
    save.key("-k", "Return")
    save.until("collision Enter returns to Filename and the answer reads Replace", lambda state: state["collisionReview"]
                and any(control["name"] == "Filename" and control["focused"] for control in state["controls"])
                and any(control["name"] == "Replace" for control in state["controls"]))
    save.capture("collision")
    save.click("Replace")
    save.answered(0, [(large / "zz-last.png").as_uri()])
    check("SP07 picker did not overwrite", (large / "zz-last.png").read_text() == "image")
    write(large / "zz-last.png", "caller write")
    check("SP07 caller owns write after callback", (large / "zz-last.png").read_text() == "caller write")

    absent = Request("SP07-save-absent", "SaveFile", current_name=GLib.Variant("s", "new.txt")).opened()
    absent.until("absent destination reviewed", lambda state: state["saveReady"] and not state["collision"])
    absent.click("Save")
    absent.answered(0, [(fixture / "new.txt").as_uri()])
    check("SP07 SaveFile created nothing", not (fixture / "new.txt").exists())


def test_savefocus():
    saving = Request("SP07-save-focus", "SaveFile", current_name=GLib.Variant("s", "report.pdf")).opened()
    before = saving.until("fresh save ready with a folder first", lambda state: state["saveReady"]
                          and state["rows"] and state["rows"][0]["n"] == "folder")
    check("fresh save focuses Filename and leaves the listing unfocused",
          any(item["name"] == "Filename" and item["focused"] for item in before["controls"])
          and not before["listFocus"] and not before["gridFocus"], before)
    saving.capture("fresh")
    saving.key("-k", "Return")
    saving.answered(0, [(fixture / "report.pdf").as_uri()])
    check("fresh Enter records the caller reply without creating a file",
          (root / "response-SP07-save-focus.json").is_file() and not (fixture / "report.pdf").exists())

    moved = Request("SP07-save-focus-up", "SaveFile", folder=fixture / "folder",
                    current_name=GLib.Variant("s", "report.pdf")).opened()
    moved.until("initial save destination ready", lambda state: state["saveReady"])
    moved.click("Parent folder")
    moved.until("pointer Up returns Filename focus in the shown folder", lambda state: state["path"] == str(fixture)
                and state["saveReady"] and any(item["name"] == "Filename" and item["focused"] for item in state["controls"]))
    moved.key("-k", "Return")
    moved.answered(0, [(fixture / "report.pdf").as_uri()])


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
        denied.key("-k", "Return")
        expected = f"Could not inspect {target}: permission denied."
        refused = denied.until("kernel permission refusal is plain and retains the picker", lambda state:
                               not state["marksBusy"] and state["messageError"] and state["message"] == expected)
        check("refused cursor choice has no accepted URI", not refused["marks"] and denied.result is None, refused["marks"])
        denied.capture("permission-denied")
    finally:
        guard(parent).chmod(0o700)
    denied.row(target.name)
    denied.until("restored cursor choice enables Open without marks", lambda state: state["canAccept"] and state["marks"] == [])
    denied.key("-k", "Return")
    denied.answered(0, [target.as_uri()])
    check("permission recovery keeps the original file contents", target.read_text() == "retained picker contents")

    for mode in ["open", "save"]:
        lost = Request(f"SP09-backend-{mode}", "SaveFile" if mode == "save" else "OpenFile",
                       **({"current_name": GLib.Variant("s", "draft.txt")} if mode == "save" else {"multiple": GLib.Variant("b", True)})).opened()
        if mode == "open": lost.mark("alpha.txt")
        else: lost.until("draft reviewed before backend loss", lambda state: state["saveReady"])
        before = lost.state()
        children = Path(f"/proc/{lost.pid}/task/{lost.pid}/children").read_text().split()
        backends = [int(pid) for pid in children if Path(f"/proc/{pid}/exe").resolve() == BIN]
        # The chooser owns exactly two candidate children: the identity checks and the listing worker.
        check("backend failure targets both owned candidate children", len(backends) == 2, backends)
        stopped = []
        try:
            # This case navigates nowhere after the read above, so the chooser replaces no worker and a missing child is a defect.
            for backend in backends:
                os.kill(backend, signal.SIGSTOP)
                stopped.append(backend)
            lost.click("Save" if mode == "save" else "Open")
            lost.until("acceptance waits on the real stopped backend", lambda state: state["submitting"])
            lost.until("submission keeps enabled Cancel focused", lambda state: any(
                control["name"] == "Cancel" and control["focused"] and control["enabled"] for control in state["controls"]))
        finally:
            # A stopped process cannot exit, so its PID was never reaped and is still the child this run signalled.
            for backend in stopped:
                kill_if_alive(backend)
        after = lost.until("lost backend clears checks and disables acceptance", lambda state: state["backendUnavailable"] and not state["submitting"] and not state["marksBusy"] and not state["saveBusy"] and not state["canAccept"] and state["messageError"])
        check("backend loss retains selected identities and draft", after["marks"] == before["marks"] and after["saveName"] == before["saveName"])
        check("backend loss advertises only cancellation", after["hints"] == "Esc cancel", after["hints"])
        check("backend loss keeps enabled Cancel focused", any(
            control["name"] == "Cancel" and control["focused"] and control["enabled"] for control in after["controls"]), after["controls"])
        held = lost.until("a lost listing worker keeps the rows and draws no empty hero", lambda state: state["listingFailed"] and state["total"] > 0 and not state["emptyHero"])
        check("backend loss draws no empty hero over retained rows", held["rows"] and not held["emptyHero"], held["emptyHero"])
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
        small.until("save ready", lambda state: state["saveReady"] and state["collision"])
        small.resize(420, 280)
        state = small.state()
        check(f"SP10 size {size} listing remains inside viewport", state["geometry"]["list"] >= 0 and state["geometry"]["save"] >= 0, state["geometry"])
        small.capture("small")
        small.focus_control("Filename")
        small.revealed("Filename")
        small.key("-k", "Return")
        small.until("collision review ready", lambda state: state["collisionReview"] and any(control["name"] == "Replace" for control in state["controls"]))
        small.revealed("Filename")
        small.capture("collision-focus")
        for target in ["Cancel", "Replace"]:
            state = small.focus_control(target)
            x, y, width, height = small.control(target)["bounds"]
            check(f"SP10 size {size} focused {target} is fully inside the window above the footer",
                  0 <= x and 0 <= y and x + width <= state["width"]
                  and y + height <= state["height"] - state["geometry"]["footer"], state["geometry"])
        for target in ["Cancel", "Filename"]:
            small.key("-M", "shift", "-k", "Tab", "-m", "shift")
            small.until(f"Shift Tab restores {target}", lambda state: small.control(target)["focused"])
        small.revealed("Filename")
        small.cancel()


def names(state):
    return [row["n"] for row in state["rows"]]


# None while the window has not written ui.json yet, or while the write is half done.
def ui_json():
    try:
        return json.loads(state_file.read_text())
    except (FileNotFoundError, json.JSONDecodeError):
        return None


# SCHEMA's top-level keys as src/uischema.rs spells them; a parse that finds no entries or an unknown constant raises.
# Sample input: 'pub const SCHEMA: &[(&str, Rule)] = &[', then '    ("view", Rule::Word(&["list", "columns", "grid", "dual"])),' and '    (STATE_VERSION, Rule::Version),', with 'pub const STATE_VERSION: &str = "stateVersion";'
def schema_top_keys(source=REPO / "src/uischema.rs"):
    text = source.read_text()
    block = re.search(r"pub const SCHEMA: &\[\(&str, Rule\)\] = &\[\n(.*?)\n\];", text, re.S)
    if block is None:
        raise AssertionError(f"SCHEMA block not found in {source}")
    body = block.group(1)
    constants = dict(re.findall(r'pub const (\w+): &str = "([^"]*)";', text))
    entries = re.findall(r'^    \((?:"([^"]+)"|([A-Z_]+)),', body, re.M)
    if not entries or len(entries) != len(re.findall(r"^    \(", body, re.M)):
        raise AssertionError(f"SCHEMA entry not parsed in {source}")
    return {literal or constants[name] for literal, name in entries}


def test_sorting():
    schema_keys = schema_top_keys()
    # The fixture's name, size, modified and kind orders all differ, so a marked order over other rows is caught by names.
    ordered = guard(root / "ordered")
    for directory in [ordered, ordered / "folder", ordered / "refused"]:
        guard(directory).mkdir()
    for name, size, year in [("alpha.txt", 200, 2022), ("bravo.txt", 3000, 2020), ("charlie.txt", 1, 2021),
                             ("photo.png", 1500, 2019), (".dot.txt", 50, 2024)]:
        write(ordered / name, "x" * size)
        stamp = time.mktime((year, 1, 1, 12, 0, 0, 0, 0, -1))
        os.utime(guard(ordered / name), (stamp, stamp))
    by_name = ["folder", "refused", "alpha.txt", "bravo.txt", "charlie.txt", "photo.png"]
    by_size = ["folder", "refused", "charlie.txt", "alpha.txt", "photo.png", "bravo.txt"]
    by_modified = ["folder", "refused", "photo.png", "bravo.txt", "charlie.txt", "alpha.txt"]
    by_modified_shown = by_modified + [".dot.txt"]
    by_kind = ["folder", "refused", "photo.png", "alpha.txt", "bravo.txt", "charlie.txt"]
    # updates.autoCheck is seeded off, a nested value the picker never writes, so the first write must keep it.
    saved = json.dumps({"sort": {"key": "kind", "reverse": False}, "updates": {"autoCheck": False}})
    write(state_file, saved)
    # Recent's newest-first order matches none of the fixture's sort orders, so a Recent that sorts is caught.
    recent_order = ["bravo.txt", "alpha.txt", "photo.png", "charlie.txt"]
    # Written in yet another order, so a Recent that keeps the file's order instead of the visit times is caught too.
    file_order = ["photo.png", "charlie.txt", "bravo.txt", "alpha.txt"]
    bookmarks = "".join(f'<bookmark href="{(ordered / name).as_uri()}" visited="2026-09-0{9 - recent_order.index(name)}T12:00:00Z"/>'
                        for name in file_order)
    write(root / "data/recently-used.xbel", f'<?xml version="1.0" encoding="UTF-8"?><xbel version="1.0">{bookmarks}</xbel>')

    sorting = Request("SP11-sorting", folder=ordered, multiple=GLib.Variant("b", True)).opened()
    inherited = sorting.until("the window's saved kind order is inherited", lambda state: state["sortBy"] == "kind" and state["state"] == "ready" and names(state) == by_kind)
    kind_order = names(inherited)
    sorting.key("S")
    # Descending reverses kind and its name tie-break together inside the folders-first group (src/backend/ordering.rs).
    reverse_kind = ["refused", "folder", "charlie.txt", "bravo.txt", "alpha.txt", "photo.png"]
    sorting.until("S reverses an inherited kind order", lambda state: state["sortBy"] == "kind" and state["sortDesc"]
                  and state["state"] == "ready" and names(state) == reverse_kind)
    # The first sort fills every shipped default into ui.json (src/uistate.rs patched), so the expectation starts from that whole file.
    def first_sort_written():
        saved_file = ui_json()
        if saved_file is not None and saved_file.get("sort") == {"key": "kind", "reverse": True} and "folderSorts" in saved_file:
            return saved_file
        return None
    first_write = wait("SP11 the first sort lands in ui.json", first_sort_written)
    # Wrong default values are pinned by src/uistate.rs unit tests on patched, so this checks the seeded value and the key set.
    check("SP11 the first write keeps the seeded nested value", first_write.get("updates", {}).get("autoCheck") is False, first_write)
    check("SP11 the first write's sort is kind, descending", first_write.get("sort") == {"key": "kind", "reverse": True}, first_write)
    check("SP11 the first write's folderSorts is exactly this folder's kind order",
          first_write.get("folderSorts") == {str(ordered): {"key": "kind", "reverse": True}}, first_write)
    check("SP11 the first write holds exactly SCHEMA's top-level keys", set(first_write) == schema_keys, first_write)
    sorting.key("s")
    sorting.until("s leaves kind for name", lambda state: state["sortBy"] == "name" and not state["sortDesc"] and names(state) == by_name)
    sorting.key("s")
    sorting.until("s steps to size", lambda state: state["sortBy"] == "size" and names(state) == by_size)
    sorting.key("s")
    sorting.until("s steps to modified", lambda state: state["sortBy"] == "mtime" and names(state) == by_modified)
    sorting.key("s")
    sorting.until("s wraps to name and never lands on kind", lambda state: state["sortBy"] == "name" and names(state) == by_name)

    sorting.mark("bravo.txt")
    sorting.click("Sort by Size")
    sorting.until("a header click sorts that column ascending, returns to the first row and keeps the list's keys",
                  lambda state: state["sortBy"] == "size" and not state["sortDesc"] and names(state) == by_size
                  and state["cursor"] == 0 and state["listFocus"] and state["headerMark"] == "size")
    sorting.click("Sort by Size")
    reverse_size = ["refused", "folder", "bravo.txt", "photo.png", "alpha.txt", "charlie.txt"]
    clicked = sorting.until("a second click reverses it", lambda state: state["sortDesc"] and names(state) == reverse_size)
    check("SP11 the mark is a path and survives both reorders", [mark["path"] for mark in clicked["marks"]] == [str(ordered / "bravo.txt")], clicked["marks"])
    sorting.capture("size-descending")
    sorting.click("Sort by Modified")
    sorting.until("a click on another column starts ascending", lambda state: state["sortBy"] == "mtime" and not state["sortDesc"] and names(state) == by_modified)

    sorting.key(".")
    sorting.until("the hidden toggle re-reads the folder in the chosen order", lambda state: state["state"] == "ready" and state["sortBy"] == "mtime" and names(state) == by_modified_shown)
    sorting.key(".")
    sorting.until("and hides the dotfile again", lambda state: state["state"] == "ready" and names(state) == by_modified)
    write(ordered / "delta.txt", "xx")
    sorting.until("a watched refresh keeps the chosen order", lambda state: names(state) == by_modified + ["delta.txt"])
    guard(ordered / "delta.txt").unlink()
    sorting.until("and keeps it when the file goes", lambda state: names(state) == by_modified)

    guard(ordered / "refused").chmod(0)
    try:
        sorting.row("refused")
        sorting.key("-k", "Return")
        sorting.until("a refused folder is told apart from an empty one", lambda state: state["path"] == str(ordered / "refused")
                      and state["listingFailed"] and not state["sortable"] and state["total"] == 0)
        check("SP11 the header is disabled over a refused folder", not sorting.control("Sort by Name")["enabled"], sorting.state()["controls"])
        sorting.key("s")
        sorting.key("S")
        sorting.click("Sort by Name")
        time.sleep(NO_EVENT_WAIT_S)
        refused = sorting.state()
        check("SP11 no sort draws the previous folder's rows under a refused path",
              refused["total"] == 0 and refused["rows"] == [] and refused["sortBy"] == "mtime" and not refused["sortDesc"], refused)
    finally:
        guard(ordered / "refused").chmod(0o700)
    sorting.click("Back")
    sorting.until("Back restores a sortable listing in the chosen order", lambda state: state["path"] == str(ordered)
                  and state["sortable"] and not state["listingFailed"] and names(state) == by_modified)

    sorting.click("Recent")
    recent = sorting.until("Recent draws no sort mark and cannot be sorted", lambda state: state["path"] == "flea:recent"
                           and state["state"] != "loading" and not state["sortable"] and state["listFocus"]
                           and state["headerMark"] == "" and [Path(name).name for name in names(state)] == recent_order)
    check("SP11 the header is disabled over Recent", not sorting.control("Sort by Name")["enabled"], recent["controls"])
    sorting.key("s")
    sorting.click("Sort by Name")
    time.sleep(NO_EVENT_WAIT_S)
    check("SP11 Recent keeps the desktop's order", names(sorting.state()) == names(recent) and sorting.state()["sortBy"] == "mtime", sorting.state())
    sorting.capture("recent")
    sorting.click("Back")
    sorting.until("leaving Recent restores the chosen order", lambda state: state["path"] == str(ordered) and names(state) == by_modified)
    # a3c7508f6: every sort writes the sort leaf and its folder's entry over the first write's whole file, and nothing else.
    expected = {**first_write, "sort": {"key": "date", "reverse": False}, "folderSorts": {str(ordered): {"key": "date", "reverse": False}}}
    check("SP11 ui.json holds the last sort and the folder's remembered order, nothing else", json.loads(state_file.read_text()) == expected, state_file.read_text())
    guard(root / "data/recently-used.xbel").unlink()
    # SP12 below starts from name, the order a box with nothing saved opens in.
    guard(state_file).unlink()
    sorting.row("alpha.txt")
    sorting.key("-k", "Return")
    sorting.answered(0, [(ordered / "bravo.txt").as_uri()])

    write(large / "aaa-first.txt", "x" * 5000)
    scrolled = Request("SP12-sorting-scrolled", folder=large).opened()
    scrolled.key("-k", "End")
    scrolled.until("the viewport left the top", lambda state: state["held"] > 0)
    scrolled.key("s")
    # A sort is refused while a listing is loading, so S sent on the heels of s can be dropped.
    scrolled.until("the first sort lands before the second is asked for",
                   lambda state: state["sortBy"] == "size" and not state["sortDesc"] and state["state"] == "ready"
                   and state["held"] == 0 and state["cursor"] == 0)
    scrolled.key("-k", "End")
    scrolled.until("the viewport left the top again", lambda state: state["held"] > 0)
    scrolled.key("S")
    scrolled.until("a sort returns the viewport and the cursor to the first row",
                   lambda state: state["sortBy"] == "size" and state["sortDesc"] and state["held"] == 0
                   and state["cursor"] == 0 and names(state)[:2] == ["aaa-first.txt", "zz-last.png"])
    scrolled.cancel()
    guard(large / "aaa-first.txt").unlink()

    write(ordered / "report.txt", "taken")
    saving = Request("SP13-sorting-save", method="SaveFile", folder=ordered, current_name=GLib.Variant("s", "report.txt")).opened()
    # a3c7508f6: SP12's last sort is the default now, so the save opens on size descending and s steps to modified.
    saving.until("the collision is reviewed", lambda state: state["saveReady"] and state["collision"] and state["state"] == "ready"
                 and state["sortBy"] == "size" and state["sortDesc"])
    saving.key("-M", "shift", "-k", "Tab", "-m", "shift")
    saving.until("deliberate list focus before sorting", lambda state: state["listFocus"])
    saving.key("s")
    saving.until("a sort keeps the filename and its collision answer", lambda state: state["sortBy"] == "mtime" and not state["sortDesc"]
                 and state["state"] == "ready" and state["saveName"] == "report.txt" and state["saveReady"] and state["collision"])
    saving.focus_control("Filename")
    saving.key("-k", "End")
    # One key per call: omarchy-drive joins several arguments with a space, which the field would take as text.
    saving.key("s")
    saving.key("S")
    typed = saving.until("s and S typed into Filename are text", lambda state: state["saveName"] == "report.txtsS")
    check("SP13 typing in Filename sorts nothing", typed["sortBy"] == "mtime" and not typed["sortDesc"], typed)
    saving.cancel()
    guard(ordered / "report.txt").unlink()
    # The groups after this one start from nothing saved, as they did before a picker sort was remembered.
    guard(state_file).unlink(missing_ok=True)


# Sample input: var MISSING_SENTENCE = "%1 does not exist or cannot be reached."
def pathbar_sentence(constant):
    text = (REPO / "ui/js/PathBar.js").read_text()
    found = re.search(r'^var ' + constant + r' = "(.*)";?$', text, re.M)
    if not found:
        raise AssertionError(f"pathbar constant {constant} missing")
    return found.group(1)


def test_path():
    siblings = guard(fixture / "sibs")
    siblings.mkdir()
    for name in ["proj-alpha", "proj-beta"]:
        guard(siblings / name).mkdir()
    write(siblings / "note.txt", "note")
    leaf = guard(fixture / "deep")
    for name in ["this-is-a-long-folder-name-1", "this-is-a-long-folder-name-2", "this-is-a-long-folder-name-3",
                 "this-is-a-long-folder-name-4", "this-is-a-long-folder-name-5", "this-is-a-long-folder-name-6"]:
        leaf = leaf / name
    guard(leaf).mkdir(parents=True, exist_ok=True)
    guard(root / "data/recently-used.xbel").unlink(missing_ok=True)
    missing_sentence = pathbar_sentence("MISSING_SENTENCE")
    filtered_sentence = pathbar_sentence("FILTERED_SENTENCE")
    refused_sentence = pathbar_sentence("REFUSED_SENTENCE")

    overflow = Request("SP14-overflow", folder=leaf).opened()
    overflow.resize(840, 513)
    overflow.until("deep path at the shipped window",
                  lambda state: state["path"] == str(leaf) and state["width"] == 840 and state["height"] == 513)
    overflow.capture("path-overflow")
    overflow.cancel()

    entry = Request("SP14-entry", folder=siblings).opened()
    entry.resize(840, 513)
    entry.key(":")
    entry.until("colon opens the path field", lambda state: state["pathBarOpen"])
    entry.key(str(siblings / "note.txt"))
    entry.until("a typed file path stands in the open field and keeps the listing",
                lambda state: state["pathBarOpen"] and state["pathFieldText"] == str(siblings / "note.txt") and state["path"] == str(siblings))
    entry.capture("path-file")
    entry.key("-k", "Escape")
    entry.until("Escape closes the path field", lambda state: not state["pathBarOpen"])

    entry.key(":")
    entry.until("colon reopens the path field", lambda state: state["pathBarOpen"])
    entry.key(str(siblings) + "/proj-")
    entry.key("-k", "Tab")
    completed = entry.until("Tab answers the shared prefix",
                            lambda state: state["message"] == "2 items start with that" and not state["messageError"])
    check("SP14 Tab completion says its line", completed["message"] == "2 items start with that", completed["message"])
    entry.capture("path-tab-complete")
    entry.key("-k", "Escape")
    entry.until("Escape closes the path field", lambda state: not state["pathBarOpen"])

    absent = fixture / "does-not-exist"
    entry.key(":")
    entry.until("colon reopens the path field", lambda state: state["pathBarOpen"])
    entry.key(str(absent))
    entry.key("-k", "Return")
    missing = entry.until("a missing path is refused in the footer",
                          lambda state: state["message"] == missing_sentence.replace("%1", str(absent)) and state["messageError"])
    check("SP14 MISSING_SENTENCE refusal", missing["message"] == missing_sentence.replace("%1", str(absent)), missing["message"])
    entry.capture("path-missing")
    entry.key("-k", "Escape")
    entry.until("Escape closes the path field", lambda state: not state["pathBarOpen"])

    other = "file://another-host.example/shared/doc.txt"
    entry.key(":")
    entry.until("colon reopens the path field", lambda state: state["pathBarOpen"])
    entry.key(other)
    entry.key("-k", "Return")
    refused = entry.until("another host is refused without error",
                          lambda state: state["message"] == refused_sentence and not state["messageError"])
    check("SP14 REFUSED_SENTENCE refusal", refused["message"] == refused_sentence, refused["message"])
    entry.capture("path-other-host")
    entry.key("-k", "Escape")
    entry.until("Escape closes the path field", lambda state: not state["pathBarOpen"])
    entry.cancel()

    submit = Request("SP14-entry-submit", folder=siblings).opened()
    # The cursor parks on a folder row, so only the typed path can answer the file URI.
    submit.row("proj-alpha")
    parked = submit.state()
    check("SP14 entry-submit parks the cursor off the typed file", parked["cursorName"] == "proj-alpha", parked["cursorName"])
    submit.key(":")
    submit.until("colon opens the path field", lambda state: state["pathBarOpen"])
    submit.key(str(siblings / "note.txt"))
    submit.until("the typed file path stands in the open field",
                lambda state: state["pathBarOpen"] and state["pathFieldText"] == str(siblings / "note.txt"))
    submit.key("-k", "Return")
    submit.answered(0, [(siblings / "note.txt").as_uri()])

    pictures = [("Images", [(0, "*.png")])]
    filtered = Request("SP14-filtered", folder=large, filters=GLib.Variant("a(sa(us))", pictures),
                       current_filter=GLib.Variant("(sa(us))", pictures[0])).opened()
    filtered.until("Images filter hides the text files", lambda state: state["total"] == 1)
    filtered.resize(840, 513)
    hidden = large / "file-007.txt"
    filtered.key(":")
    filtered.until("colon opens the path field", lambda state: state["pathBarOpen"])
    filtered.key(str(hidden))
    filtered.key("-k", "Return")
    told = filtered.until("a filtered file is told apart in the footer",
                          lambda state: state["message"] == filtered_sentence.replace("%1", hidden.name) and state["messageError"])
    check("SP14 FILTERED_SENTENCE refusal", told["message"] == filtered_sentence.replace("%1", hidden.name), told["message"])
    filtered.capture("path-filtered")
    filtered.key("-k", "Escape")
    filtered.until("Escape closes the path field", lambda state: not state["pathBarOpen"])
    filtered.cancel()

    recent = Request("SP14-recent").opened()
    recent.click("Recent")
    fresh = recent.until("Recent opens from the folder", lambda state: state["path"] == "flea:recent" and state["state"] != "loading")
    back = [item for item in fresh["controls"] if item["name"] == "Back"]
    up = [item for item in fresh["controls"] if item["name"].startswith("Parent folder")]
    # Back returns to the folder the dialog opened on; only Up has nowhere to go in Recent.
    check("SP14 Recent opened from a folder keeps Back live", len(back) == 1 and back[0]["enabled"], back)
    check("SP14 Recent leaves Up dead", len(up) == 1 and not up[0]["enabled"], up)
    recent.capture("path-recent-fresh")
    recent.click("Back")
    recent.until("Back from Recent returns to the folder the dialog opened on",
                 lambda state: state["path"] == str(fixture) and state["state"] != "loading")
    recent.cancel()

    focus = Request("SP14-focus", folder=siblings).opened()
    focus.key("-M", "ctrl", "-k", "l", "-m", "ctrl")
    focus.until("Ctrl+L opens the path field", lambda state: state["pathBarOpen"])
    centre = run([*IPC.command(), "--pid", focus.pid, "call", "fleapicker", "rowCentre", 0], picker_env)
    focus.point(centre)
    focus.until("a row tap moves focus out and closes the field", lambda state: not state["pathBarOpen"] and state["listFocus"])
    focus.capture("path-focus-left")
    focus.cancel()

    jump_base = guard(fixture / "jump")
    fav_names = [f"jump-fav-{index:02d}" for index in range(5)]
    recent_names = [f"jump-recent-{index:02d}" for index in range(5)]
    for name in fav_names + recent_names:
        guard(jump_base / name).mkdir(parents=True, exist_ok=True)
        write(jump_base / name / "note.txt", "note")
    state_file = root / "state/flea/ui.json"
    saved_state = state_file.read_text() if state_file.exists() else None
    try:
        try:
            state_doc = json.loads(state_file.read_text())
        except (OSError, ValueError):
            state_doc = {}
        if not isinstance(state_doc, dict):
            state_doc = {}
        places = dict(state_doc.get("places") or {})
        places["favourites"] = [{"label": name, "path": str(jump_base / name)} for name in fav_names]
        state_doc["places"] = places
        write(state_file, json.dumps(state_doc))
        bookmarks = "\n".join(
            f'  <bookmark href="file://{jump_base / name}/note.txt" added="2026-09-22T10:00:0{index}Z"'
            f' modified="2026-09-22T10:00:0{index}Z" visited="2026-09-22T10:00:0{index}Z"/>'
            for index, name in enumerate(recent_names))
        write(root / "data/recently-used.xbel",
              f'<?xml version="1.0" encoding="UTF-8"?>\n<xbel version="1.0">\n{bookmarks}\n</xbel>\n')

        jumper = Request("SP14-jump", folder=siblings).opened()
        jumper.resize(840, 513)
        jumper.key(":")
        jumper.until("colon opens the path field", lambda state: state["pathBarOpen"])
        jumper.key("jump")
        jumper.until("Jump lists the seeded folders",
                    lambda state: state["jump"]["open"] and state["jump"]["rows"] == 10
                    and any("jump-fav-" in path for path in state["jump"]["paths"])
                    and any("jump-recent-" in path for path in state["jump"]["paths"]))
        jumper.capture("path-folder-jump")
        jumper.resize(800, 280)
        jumper.until("Jump overflows the short window",
                    lambda state: state["jump"]["open"] and state["jump"]["rows"] > state["jump"]["visibleRows"])
        scrolled = jumper.state()
        for _ in range(scrolled["jump"]["rows"]):
            jumper.key("-k", "Down")
        jumper.until("a scrolled Jump leaves the top",
                    lambda state: state["jump"]["open"] and state["jump"]["rows"] > state["jump"]["visibleRows"]
                    and state["jump"]["contentY"] > 0)
        jumper.capture("path-jump-scroll")
        jumper.key("-k", "Escape")
        jumper.until("Escape closes the path field", lambda state: not state["pathBarOpen"])
        jumper.cancel()

        grid = Request("SP14-grid", folder=siblings).opened()
        grid.resize(840, 513)
        grid.key("-M", "ctrl", "-k", "3", "-m", "ctrl")
        grid.until("Ctrl+3 shows the grid", lambda state: state["view"] == "grid")
        grid.key(":")
        grid.until("colon opens the path field", lambda state: state["pathBarOpen"])
        grid.key("jump")
        grid.until("Jump lists in grid view",
                  lambda state: state["view"] == "grid" and state["jump"]["open"] and state["jump"]["rows"] == 10
                  and any("jump-fav-" in path for path in state["jump"]["paths"])
                  and any("jump-recent-" in path for path in state["jump"]["paths"]))
        grid.capture("path-jump-grid")
        grid.key("-k", "Escape")
        grid.until("Escape closes the path field", lambda state: not state["pathBarOpen"])
        grid.cancel()
    finally:
        # The seeded places and Recent are this group's alone, so a group run after it opens on what it found, a failure included.
        guard(root / "data/recently-used.xbel").unlink(missing_ok=True)
        if saved_state is None:
            guard(state_file).unlink(missing_ok=True)
        else:
            write(state_file, saved_state)


# Sample input: FLEA_DISPLAY_LOCK_FD=9 names the runner's already locked open descriptor.
def take_display_lock(runtime_dir):
    raw = os.environ.get("FLEA_DISPLAY_LOCK_FD")
    expected = Path(runtime_dir) / "flea-display.lock"
    try:
        if raw is None:
            held = open(expected, "a")
        else:
            if re.fullmatch(r"[0-9]+", raw) is None:
                raise ValueError("expected a decimal open fd")
            held = os.fdopen(os.dup(int(raw)), "a")
        try:
            if raw is not None:
                inherited = os.fstat(held.fileno())
                runtime = expected.stat()
                if (inherited.st_dev, inherited.st_ino) != (runtime.st_dev, runtime.st_ino):
                    actual = os.readlink("/proc/self/fd/" + str(held.fileno()))
                    raise ValueError("inherited descriptor " + raw + " (" + actual + ") is not " + str(expected))
            fcntl.flock(held, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except Exception:
            held.close()
            raise
        return held
    except (OSError, ValueError, OverflowError) as error:
        message = "FAIL display lock FLEA_DISPLAY_LOCK_FD=" + repr(raw) + " expected " + str(expected) + ": " + str(error)
        print(message, flush=True)
        raise AssertionError(message) from error


def main():
    from picker_issue313_native import test_issue313
    groups = {"single": test_single, "multiple": test_multiple, "directory": test_directory,
              "filters": test_filters, "changed": test_changed, "save": test_save,
              "savefocus": test_savefocus, "cancel": test_cancel, "failure": test_failure, "keys": test_keys,
              "sorting": test_sorting, "path": test_path}
    groups["issue313"] = lambda: test_issue313(globals())
    selected = sys.argv[1:] or [name for name in groups if name != "issue313"]
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
    display_lock = take_display_lock(drive_env["XDG_RUNTIME_DIR"])
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
    check_build_inputs()
    listed, how = build_inputs(BIN)
    check("candidate binary built from this tree", how != "another tree", {"inputs": how, "foreign": [str(path) for path in listed[:3]]})
    inputs = [*listed, REPO / "Cargo.toml", REPO / "Cargo.lock"]
    if (REPO / "build.rs").is_file(): inputs.append(REPO / "build.rs")
    is_fresh, detail = fresh(BIN, inputs, how)
    check("candidate binary newer than build inputs", is_fresh, detail)
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
    daemon = subprocess.Popen(["dbus-daemon", "--session", "--nofork", "--print-address=1"], env=picker_env, stdout=subprocess.PIPE, stderr=guard(root / "bus.log").open("w"), text=True, start_new_session=True, close_fds=True)
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
    # Evidence and its marker deliberately remain for maintainer inspection and candidate attribution.
