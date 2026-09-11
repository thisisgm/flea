#!/usr/bin/env bash
# Standalone native TUI proof; owns flea-display.lock and preserves its marked /tmp evidence root.
set -euo pipefail
exec python3 - "$0" "$@" <<'PY'
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import re
import shlex
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import wave

SCRIPT = Path(sys.argv[1]).resolve()
ARGS = sys.argv[2:]
ESCAPE = re.compile(rb"\x1b\[[0-?]*[ -/]*[@-~]")
STRING_CONTROL = re.compile(rb"\x1b(?:[P_^].*?(?:\x1b\\|$)|].*?(?:\x07|\x1b\\|$))", re.DOTALL)
WAIT_SECONDS = 20
POLL_SECONDS = 0.05
PRESETS = ("default", "vim", "mac", "windows")
TERMINALS = ("foot", "kitty")


def command(args, **options):
    result = subprocess.run([str(arg) for arg in args], capture_output=True, **options)
    if result.returncode:
        raise RuntimeError(f"{shlex.join(map(str, args))} exited {result.returncode}: {result.stderr.decode(errors='replace').strip()}")
    return result.stdout


def guard(root, path):
    root, path = Path(root), Path(path)
    if not root.is_absolute() or not path.is_absolute() or not (root / ".flea-test-sandbox").is_file():
        raise RuntimeError("TUI fixture requires absolute paths and its own marker")
    path = path.resolve()
    if not path.is_relative_to(root.resolve()) or path == root.resolve():
        raise RuntimeError(f"TUI fixture path escaped its owned root: {path}")
    return path


# Native output: ESC[H starts a frame; ESC[rows;1H introduces its full-width final row, including editors.
def frame(data, rows, columns):
    raw = data.rsplit(b"\x1b[H", 1)[-1] if b"\x1b[H" in data else b""
    screen = STRING_CONTROL.sub(b"", raw)
    footer = b"\x1b[" + str(rows).encode() + b";1H"
    if footer not in screen or len(ESCAPE.sub(b"", screen.split(footer, 1)[1]).decode("utf-8", errors="replace")) < columns:
        return b"", ""
    text = re.sub(rb"\x1b\[\d+;1H", b"\n", screen)
    return raw, ESCAPE.sub(b"", text).decode("utf-8", errors="replace")


def separator_pixel(pixels, width, height, grid_width, grid_height, rows):
    if min(width, height, grid_width, grid_height, rows) <= 0 or grid_width > width or grid_height > height \
            or len(pixels) != width * height * 3:
        raise RuntimeError("terminal separator image and measured grid disagree")
    stride = width * 3
    cell_height = grid_height / rows
    transitions = []
    # Search every possible row-2 placement; only an observed full-grid-width stroke permits input.
    for y in range(max(1, math.floor(cell_height)), min(height, math.ceil(height - grid_height + 2 * cell_height) + 1)):
        before, after = pixels[(y - 1) * stride:y * stride], pixels[y * stride:(y + 1) * stride]
        changed = [x for x in range(width) if before[x * 3:(x + 1) * 3] != after[x * 3:(x + 1) * 3]]
        if changed:
            transitions.append((y, changed[0], changed[-1], len(changed)))
    anchors = []
    for index, (top, _, _, _) in enumerate(transitions):
        for end in range(index + 1, len(transitions)):
            bottom = transitions[end][0]
            if bottom - top >= cell_height:
                break
            stroke = transitions[index:end + 1]
            left, right = min(item[1] for item in stroke), max(item[2] for item in stroke)
            if right - left + 1 != grid_width or any(count != last - first + 1 for _, first, last, count in stroke):
                continue
            section = lambda y: pixels[y * stride + left * 3:y * stride + (right + 1) * 3]
            # Antialiasing can vary every raster row; the observed stroke must return to its original background.
            background = section(top - 1)
            if section(bottom) == background and all(section(y) != background for y in range(top, bottom)):
                anchors.append((left + grid_width // 2, (top + bottom - 1) // 2))
    # ponytail: Clipped or overlaid rules require an unobscured native capture at the same geometry.
    if len(anchors) != 1:
        raise RuntimeError(f"terminal row-2 separator is missing or ambiguous: {len(anchors)} strokes")
    return anchors[0]


def separator_image(path, terminal_size):
    # ImageMagick identify: 2536 1386; RGB output then contains exactly three bytes per pixel.
    dimensions = command(["magick", "identify", "-format", "%w %h", path]).decode().split()
    if len(dimensions) != 2 or not all(value.isdecimal() for value in dimensions):
        raise RuntimeError("ImageMagick returned invalid terminal screenshot dimensions")
    width, height = map(int, dimensions)
    rows, _, grid_width, grid_height = terminal_size
    pixels = command(["magick", path, "-depth", "8", "rgb:-"])
    return (width, height), separator_pixel(pixels, width, height, grid_width, grid_height, rows)


def calibration_check(path, dimensions, pixels):
    samples = {"099561e36935a3c523bf4aade3cf477282f219a6e976883ef8b1211ed1865a8e": (1267, 52),
               "8737bf7e888d53c5476e1befdb136fac2b6e7294f2c4622447c8ed10968c4d18": (1267, 53)}
    expected = samples.get(hashlib.sha256(path.read_bytes()).hexdigest())
    if expected is None:
        raise RuntimeError("calibration check requires an archived f0d or cc1 Kitty listing.png sample")
    metadata = json.loads(path.with_suffix(".json").read_text())
    rows, _, grid_width, grid_height = metadata["pty_rows_columns_pixels"]
    width, height = dimensions
    cell_height = grid_height / rows
    padding = height - grid_height
    old_anchor = round((padding + 2 * cell_height) / 2)
    assert not padding <= old_anchor < 2 * cell_height
    assert separator_pixel(pixels, width, height, grid_width, grid_height, rows) == expected
    background = bytes((20, 24, 26))
    blank = background * width * height
    ambiguous = bytearray(blank)
    for y in (52, 72):
        for stripe_y in (y, y + 1):
            offset = (stripe_y * width + 21) * 3
            ambiguous[offset:offset + grid_width * 3] = bytes((132, 139, 145)) * grid_width
    for sample in (blank, ambiguous):
        try:
            separator_pixel(sample, width, height, grid_width, grid_height, rows)
        except RuntimeError as error:
            assert "missing or ambiguous" in str(error)
        else:
            raise AssertionError("calibration accepted an absent or ambiguous separator")
    print("TUI_CALIBRATION_SELF_CHECK 4 passed; native SGR proof not exercised")


def provider_fixture(case):
    for directory in ["providers/bin", "home"]:
        guard(case, case / directory).mkdir(parents=True)
    binary_path = case / "providers/bin"
    helpers = {"stty": Path(shutil.which("stty")).resolve(strict=True), "python3": Path(sys.executable).resolve(strict=True)}
    for name, target in helpers.items():
        guard(case, binary_path / name).symlink_to(target)
    for name in ["tailscale", "omarchy-tailscale-send", "wl-copy", "wl-paste"]:
        helper = guard(case, binary_path / name)
        helper.write_text("#!/bin/sh\nexec " + shlex.join([shutil.which("bash"), str(SCRIPT), "--provider-helper", str(case), name]) + ' "$@"\n')
        helper.chmod(0o700)
    guard(case, case / "providers/calls.jsonl").write_text("")
    os.mkfifo(guard(case, case / "providers/status-release"), 0o600)
    theme = Path.home() / ".local/state/omarchy/current/theme/colors.toml"
    if theme.is_file():
        destination = guard(case, case / "home/.local/state/omarchy/current/theme/colors.toml")
        destination.parent.mkdir(parents=True)
        destination.write_bytes(theme.read_bytes())
    guard(case, case / "providers/helpers.json").write_text(json.dumps({name: str(path) for name, path in helpers.items()}))
    return str(binary_path)


def provider_helper(case, name, args):
    case = Path(case)
    state = json.loads(guard(case, case / "providers/control.json").read_text())
    call = {"name": name, "args": args, "pid": os.getpid(), "state": state["name"]}
    if name == "tailscale":
        if args != ["status", "--json"]:
            raise RuntimeError("provider fixture refuses every non-status Tailscale invocation")
        status = state["statusExit"]
    elif name == "omarchy-tailscale-send":
        if len(args) < 2 or args[0] not in ("alpha.fixture.invalid", "beta.fixture.invalid"):
            raise RuntimeError("sender fixture refuses an unexpected peer or empty selection")
        call["sources"] = [{"path": str(guard(case, Path(path))),
                            "sha256": hashlib.sha256(guard(case, Path(path)).read_bytes()).hexdigest()} for path in args[1:]]
        status = state["senderExit"]
    elif name in ("wl-copy", "wl-paste"):
        status = 97  # Clipboard spies never connect to the compositor.
    else:
        raise RuntimeError("unknown private provider helper")
    with guard(case, case / "providers/calls.jsonl").open("a") as output:
        output.write(json.dumps(call) + "\n")
    if name == "tailscale":
        if state.get("statusGate"):
            with guard(case, case / "providers/status-release").open("rb", buffering=0) as release:
                if release.read(1) != b"r":
                    raise RuntimeError("status fixture gate closed without its release")
        print(json.dumps(state["status"]))
    return status


def product_environment(case):
    guard(case, case / "listing")
    environment = os.environ.copy()
    for name, directory in [("XDG_STATE_HOME", "state"), ("XDG_CONFIG_HOME", "config"),
                            ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")]:
        environment[name] = str(guard(case, case / directory))
    if "FLEA_TUI_PROVIDER_BIN" in environment:
        expected = str(guard(case, case / "providers/bin"))
        if environment["FLEA_TUI_PROVIDER_BIN"] != expected:
            raise RuntimeError("provider PATH does not belong to this native case")
        environment["PATH"] = expected
        environment["HOME"] = str(guard(case, case / "home"))
    return environment


def child(case, binary):
    case, binary = Path(case), Path(binary)
    environment = product_environment(case)
    # A concrete PTY remains observable from SSH; reopening /dev/tty would select the observer's controlling terminal.
    with open(os.ttyname(sys.stdout.fileno()), "r+b", buffering=0) as terminal:
        before = command(["stty", "-g"], stdin=terminal).decode().strip()
        process = subprocess.Popen([str(binary), "--tui", str(case / "listing")],
                                   stdin=terminal, stdout=terminal, stderr=terminal, env=environment)
        (case / "product.pid").write_text(str(process.pid))
        status = process.wait()
        after = command(["stty", "-g"], stdin=terminal).decode().strip()
        (case / "exit.json").write_text(json.dumps({"status": status, "before": before, "after": after}))
    return status


def provider_self_check():
    case = Path(tempfile.mkdtemp(prefix="flea-tui-provider-check.", dir="/tmp")).resolve()
    (case / ".flea-test-sandbox").write_text("Flea provider helper self-check\n")
    for directory in ("listing", "state", "config", "data", "cache", "fallback"):
        guard(case, case / directory).mkdir()
    source = guard(case, case / "listing/source.txt")
    source.write_text("provider source stays unchanged\n")
    control = {"name": "self-check", "status": {"BackendState": "NeedsLogin"}, "statusExit": 0, "senderExit": 0}
    binary_path = provider_fixture(case)
    guard(case, case / "providers/control.json").write_text(json.dumps(control))
    fallback = guard(case, case / "fallback/omarchy-tailscale-send")
    fallback.write_text("#!/bin/sh\nexit 88\n")
    fallback.chmod(0o700)
    before = dict(os.environ)
    try:
        os.environ["PATH"] = str(fallback.parent) + os.pathsep + before["PATH"]
        os.environ["FLEA_TUI_PROVIDER_BIN"] = binary_path
        environment = product_environment(case)
    finally:
        os.environ.clear()
        os.environ.update(before)
    assert environment["PATH"] == binary_path and environment["HOME"] == str(case / "home")
    for name, directory in (("XDG_STATE_HOME", "state"), ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")):
        assert environment[name] == str(case / directory)
    assert json.loads(command(["tailscale", "status", "--json"], env=environment)) == control["status"]
    command(["omarchy-tailscale-send", "alpha.fixture.invalid", source], env=environment)
    sender = guard(case, case / "providers/bin/omarchy-tailscale-send")
    held = guard(case, case / "providers/sender-held")
    sender.rename(held)
    for exception in (FileNotFoundError, PermissionError):
        try:
            subprocess.run(["omarchy-tailscale-send", "alpha.fixture.invalid", str(source)], env=environment, check=True)
        except exception:
            pass
        else:
            raise AssertionError("private sender absence/permissions fell through to another PATH")
        if exception is FileNotFoundError:
            guard(case, held).rename(guard(case, sender))
            sender.chmod(0o600)
    sender.chmod(0o700)
    control["senderExit"] = 7
    guard(case, case / "providers/control.json").write_text(json.dumps(control))
    assert subprocess.run(["omarchy-tailscale-send", "alpha.fixture.invalid", str(source)], env=environment).returncode == 7
    calls = [json.loads(line) for line in guard(case, case / "providers/calls.jsonl").read_text().splitlines()]
    assert [call["name"] for call in calls] == ["tailscale", "omarchy-tailscale-send", "omarchy-tailscale-send"]
    assert calls[-1]["args"] == ["alpha.fixture.invalid", str(source)]
    assert calls[-1]["sources"] == [{"path": str(source), "sha256": hashlib.sha256(source.read_bytes()).hexdigest()}]
    print(f"TUI_PROVIDER_SELF_CHECK private-HOME/XDG/PATH no-fallback status exact-dispatch exit-failure clipboard-uninvoked passed; native coverage not exercised; root={case}")


class Native:
    def __init__(self, root, binary, preset, terminal):
        self.root, self.binary = root, binary
        self.preset, self.terminal = preset, terminal
        self.case = guard(root, root / (terminal + "-" + preset))
        self.case.mkdir()
        (self.case / ".flea-test-sandbox").write_text("Flea TUI native fixture\n")
        self.title = "flea-tui-" + root.name + "-" + self.case.name
        self.address = None
        self.product_pid = None
        self.checks = 0
        self.undriven = []
        self.separator_anchors = {}
        self.environment = os.environ.copy()
        self.environment["FLEA_TUI_TEST_CASE"] = str(self.case)
        self.log = open(self.case / "commands.log", "xb", buffering=0)
        for directory in ["listing/amber", "listing/bronze", "state/flea", "config", "data", "cache", "evidence"]:
            guard(self.case, self.case / directory).mkdir(parents=True, exist_ok=True)
        for name in ["charlie.txt", "delta-needleproof.txt", "echo.txt", ".hidden-proof"]:
            guard(self.case, self.case / "listing" / name).write_text(name + "\n")
        (self.case / "listing/amber/nested-proof.txt").write_text("native navigation proof\n")
        (self.case / "state/flea/ui.json").write_text(json.dumps({"keys": preset, "hidden": False,
            "sort": {"key": "name", "reverse": False}, "preview": {"loadOn": "manual", "column": True}}))
        (self.case / "config/xdg-terminals.list").write_text(terminal + ".desktop\n")

    def drive(self, *args):
        invocation = ["omarchy-drive", *map(str, args)]
        self.log.write((shlex.join(invocation) + "\n").encode())
        result = subprocess.run(invocation, capture_output=True, env=self.environment)
        self.log.write(result.stdout + result.stderr + f"exit={result.returncode}\n".encode())
        if result.returncode:
            raise RuntimeError(f"native driver refused or failed ({result.returncode}): {result.stderr.decode(errors='replace').strip()}")
        return result.stdout

    def owned_process(self, pid):
        try:
            process = Path("/proc") / str(pid)
            values = (process / "environ").read_bytes().split(b"\0")
            return process.stat().st_uid == os.getuid() and ("FLEA_TUI_TEST_CASE=" + str(self.case)).encode() in values
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            return False

    def window(self):
        # hyprctl clients adds PID identity omitted by omarchy-drive's normalized window payload.
        clients = json.loads(command(["hyprctl", "clients", "-j"]))
        matches = [item for item in clients if item.get("address") == self.address] if self.address else [
            item for item in clients if item.get("title") == self.title or item.get("initialTitle") == self.title]
        if len(matches) != 1 or not self.owned_process(matches[0]["pid"]):
            raise RuntimeError("native terminal window is missing, ambiguous or not owned by this run")
        return matches[0]

    def identity(self):
        window = self.window()
        if self.product_pid is None:
            self.product_pid = int((self.case / "product.pid").read_text())
        process = Path("/proc") / str(self.product_pid)
        if not self.owned_process(self.product_pid) or (process / "exe").resolve() != self.binary:
            raise RuntimeError("TUI PID does not execute the identified candidate")
        if (process / "cmdline").read_bytes().split(b"\0")[:3] != [os.fsencode(self.binary), b"--tui", os.fsencode(self.case / "listing")]:
            raise RuntimeError("TUI command line does not match this fixture")
        return window

    def wait(self, label, predicate):
        deadline = time.monotonic() + WAIT_SECONDS
        while time.monotonic() < deadline:
            if predicate():
                self.checks += 1
                print(f"TUI_PASS {label}", flush=True)
                return
            time.sleep(POLL_SECONDS)
        raise RuntimeError(f"native TUI did not reach {label} within {WAIT_SECONDS}s")

    def terminal_size(self):
        self.identity()
        process = Path("/proc") / str(self.product_pid)
        descriptors = {str(number): os.readlink(process / "fd" / str(number)) for number in range(3)}
        # /proc/PID/stat: 2970648 (flea) R 2970620 2970620 2970620 34817 ...; tty_nr is field seven.
        tty_number = int((process / "stat").read_text().rsplit(") ", 1)[1].split()[4])
        diagnostics = {"product_pid": self.product_pid, "descriptors": descriptors, "controlling_tty": tty_number}
        if diagnostics != getattr(self, "terminal_diagnostics", None):
            self.log.write(("TUI_PTY " + json.dumps(diagnostics) + "\n").encode())
            self.terminal_diagnostics = diagnostics
        descriptor = os.open(f"/proc/{self.product_pid}/fd/1", os.O_RDONLY | os.O_NOCTTY)
        try:
            return struct.unpack("HHHH", fcntl.ioctl(descriptor, termios.TIOCGWINSZ, bytes(8)))
        finally:
            os.close(descriptor)

    def snapshot(self, label, predicate):
        def reached():
            path = self.case / "output.bin"
            self.size = self.terminal_size()
            self.raw, self.text = frame(path.read_bytes() if path.exists() else b"", *self.size[:2])
            return bool(self.raw) and predicate(self.text)
        self.wait(label, reached)
        self.identity()
        evidence = self.case / "evidence"
        (evidence / (label + ".ansi")).write_bytes(self.raw)
        (evidence / (label + ".txt")).write_text(self.text)
        window = self.window()
        (evidence / (label + ".json")).write_text(json.dumps({"window": window, "pty_rows_columns_pixels": self.size}))
        shot = guard(self.case, evidence / (label + ".png"))
        if shot.exists():
            raise RuntimeError("refused stale TUI screenshot")
        self.drive("shot", shot, self.address)
        if not shot.is_file() or shot.stat().st_size == 0:
            raise RuntimeError("native screenshot command returned no new image")
        self.last_snapshot = (shot, window, self.size)
        print(f"TUI_SHOT {shot} sha256={hashlib.sha256(shot.read_bytes()).hexdigest()} inspection=pending", flush=True)

    def key(self, *args):
        self.identity()
        before = (self.case / "input.bin").stat().st_size
        self.drive("key", "--window", self.address, *args)
        self.wait("pty-input-" + str(self.checks), lambda: (self.case / "input.bin").stat().st_size > before)

    def chord(self, key, *modifiers):
        if modifiers == ("shift",) and len(key) == 1:
            self.identity()
            self.drive("focus", self.address)
            active = json.loads(command(["hyprctl", "activewindow", "-j"]))
            if active.get("address") != self.address or not self.owned_process(active.get("pid", 0)):
                raise RuntimeError("shifted terminal input requires the owned window to hold focus")
            before = (self.case / "input.bin").stat().st_size
            self.drive("hotkey", "--global", "shift", key.lower())
            self.wait("pty-shift-" + key, lambda: (self.case / "input.bin").stat().st_size > before)
            if (self.case / "input.bin").read_bytes()[before:] != key.upper().encode():
                raise RuntimeError("native Shift chord did not reach the PTY as the expected uppercase byte")
            return
        args = []
        for modifier in modifiers:
            args.extend(["-M", modifier])
        args.extend(["-k", key])
        for modifier in reversed(modifiers):
            args.extend(["-m", modifier])
        self.key(*args)

    def cursor_is(self, name):
        # The listing cursor is reverse video; marked rows use a background without reverse video.
        runs = re.findall(rb"\x1b\[7m([^\x1b]*)", self.raw)
        return any(name in run.decode("utf-8", errors="replace") and run.lstrip().startswith((b"\xe2\x80\xba ", b"\xe2\x96\xa1 ", b"@ ", b"* ")) for run in runs)

    def geometry(self, label, preview):
        lines = self.text.splitlines()
        row = lines[2]
        columns = self.size[1]
        borders = [index for index, char in enumerate(row) if char == "\u2502"]
        expected = [columns * 22 // 100 - 1]
        if preview:
            expected.append(columns * 22 // 100 + columns * 40 // 100 - 1)
        if len(row) != columns or borders != expected:
            raise RuntimeError(f"TUI {label} geometry differs from Tui.html 22:40:38: columns={columns}, row={len(row)}, borders={borders}, expected={expected}")
        if lines[1] != "\u2500" * columns or lines[-2] != "\u2500" * columns:
            raise RuntimeError("TUI header/footer separator is missing or clipped")
        if not (lines[0].startswith(" ") and lines[0].endswith(" ")
                and lines[-1].startswith(" ") and lines[-1].endswith(" ")):
            raise RuntimeError("TUI header/footer horizontal padding is missing")
        for pane in row.split("\u2502"):
            if len(pane) >= 7 and not (pane.startswith(" ") and pane.endswith(" ")):
                raise RuntimeError("TUI pane content touches its separator")
        self.checks += 1
        print(f"TUI_PASS {label} geometry columns={columns} borders={borders}", flush=True)

    def layout_pointer(self):
        middle = self.size[1] * 22 // 100 + 1
        self.click_cell(middle, 4, "padded-second-row")
        self.snapshot("padded-second-row", lambda text: self.cursor_is("bronze"))
        self.click_cell(middle, 2, "header-rule-inert")
        self.key("-k", "Down")
        self.snapshot("header-rule-inert", lambda text: self.cursor_is("charlie.txt"))
        self.click_cell(middle, self.size[0] - 1, "footer-rule-inert")
        self.key("-k", "Up")
        self.snapshot("footer-rule-inert", lambda text: self.cursor_is("bronze"))
        self.click_cell(middle, 3, "padded-first-row")
        self.snapshot("padded-first-row", lambda text: self.cursor_is("amber"))

    def spiral_geometry(self):
        rows, columns, pixels_x, pixels_y = self.size
        if min(pixels_x, pixels_y) <= 0:
            raise RuntimeError("native terminal has no measured cells for spiral aspect proof")
        marks = []
        for line in self.text.splitlines()[2:-2]:
            pane = line.split("\u2502")[1]
            cells = [index for index, char in enumerate(pane) if char in "\u2580\u2584\u2588"]
            if cells:
                marks.append(cells[-1] - cells[0] + 1)
        cell_x, cell_y = pixels_x // columns, pixels_y // rows
        if not marks or abs(max(marks) * cell_x * 10 - len(marks) * cell_y * 11) > cell_x * 10 / 2:
            raise RuntimeError("TUI spiral does not preserve its board aspect within one-half measured cell")
        self.checks += 1
        print(f"TUI_PASS spiral measured_cell={cell_x},{cell_y} mark_cells={max(marks)},{len(marks)}", flush=True)

    def navigate(self, path, label, expected):
        guard(self.case, path)
        self.chord("l", "ctrl")
        self.snapshot(label + "-editor", lambda text: ": " in text.splitlines()[-1])
        self.key(str(path))
        self.key("-k", "Return")
        self.snapshot(label, lambda text: path.name in text.splitlines()[0] and expected in text)

    def start(self):
        child_command = shlex.join(["bash", str(SCRIPT), "--child", str(self.case), str(self.binary)])
        invocation = ["xdg-terminal-exec", "--title=" + self.title, "--dir=" + str(self.case / "listing"), "--",
            "script", "--quiet", "--flush", "--return", "--log-in", str(self.case / "input.bin"),
            "--log-out", str(self.case / "output.bin"), "--log-timing", str(self.case / "timing.log"), "--command", child_command]
        self.log.write((shlex.join(invocation) + "\n").encode())
        terminal_environment = dict(self.environment, XDG_CONFIG_HOME=str(self.case / "config"))
        self.launcher = subprocess.Popen(invocation, env=terminal_environment, stdout=self.log, stderr=self.log, start_new_session=True)
        self.drive("wait", "window", self.title, "--timeout", str(WAIT_SECONDS))
        window = self.window()
        self.address = window["address"]
        # The user authorized native TUI input; enable only this verified terminal's class and target its unique address.
        self.environment["OMARCHY_DRIVE_TIER_FULL"] = window["class"]
        self.wait("product-pid", lambda: (self.case / "product.pid").is_file())
        window = self.identity()
        executable = Path(os.readlink(f'/proc/{window["pid"]}/exe')).name
        if executable != self.terminal:
            raise RuntimeError(f"xdg-terminal-exec selected {executable}, expected {self.terminal}")
        (self.case / "identity.json").write_text(json.dumps({"window": window, "product_pid": self.product_pid,
            "command": invocation, "terminal_exe": os.readlink(f'/proc/{window["pid"]}/exe')}))

    def smoke(self):
        self.start()
        self.snapshot("listing", lambda text: "5 items" in text and "charlie.txt" in text)
        self.geometry("listing", True)
        self.layout_pointer()
        self.key("-k", "Down")
        self.key("-k", "Right" if self.preset == "mac" else "Return")
        self.snapshot("empty-navigation", lambda text: "1 bronze" in text and "0 items" in text)
        self.spiral_geometry()
        self.key("-k", "BackSpace")
        self.snapshot("parent-navigation", lambda text: "1 listing" in text and "5 items" in text)
        self.key("-k", "Home")
        self.key("-k", "Right" if self.preset == "mac" else "Return")
        self.snapshot("nested-navigation", lambda text: "1 amber" in text and "nested-proof.txt" in text and "1 items" in text)
        self.key("-k", "BackSpace")
        self.snapshot("listing-restored", lambda text: "1 listing" in text and "5 items" in text)
        self.key("-k", "Home")
        self.key("v")
        self.snapshot("selection-one", lambda text: " V 1 " in text)
        self.key("-k", "Down")
        self.key("v")
        self.snapshot("selection-two", lambda text: " V 2 " in text and "2 items selected" in text)
        self.key("-k", "Escape")
        self.snapshot("selection-cleared", lambda text: " V " not in text and "5 items" in text)
        self.key("-M", "ctrl", "-k", "f", "-m", "ctrl")
        self.snapshot("search-editor", lambda text: "search:" in text and "Tab changes scope" in text)
        self.key("-k", "Tab")
        self.snapshot("search-current-scope", lambda text: "search:" in text and "in " + str(self.case / "listing") in text)
        self.key("needleproof")
        self.key("-k", "Return")
        self.snapshot("search-result", lambda text: "Search: 1 matches" in text and "delta-needleproof.txt" in text)
        self.search_identity()
        self.key("-k", "Escape")
        self.snapshot("search-dismissed", lambda text: "Search:" not in text and "5 items" in text)
        self.controls()
        self.tabs()
        self.previews()
        before = self.window()["size"]
        before_cells = self.terminal_size()[:2]
        self.drive("window", "fullscreen", self.address)
        self.wait("native-resize", lambda: self.window()["size"] != before and self.terminal_size()[:2] != before_cells)
        self.snapshot("resized", lambda text: "5 items" in text and "1 listing" in text)
        self.geometry("resized", True)
        self.quit()

    def quit(self):
        self.key("q")
        self.wait("clean-quit", lambda: (self.case / "exit.json").is_file())
        receipt = json.loads((self.case / "exit.json").read_text())
        if receipt["status"] != 0 or receipt["before"] != receipt["after"]:
            raise RuntimeError(f"TUI exit or terminal restoration failed: {receipt}")
        self.wait("terminal-closed", lambda: not self.owned_process(self.product_pid) and self.launcher.poll() is not None)

    def provider_calls(self, name):
        return [row for line in guard(self.case, self.case / "providers/calls.jsonl").read_text().splitlines()
                if (row := json.loads(line))["name"] == name]

    def providers(self):
        self.environment["FLEA_TUI_PROVIDER_BIN"] = provider_fixture(self.case)
        self.provider_gate = os.open(guard(self.case, self.case / "providers/status-release"), os.O_RDWR | os.O_NONBLOCK)
        source = guard(self.case, self.case / "listing/charlie.txt")
        original = source.read_bytes()
        sender = guard(self.case, self.case / "providers/bin/omarchy-tailscale-send")
        held_sender = guard(self.case, self.case / "providers/sender-held")
        taildrop_label = "taildrop  \u25b6"

        def status(name, peer="Alpha", backend="Running", status_exit=0, sender_exit=0, capability=True, gate=False, node=None, address=None, online=True):
            owner = {"UserID": 1, "Capabilities": ["https://tailscale.com/cap/file-sharing"] if capability else []}
            peers = {} if peer is None else {node or peer: {"HostName": peer, "DNSName": address or peer.lower() + ".fixture.invalid.",
                                                          "Online": online, "TaildropTarget": 1, "UserID": 1}}
            guard(self.case, self.case / "providers/control.json").write_text(json.dumps({"name": name,
                "status": {"BackendState": backend, "Self": owner, "Peer": peers},
                "statusExit": status_exit, "senderExit": sender_exit, "statusGate": gate}))

        def chosen(label):
            row = self.menu_entry(label)
            return row is not None and row["enabled"] and row["selected"]

        def peer_menu(label, peer="Alpha"):
            self.key("m")
            self.snapshot(label + "-eligible", lambda text: chosen("open")
                          and (row := self.menu_entry(taildrop_label)) is not None and row["enabled"])
            self.key("-k", "Down")
            self.key("-k", "Down")
            self.snapshot(label + "-taildrop-focused", lambda text: chosen(taildrop_label))
            self.key("-k", "Return")
            self.snapshot(label + "-peer-focused", lambda text: chosen(peer)
                          and self.menu_entry("Beta" if peer == "Alpha" else "Alpha") is None)

        def no_clipboard_or_source_change():
            assert not self.provider_calls("wl-copy") and not self.provider_calls("wl-paste"), "Taildrop touched the clipboard"
            assert source.read_bytes() == original, "Taildrop changed the source fixture"
            for call in self.provider_calls("omarchy-tailscale-send"):
                assert call["args"] == ["alpha.fixture.invalid", str(source)], call
                assert call["sources"] == [{"path": str(source), "sha256": hashlib.sha256(original).hexdigest()}], call

        status("initial")
        self.start()
        self.snapshot("providers-listing", lambda text: "5 items" in text and "charlie.txt" in text)
        expected = {"PATH": self.environment["FLEA_TUI_PROVIDER_BIN"], "HOME": str(self.case / "home"),
                    **{name: str(self.case / directory) for name, directory in [("XDG_STATE_HOME", "state"),
                       ("XDG_CONFIG_HOME", "config"), ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")]}}
        actual = dict(row.decode().split("=", 1) for row in Path(f"/proc/{self.product_pid}/environ").read_bytes().split(b"\0") if b"=" in row)
        terminal = dict(row.decode().split("=", 1) for row in Path(f'/proc/{self.window()["pid"]}/environ').read_bytes().split(b"\0") if b"=" in row)
        assert all(actual.get(name) == value for name, value in expected.items()), "product provider environment escaped its fixture"
        assert terminal["PATH"] == self.environment["PATH"] and terminal["HOME"] == os.environ["HOME"], "terminal startup environment changed"
        guard(self.case, self.case / "evidence/providers-environment.json").write_text(json.dumps({"product": expected,
            "terminal_path": terminal["PATH"], "terminal_home": terminal["HOME"], "real_sender_fallback": False}))
        self.key("-k", "Home")
        self.key("-k", "Down")
        self.key("-k", "Down")
        self.snapshot("providers-source", lambda text: self.cursor_is(source.name))

        for scenario, message, calls in [
            ("missing", "Taildrop: file or folder not found", 0),
            ("nonexecutable", "Taildrop: permission denied", 0),
            ("exit-failure", "Taildrop to Alpha failed (exit status: 7)", 1),
            ("success", "Sent to Alpha", 1),
        ]:
            status(scenario, sender_exit=7 if scenario == "exit-failure" else 0)
            before = len(self.provider_calls("omarchy-tailscale-send"))
            peer_menu("provider-" + scenario)
            # Change availability after an eligible peer is selected; PATH has no installed fallback.
            if scenario == "missing":
                guard(self.case, sender).rename(guard(self.case, held_sender))
            elif scenario == "nonexecutable":
                guard(self.case, sender).chmod(0o600)
            try:
                self.key("-k", "Return")
                self.snapshot("provider-" + scenario + "-result", lambda text: message in text and self.menu_entry("Alpha") is None)
            finally:
                if scenario == "missing":
                    guard(self.case, held_sender).rename(guard(self.case, sender))
                elif scenario == "nonexecutable":
                    guard(self.case, sender).chmod(0o700)
            dispatches = self.provider_calls("omarchy-tailscale-send")
            assert len(dispatches) == before + calls, (scenario, dispatches)
            no_clipboard_or_source_change()
            if scenario != "success":
                assert "(os error" not in self.text, self.text
                self.key("m")
                self.snapshot("provider-" + scenario + "-retained", lambda text: message in text and self.menu_entry("open") is not None)
                self.key("-k", "Escape")
                self.snapshot("provider-" + scenario + "-menu-dismissed", lambda text: message in text and self.menu_entry("open") is None)
                self.key("-k", "Escape")
                self.snapshot("provider-" + scenario + "-acknowledged", lambda text: message not in text and self.cursor_is(source.name))

        status_helper = guard(self.case, self.case / "providers/bin/tailscale")
        held_status = guard(self.case, self.case / "providers/status-held")
        for scenario, backend, peer, status_exit, capability, reason in [
            ("status-missing", "Running", "Alpha", 0, True, "Taildrop unavailable: tailscale could not start"),
            ("status-failed", "Running", "Alpha", 7, True, "Taildrop status failed"),
            ("signed-out-stale-peer", "NeedsLogin", "Alpha", 0, True, "signed out"),
            ("account-disabled-stale-peer", "Running", "Alpha", 0, False, "disabled for this account"),
            ("empty", "Running", None, 0, True, "no peers"),
        ]:
            status(scenario, peer, backend, status_exit, capability=capability)
            if scenario == "status-missing":
                guard(self.case, status_helper).rename(guard(self.case, held_status))
            self.key("m")
            if scenario != "status-missing":
                self.wait(scenario + "-status-exited", lambda: (rows := self.provider_calls("tailscale"))
                          and rows[-1]["state"] == scenario and not self.owned_process(rows[-1]["pid"]))
            self.snapshot("provider-" + scenario + "-disabled", lambda text: chosen("open")
                          and (row := self.menu_entry(taildrop_label)) is not None and not row["enabled"] and reason in row["text"])
            row = self.menu_entry(taildrop_label)
            self.click_cell(row["column"] + row["width"] // 2, row["row"], "provider-" + scenario)
            self.snapshot("provider-" + scenario + "-refused", lambda text: self.menu_entry(taildrop_label) is not None
                          and self.menu_entry("Alpha") is None)
            assert len(self.provider_calls("omarchy-tailscale-send")) == 2, "disabled provider reached sender"
            self.key("-k", "Escape")
            self.snapshot("provider-" + scenario + "-closed", lambda text: self.menu_entry(taildrop_label) is None and self.cursor_is(source.name))
            if scenario == "status-missing":
                guard(self.case, held_status).rename(guard(self.case, status_helper))
            no_clipboard_or_source_change()

        for scenario, changed, reason in [
            ("changed-node", {"node": "replacement-node"}, "Selected Taildrop device changed or is no longer reachable"),
            ("changed-address", {"address": "beta.fixture.invalid."}, "Selected Taildrop device changed or is no longer reachable"),
            ("peer-offline", {"online": False}, "no peers"),
            ("signed-out-on-activate", {"backend": "NeedsLogin"}, "signed out"),
            ("account-disabled-on-activate", {"capability": False}, "disabled for this account"),
        ]:
            status(scenario + "-before")
            peer_menu("provider-" + scenario)
            status(scenario, **changed)
            self.key("-k", "Return")
            self.snapshot("provider-" + scenario + "-refused", lambda text: reason in text and self.menu_entry("Alpha") is None)
            assert len(self.provider_calls("omarchy-tailscale-send")) == 2, "stale peer reached sender"
            self.key("-k", "Escape")
            self.snapshot("provider-" + scenario + "-acknowledged", lambda text: reason not in text and self.cursor_is(source.name))
            no_clipboard_or_source_change()

        for scenario in ("cancel-refresh", "source-changed-refresh", "repeat-refresh"):
            status(scenario + "-before")
            peer_menu("provider-" + scenario)
            status(scenario, gate=True)
            before = len(self.provider_calls("omarchy-tailscale-send"))
            self.key("-k", "Return")
            self.wait(scenario + "-status-started", lambda: (rows := self.provider_calls("tailscale")) and rows[-1]["state"] == scenario)
            status_pid = self.provider_calls("tailscale")[-1]["pid"]
            assert self.owned_process(status_pid), "status gate did not hold the real helper"
            self.snapshot("provider-" + scenario + "-pending", lambda text: "Checking Taildrop availability" in text)
            held_source = guard(self.case, self.case / "providers/source-held")
            try:
                if scenario == "cancel-refresh":
                    self.key("-k", "Escape")
                    self.snapshot("provider-cancel-refresh-dismissed", lambda text: "Checking Taildrop availability" not in text and self.cursor_is(source.name))
                elif scenario == "source-changed-refresh":
                    guard(self.case, source).rename(guard(self.case, held_source))
                    guard(self.case, source).write_bytes(b"replacement must never be sent\n")
                else:
                    self.key("-k", "Return")
                    self.key("-k", "Return")
                    assert len([row for row in self.provider_calls("tailscale") if row["state"] == scenario]) == 1, "repeated activation started another refresh"
                os.write(self.provider_gate, b"r")
                self.wait(scenario + "-status-exited", lambda: not self.owned_process(status_pid))
                if scenario == "repeat-refresh":
                    self.snapshot("provider-repeat-refresh-sent-once", lambda text: "Sent to Alpha" in text)
                    assert len(self.provider_calls("omarchy-tailscale-send")) == before + 1, "repeated activation sent more than once"
                elif scenario == "source-changed-refresh":
                    self.snapshot("provider-source-changed-refresh-refused", lambda text: "Taildrop cancelled: selection changed" in text or "Selected item changed" in text)
                    assert len(self.provider_calls("omarchy-tailscale-send")) == before, "changed source reached sender"
                else:
                    # A new native menu round-trip observes the late reply without mutating product state through IPC.
                    status("after-cancel")
                    peer_menu("provider-after-cancel")
                    assert len(self.provider_calls("omarchy-tailscale-send")) == before, "cancelled refresh sent after its late reply"
                    self.key("-k", "Escape")
                    self.snapshot("provider-after-cancel-parent", lambda text: chosen(taildrop_label))
                    self.key("-k", "Escape")
            finally:
                if held_source.exists():
                    guard(self.case, held_source).replace(guard(self.case, source))
            if scenario == "source-changed-refresh":
                self.key("-k", "Escape")
                self.key("-k", "Home")
                self.key("-k", "Down")
                self.key("-k", "Down")
                self.snapshot("provider-source-restored", lambda text: self.cursor_is(source.name) and "Selected item changed" not in text)
            no_clipboard_or_source_change()

        for peer in ("Alpha", "Beta"):
            status("refresh-" + peer, peer)
            peer_menu("provider-refresh-" + peer, peer)
            self.key("-k", "Escape")
            self.snapshot("provider-refresh-" + peer + "-parent", lambda text: chosen(taildrop_label))
            self.key("-k", "Escape")
            self.snapshot("provider-refresh-" + peer + "-closed", lambda text: self.menu_entry(taildrop_label) is None)
        assert len(self.provider_calls("omarchy-tailscale-send")) == 3, "peer inspection dispatched an unrequested send"
        no_clipboard_or_source_change()
        status("quit-before")
        peer_menu("provider-quit-refresh")
        status("quit-refresh", gate=True)
        self.key("-k", "Return")
        self.wait("quit-refresh-status-started", lambda: (rows := self.provider_calls("tailscale")) and rows[-1]["state"] == "quit-refresh")
        status_pid = self.provider_calls("tailscale")[-1]["pid"]
        self.snapshot("provider-quit-refresh-pending", lambda text: "Checking Taildrop availability" in text)
        self.quit()
        assert not self.owned_process(status_pid), "pending provider helper survived TUI exit"
        assert len(self.provider_calls("omarchy-tailscale-send")) == 3, "quitting dispatched the pending send"
        no_clipboard_or_source_change()
        print("TUI_PROVIDERS native-menu-peer-Return missing/nonexecutable/exit-failure/success exact-source-peer clipboard-untouched private-environment visible-refusal stale-status/peer/source-refusal cancel-late-result repeated-activation reopened-peer-refresh quit-pending=ok", flush=True)
        print("TUI_PROVIDERS_UNVERIFIED real-peer-delivery matched-pixels", flush=True)

    def search_identity(self):
        listing = self.case / "listing"
        original = guard(self.case, listing / "delta-needleproof.txt")
        renamed = guard(self.case, listing / "delta-needleproof-renamed.txt")
        contents = original.read_bytes()
        for source, destination, label in [(original, renamed, "search-rename"), (renamed, original, "search-restore")]:
            guard(self.case, source)
            guard(self.case, destination)
            self.key("r")
            self.snapshot(label + "-editor", lambda text: "Enter saves" in text)
            self.key(destination.stem)
            self.key("-k", "Return")
            self.wait(label + "-identity", lambda: not source.exists() and destination.is_file() and destination.read_bytes() == contents)
            self.snapshot(label, lambda text: "Search: 1 matches" in text and self.cursor_is(destination.name))

    def controls(self):
        for label, args, expected in [
            ("home", ("-k", "Home"), "amber"), ("down", ("-k", "Down"), "bronze"),
            ("j", ("j",), "charlie.txt"), ("k", ("k",), "bronze"),
            ("up", ("-k", "Up"), "amber"), ("end", ("-k", "End"), "echo.txt"),
            ("home-again", ("-k", "Home"), "amber"), ("G", ("G",), "echo.txt"),
            ("page-up", ("-k", "Page_Up"), "amber"), ("page-down", ("-k", "Page_Down"), "echo.txt"),
        ]:
            self.key(*args)
            self.snapshot("navigation-" + label, lambda text: self.cursor_is(expected))
        self.chord("u", "ctrl")
        self.snapshot("navigation-ctrl-u", lambda text: self.cursor_is("amber"))
        self.key("-k", "End")
        self.key("gg" if self.preset == "vim" else "g")
        self.snapshot("navigation-first-key", lambda text: self.cursor_is("amber"))
        for label, args, count in [("shift-down", ("Down", "shift"), 2), ("shift-up", ("Up", "shift"), 2)]:
            self.chord(*args)
            self.snapshot("selection-" + label, lambda text: f" V {count} " in text)
        self.key("-k", "Escape")
        self.chord("a", "ctrl")
        self.snapshot("selection-all-ctrl", lambda text: " V 5 " in text and "5 items selected" in text)
        self.key("-k", "Escape")
        if self.preset == "mac":
            self.chord("a", "super")
            self.snapshot("selection-all-super", lambda text: " V 5 " in text)
            self.key("-k", "Escape")
        self.key("/")
        self.snapshot("filter-editor", lambda text: "filter:" in text)
        self.key("charlie")
        self.snapshot("filter-live", lambda text: "filter: charlie" in text.splitlines()[-1]
                      and "4 rows hidden by the filter" in text and self.cursor_is("charlie.txt"))
        self.key("-k", "Return")
        self.snapshot("filter-accepted", lambda text: "filter:" not in text and "1 matches in 5 loaded rows" in text)
        self.key("-k", "Escape")
        self.snapshot("filter-cleared", lambda text: "Filter " not in text and "5 items" in text)
        for sort in ("size", "date", "kind", "name"):
            self.key("s")
            self.snapshot("sort-" + sort, lambda text: "\u00b7 " + sort + " \u25b4" in text.splitlines()[0]
                          and self.cursor_is("charlie.txt") and "Could not save settings" not in text)
            self.wait("sort-" + sort + "-persisted", lambda: json.loads((self.case / "state/flea/ui.json").read_text())["sort"]
                      == {"key": sort, "reverse": False})
        for label, glyph in [("reverse", "\u25be"), ("forward", "\u25b4")]:
            self.chord("S", "shift")
            self.snapshot("sort-" + label, lambda text: "name " + glyph in text.splitlines()[0]
                          and self.cursor_is("charlie.txt") and "Could not save settings" not in text)
            self.wait("sort-" + label + "-persisted", lambda: json.loads((self.case / "state/flea/ui.json").read_text())["sort"]
                      == {"key": "name", "reverse": label == "reverse"})
        self.key(".")
        self.snapshot("hidden-shown", lambda text: ".hidden-proof" in text and "6 items" in text and self.cursor_is("charlie.txt"))
        self.key(".")
        self.snapshot("hidden-restored", lambda text: ".hidden-proof" not in text and "5 items" in text and self.cursor_is("charlie.txt"))
        self.key("-k", "Home")
        self.snapshot("rename-target-ready", lambda text: self.cursor_is("amber"))
        for label, args in [("r", ("r",)), ("f2", ("-k", "F2"))] + ([("enter", ("-k", "Return"))] if self.preset == "mac" else []):
            self.key(*args)
            self.snapshot("rename-" + label, lambda text: "Enter saves" in text)
            self.key("-k", "Escape")
            self.snapshot("rename-" + label + "-cancelled", lambda text: "Enter saves" not in text and self.cursor_is("amber"))
        self.key(":")
        self.key("am")
        self.snapshot("path-completion", lambda text: text.splitlines()[-1].startswith(" : am\u258fber/"))
        accent = re.search(rb"(\x1b\[38;2;\d+;\d+;\d+m)1 listing", self.raw)
        if not accent or accent[1] + b": " not in self.raw or accent[1] + "\u258f".encode() not in self.raw:
            raise RuntimeError("TUI path prompt/caret does not use the active-tab accent")
        self.checks += 1
        print("TUI_PASS path prompt and caret share accent role", flush=True)
        self.key("-k", "Tab")
        self.key("-k", "Return")
        self.snapshot("path-completion-opened", lambda text: "1 amber" in text and "1 items" in text)
        history = {"default": ("H", "L"), "vim": ("H", "L"),
                   "mac": (("bracketleft", "super"), ("bracketright", "super")),
                   "windows": (("Left", "alt"), ("Right", "alt"))}[self.preset]
        for label, action, directory, count in [("back", history[0], "listing", 5), ("forward", history[1], "amber", 1)]:
            self.key(action) if isinstance(action, str) else self.chord(*action)
            self.snapshot("history-" + label, lambda text: "1 " + directory in text and f"{count} items" in text)
        self.key("-k", "BackSpace")
        self.snapshot("history-returned", lambda text: "1 listing" in text and "5 items" in text)
        self.chord("l", "ctrl")
        self.key(str(guard(self.case, self.case / "missing-directory")))
        self.key("-k", "Return")
        self.snapshot("missing-path-error", lambda text: "Esc dismisses" in text and "5 items" in text and self.cursor_is("amber"))
        self.key("s")
        self.snapshot("missing-path-persists", lambda text: "Esc dismisses" in text and "\u00b7 size" in text.splitlines()[0]
                      and "charlie.txt" in text and self.cursor_is("amber"))
        self.key("-k", "Escape")
        self.snapshot("missing-path-acknowledged", lambda text: "Esc dismisses" not in text)
        self.key("sss")
        self.snapshot("sort-restored-after-error", lambda text: "\u00b7 name" in text.splitlines()[0] and self.cursor_is("amber"))
        self.rename_changed_identity()
        self.menu_and_panel()

    def rename_changed_identity(self):
        original = guard(self.case, self.case / "listing/charlie.txt")
        held = guard(self.case, self.case / "charlie-held.txt")
        destination = guard(self.case, self.case / "listing/changed-name.txt")
        contents = original.read_bytes()
        identity = original.lstat()
        self.key("-k", "Home")
        self.key("-k", "Down")
        self.key("-k", "Down")
        self.snapshot("rename-identity-selected", lambda text: self.cursor_is(original.name))
        self.key("r")
        self.snapshot("rename-identity-editor", lambda text: "Enter saves" in text)
        original.rename(held)
        original.write_text("external replacement fixture\n")
        self.key(destination.stem)
        self.key("-k", "Return")
        self.snapshot("rename-changed-identity-refused", lambda text: "Selected item changed; reopen Rename." in text)
        if destination.exists() or held.read_bytes() != contents or original.read_text() != "external replacement fixture\n":
            raise RuntimeError("Rename changed an item after its displayed identity was replaced")
        self.key("-k", "Escape")
        guard(self.case, held).replace(guard(self.case, original))
        restored = original.lstat()
        if (restored.st_dev, restored.st_ino, restored.st_mode) != (identity.st_dev, identity.st_ino, identity.st_mode) \
                or original.read_bytes() != contents:
            raise RuntimeError("Rename fixture did not restore the original item identity and bytes")
        self.snapshot("rename-identity-recovered", lambda text: "5 items" in text and "Selected item changed" not in text
                      and self.cursor_is(original.name) and held.name not in text
                      and any(original.name in middle and middle.rstrip().endswith(f" {len(contents)} B")
                              for middle in (line.split("\u2502")[1] for line in text.splitlines() if "\u2502" in line)))

    def menu_and_panel(self):
        for label, activate in [("letter", lambda: self.key("m")), ("shift-f10", lambda: self.chord("F10", "shift")),
                                ("menu-key", lambda: self.key("-k", "Menu"))]:
            hidden_label = "hide hidden" if label == "shift-f10" else "show hidden"
            activate()
            self.snapshot("menu-" + label, lambda text: hidden_label in text and "taildrop" in text)
            self.key("r")
            self.snapshot("menu-" + label + "-contained", lambda text: hidden_label in text and "Enter saves" not in text)
            self.key("-k", "Tab")
            self.key("-k", "Return")
            expected = 6 if label != "shift-f10" else 5
            self.snapshot("menu-" + label + "-activated", lambda text: hidden_label not in text and f"{expected} items" in text)
        self.key(".")
        self.snapshot("menu-hidden-restored", lambda text: "5 items" in text and ".hidden-proof" not in text)
        self.tiny_menu()
        self.key("?")
        self.snapshot("keymap-panel", lambda text: "\u2500 keys " in text and "open" in text)
        self.key("r")
        self.snapshot("keymap-panel-contained", lambda text: "\u2500 keys " in text and "Enter saves" not in text)
        for label, args in [("down", ("-k", "Down")), ("j", ("j",)), ("up", ("-k", "Up")), ("k", ("k",))]:
            before = self.raw
            self.key(*args)
            self.snapshot("keymap-scroll-" + label, lambda text: "\u2500 keys " in text and self.raw != before)
        self.key("-k", "Escape")
        self.snapshot("keymap-dismissed", lambda text: "\u2500 keys " not in text and "5 items" in text)

    def cell_geometry(self):
        window = self.identity()
        rows, columns, pixels_x, pixels_y = self.terminal_size()
        monitors = json.loads(command(["hyprctl", "monitors", "-j"]))
        scale = next(item["scale"] for item in monitors if item["id"] == window["monitor"])
        if min(rows, columns, pixels_x, pixels_y, scale) <= 0:
            raise RuntimeError("native terminal did not report usable cell geometry")
        return window, (rows, columns), (pixels_x / columns / scale, pixels_y / rows / scale)

    def resize_window(self, width, height, label):
        window = self.identity()
        if not re.fullmatch(r"0x[0-9a-fA-F]+", self.address) or min(width, height) <= 0:
            raise RuntimeError("native TUI resize has an invalid owned address or extent")
        if not window["floating"]:
            self.drive("window", "float", self.address)
        dispatch = f'hl.dsp.window.resize({{ x = {width}, y = {height}, exact = true, window = "address:{self.address}" }})'
        result = command(["hyprctl", "dispatch", dispatch]).decode().strip()
        self.log.write((dispatch + "\n" + result + "\n").encode())
        if not result.startswith("ok"):
            raise RuntimeError("compositor refused native TUI resize: " + result)
        self.drive("window", "center", self.address)
        self.wait(label, lambda: self.identity()["size"] == [width, height])

    def menu_entry(self, label):
        # Native menu row: ESC[row;columnH + SGR colours + box border, padded label and box border.
        chunks = re.split(rb"\x1b\[(\d+);(\d+)H", self.raw)
        for index in range(1, len(chunks), 3):
            run = chunks[index + 2]
            text = ESCAPE.sub(b"", run).decode("utf-8", errors="replace")
            content = text[1:-1].strip()
            if text.startswith("\u2502") and text.endswith("\u2502") and (content == label
                    or label == "taildrop  \u25b6" and content.startswith("taildrop \u00b7 ")):
                prefix = run.split(content.encode(), 1)[0]
                colours = re.findall(rb"\x1b\[38;2;[0-9;]+m", prefix)
                return {"row": int(chunks[index]), "column": int(chunks[index + 1]), "width": len(text),
                        "selected": b"\x1b[7m" in prefix, "enabled": len(colours) == 1, "text": text}
        return None

    def click_cell(self, column, row, label):
        window, (rows, columns), (cell_x, cell_y) = self.cell_geometry()
        if not (1 <= column <= columns and 1 <= row <= rows):
            raise RuntimeError("native pointer target is outside the measured terminal grid")
        self.drive("focus", self.address)

        def owned_target(x, y):
            active = json.loads(command(["hyprctl", "activewindow", "-j"]))
            current = self.identity()
            if active.get("address") != self.address or current["at"] != window["at"] or current["size"] != window["size"] \
                    or not (window["at"][0] <= x < window["at"][0] + window["size"][0]
                            and window["at"][1] <= y < window["at"][1] + window["size"][1]):
                raise RuntimeError("native pointer target moved or lost focus before activation")

        # Intersect every possible padding placement of the measured grid's header and separator rows.
        inert_rows = 2
        padding_x, padding_y = window["size"][0] - columns * cell_x, window["size"][1] - rows * cell_y
        anchor_x = round(window["at"][0] + window["size"][0] / 2)
        anchor_y = round(window["at"][1] + (padding_y + inert_rows * cell_y) / 2)
        if min(padding_x, padding_y) < 0:
            raise RuntimeError("measured terminal grid exceeds its native window")
        image_anchor = not (window["at"][0] + padding_x <= anchor_x < window["at"][0] + columns * cell_x
                            and window["at"][1] + padding_y <= anchor_y < window["at"][1] + inert_rows * cell_y)
        geometry = (tuple(window["size"]), window["monitor"], rows, columns, cell_x, cell_y)
        if image_anchor:
            local_anchor = self.separator_anchors.get(geometry)
            if local_anchor is None:
                shot, captured_window, captured_size = self.last_snapshot
                if any(captured_window[key] != window[key] for key in ("address", "size", "monitor")) \
                        or captured_size != self.terminal_size():
                    raise RuntimeError("terminal separator capture does not match the current owned geometry")
                dimensions, pixel = separator_image(guard(self.case, shot), captured_size)
                scale_x, scale_y = dimensions[0] / window["size"][0], dimensions[1] / window["size"][1]
                if scale_x != scale_y or not math.isclose(scale_x * columns * cell_x, captured_size[2]):
                    raise RuntimeError("terminal screenshot scale differs from the measured grid")
                local_anchor = (round(pixel[0] / scale_x), round(pixel[1] / scale_y))
                print(f"TUI_SEPARATOR shot={shot} image_size={dimensions} pixel={pixel} window_local={local_anchor}", flush=True)
            anchor_x, anchor_y = window["at"][0] + local_anchor[0], window["at"][1] + local_anchor[1]
        owned_target(anchor_x, anchor_y)
        before = STRING_CONTROL.sub(b"", frame((self.case / "output.bin").read_bytes(), rows, columns)[0])
        if not before:
            raise RuntimeError("native pointer calibration requires a complete rendered frame")
        offset = (self.case / "input.bin").stat().st_size
        # SGR middle press/release: ESC[<1;column;rowM/m, reported under the product's normal 1002 mode.
        reports = lambda: re.findall(rb"\x1b\[<(\d+);(\d+);(\d+)([Mm])", (self.case / "input.bin").read_bytes()[offset:])
        anchors = lambda: [report for report in reports() if report[0] == b"1" and report[3] == b"M"]
        self.drive("click", anchor_x, anchor_y, "middle")
        self.wait(label + "-pointer-anchor", lambda: bool(anchors()))
        anchor = anchors()[-1]
        if not (1 <= int(anchor[1]) <= columns and (int(anchor[2]) == 2 if image_anchor else 1 <= int(anchor[2]) <= inert_rows)):
            raise RuntimeError("native middle-button calibration escaped the inert chrome cells")
        self.wait(label + "-pointer-release", lambda: (b"1", anchor[1], anchor[2], b"m") in reports())
        owned_target(anchor_x, anchor_y)
        self.wait(label + "-pointer-inert", lambda: STRING_CONTROL.sub(b"", frame(
            (self.case / "output.bin").read_bytes(), rows, columns)[0]) == before)
        cursor = json.loads(command(["hyprctl", "cursorpos", "-j"]))
        if cursor["x"] != anchor_x or cursor["y"] != anchor_y:
            raise RuntimeError("native pointer moved during chrome calibration")
        if image_anchor:
            self.separator_anchors[geometry] = local_anchor
        target_x = round(anchor_x + (column - int(anchor[1])) * cell_x)
        target_y = round(anchor_y + (row - int(anchor[2])) * cell_y)
        owned_target(target_x, target_y)
        offset = (self.case / "input.bin").stat().st_size
        self.drive("click", target_x, target_y, "left")
        self.wait(label + "-pointer-press", lambda: any(int(button) == 0 and int(x) == column and int(y) == row and kind == b"M"
                  for button, x, y, kind in reports()))
        print(f"TUI_POINTER {label} cell={column},{row} pixel={target_x},{target_y} measured_cell={cell_x},{cell_y}", flush=True)

    def tiny_menu(self):
        original, cells, (_, cell_y) = self.cell_geometry()
        self.key("m")
        peer_ready = True
        try:
            self.snapshot("tiny-menu-peer-ready", lambda text: (entry := self.menu_entry("taildrop  \u25b6")) is not None and entry["enabled"])
        except RuntimeError as error:
            if str(error) != f"native TUI did not reach tiny-menu-peer-ready within {WAIT_SECONDS}s":
                raise
            self.snapshot("tiny-menu-peer-disabled", lambda text: (entry := self.menu_entry("taildrop  \u25b6")) is not None
                          and not entry["enabled"] and (opened := self.menu_entry("open")) is not None and opened["enabled"])
            peer_ready = False
            gap = {"requirement": "Tui.html: six-row root menu scrolls to eligible Taildrop", "state": "undriven",
                   "reason": f"Taildrop was still disabled after a {WAIT_SECONDS}s readiness wait; Open was enabled",
                   "evidence": str(self.case / "evidence/tiny-menu-peer-disabled.json")}
            self.undriven.append(gap)
            guard(self.case, self.case / "evidence/undriven.json").write_text(json.dumps(self.undriven))
            print("TUI_UNDRIVEN " + json.dumps(gap), flush=True)
        try:
            # Calibrate the new grid while its separator is unobscured, before reopening the menu.
            self.key("-k", "Escape")
            self.resize_window(original["size"][0], round(original["size"][1] + (6 - cells[0]) * cell_y), "tiny-menu-window")
            self.wait("tiny-menu-six-rows", lambda: self.terminal_size()[:2] == (6, cells[1]))
            self.snapshot("tiny-menu-chrome", lambda text: "5 items" in text and self.menu_entry("show hidden") is None)
            self.click_cell(1, 2, "tiny-menu-chrome-calibration")
            self.key("m")
            self.snapshot("tiny-menu-open", lambda text: (entry := self.menu_entry("open")) is not None and entry["selected"]
                          and self.menu_entry("show hidden") is not None)
            self.key("-k", "Down")
            if peer_ready:
                self.key("-k", "Down")
                self.snapshot("tiny-menu-taildrop-selected", lambda text: (entry := self.menu_entry("taildrop  \u25b6")) is not None
                              and entry["selected"] and self.menu_entry("open") is None and self.menu_entry("show hidden") is not None)
            else:
                self.snapshot("tiny-menu-hidden-selected", lambda text: (entry := self.menu_entry("show hidden")) is not None
                              and entry["selected"] and self.menu_entry("open") is not None)
            entry = self.menu_entry("show hidden")
            self.click_cell(entry["column"] + entry["width"] - 2, entry["row"], "tiny-menu-right-padding")
            self.snapshot("tiny-menu-pointer-show", lambda text: self.menu_entry("show hidden") is None and "6 items" in text)
            self.key("m")
            self.snapshot("tiny-menu-hide-label", lambda text: self.menu_entry("hide hidden") is not None)
            entry = self.menu_entry("hide hidden")
            self.click_cell(entry["column"], entry["row"], "tiny-menu-left-border")
            self.snapshot("tiny-menu-border-dismissed", lambda text: self.menu_entry("hide hidden") is None and "6 items" in text)
            self.key("m")
            self.snapshot("tiny-menu-hide-reopened", lambda text: self.menu_entry("hide hidden") is not None)
            entry = self.menu_entry("hide hidden")
            self.click_cell(entry["column"] + 1, entry["row"], "tiny-menu-left-padding")
            self.snapshot("tiny-menu-pointer-hide", lambda text: self.menu_entry("hide hidden") is None and "5 items" in text)
        finally:
            self.resize_window(*original["size"], "tiny-menu-window-restored")
            if not original["floating"]:
                self.drive("window", "float", self.address)
            self.wait("tiny-menu-cells-restored", lambda: self.terminal_size()[:2] == cells)
        self.snapshot("tiny-menu-restored", lambda text: "5 items" in text and self.menu_entry("hide hidden") is None)

    def tabs(self):
        paths = []
        for index in range(1, 10):
            path = guard(self.case, self.case / ("t" + str(index)))
            path.mkdir()
            guard(self.case, path / ("tab-" + str(index) + ".txt")).write_text("independent tab fixture\n")
            paths.append(path)
            if index > 1:
                self.chord("t", "ctrl") if self.preset in ("mac", "windows") else self.key("t")
                self.snapshot("tab-new-" + str(index), lambda text: f"{index} t{index - 1}" in text.splitlines()[0])
            self.navigate(path, "tab-path-" + str(index), "1 items")
        for index in range(1, 10):
            self.key(str(index))
            self.snapshot("tab-direct-" + str(index), lambda text: "tab-" + str(index) + ".txt" in text and f"/{paths[index - 1].name} \u00b7" in text.splitlines()[0])
        self.chord("Page_Down", "ctrl")
        self.snapshot("tab-next-wrap", lambda text: "tab-1.txt" in text)
        self.chord("Page_Up", "ctrl")
        self.snapshot("tab-previous-wrap", lambda text: "tab-9.txt" in text)
        self.key("-k", "space")
        self.snapshot("tab-quicklook-open", lambda text: "\u2502" not in text and "independent tab fixture" in text)
        header = self.raw.split(b"\x1b[2;1H", 1)[0]
        column = self.text.splitlines()[0].index("1 t1") + 1
        next_line = self.text.splitlines()[3]
        if next_line == self.text.splitlines()[2]:
            raise RuntimeError("Quick Look fixture cannot demonstrate the preview scroll barrier")
        self.click_cell(column, 1, "tab-quicklook-header-inert")
        self.key("-k", "Down")
        self.snapshot("tab-quicklook-header-inert", lambda text: "\u2502" not in text
                      and text.splitlines()[2] == next_line and self.raw.split(b"\x1b[2;1H", 1)[0] == header)
        self.key("-k", "Escape")
        self.snapshot("tab-quicklook-closed", lambda text: "\u2502" in text and self.cursor_is("tab-9.txt"))
        self.click_cell(column, 1, "tab-header-active-after-close")
        self.snapshot("tab-header-active-after-close", lambda text: self.cursor_is("tab-1.txt"))
        self.key("9")
        self.snapshot("tab-header-restored", lambda text: self.cursor_is("tab-9.txt"))
        for index in range(9, 1, -1):
            self.chord("w", "ctrl")
            self.snapshot("tab-close-" + str(index), lambda text: "tab-" + str(index - 1) + ".txt" in text)
        self.navigate(self.case / "listing", "tabs-returned", "5 items")

    def media_fixtures(self):
        directory = guard(self.case, self.case / "media")
        directory.mkdir()
        guard(self.case, directory / "01-text.txt").write_text("\n".join(f"TEXT-PROOF-{index:03d}" for index in range(300)))
        page_one = b"BT /F1 20 Tf 20 100 Td (PDF PAGE ONE) Tj ET"
        page_two = b"BT /F1 20 Tf 20 100 Td (PDF PAGE TWO) Tj ET"
        objects = [b"<< /Type /Catalog /Pages 2 0 R >>", b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>"]
        for stream in (5, 6):
            objects.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 160] /Resources << /Font << /F1 7 0 R >> >> /Contents {stream} 0 R >>".encode())
        for content in (page_one, page_two):
            objects.append(b"<< /Length " + str(len(content)).encode() + b" >>\nstream\n" + content + b"\nendstream")
        objects.append(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        pdf, offsets = bytearray(b"%PDF-1.4\n"), [0]
        for index, content in enumerate(objects, 1):
            offsets.append(len(pdf))
            pdf.extend(f"{index} 0 obj\n".encode() + content + b"\nendobj\n")
        xref = len(pdf)
        pdf.extend(f"xref\n0 {len(offsets)}\n0000000000 65535 f \n".encode())
        for offset in offsets[1:]:
            pdf.extend(f"{offset:010d} 00000 n \n".encode())
        pdf.extend(f"trailer\n<< /Size {len(offsets)} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n".encode())
        guard(self.case, directory / "02-pages.pdf").write_bytes(pdf)
        with wave.open(str(guard(self.case, directory / "03-audio.wav")), "wb") as audio:
            audio.setparams((1, 2, 8000, 0, "NONE", "not compressed"))
            audio.writeframes(bytes(12 * 8000 * 2))
        video = guard(self.case, directory / "04-video.mp4")
        command(["ffmpeg", "-nostdin", "-v", "error", "-f", "lavfi", "-i", "testsrc=size=96x64:rate=2",
                 "-t", "12", "-c:v", "mpeg4", "-pix_fmt", "yuv420p", video])
        return directory

    def graphics(self, label, offset):
        def received():
            output = (self.case / "output.bin").read_bytes()[offset:]
            return b"\x1b_Ga=T," in output if self.terminal == "kitty" else re.search(rb"\x1bP[0-9;]*q", output)
        self.wait(label + "-graphics-output", received)

    def previews(self):
        directory = self.media_fixtures()
        self.navigate(directory, "preview-fixtures", "4 items")
        self.key("-k", "Home")
        self.snapshot("preview-manual-unloaded", lambda text: self.cursor_is("01-text.txt") and "Ctrl+Space to load preview" in text and "TEXT-PROOF-000" not in text)
        self.chord("space", "ctrl")
        self.snapshot("preview-manual-loaded", lambda text: "TEXT-PROOF-000" in text and "Ctrl+Space to load preview" not in text)
        self.chord("Tab", "ctrl")
        self.key("r")
        self.key("t")
        self.key("v")
        self.snapshot("preview-context-contained", lambda text: self.cursor_is("01-text.txt") and "Enter saves" not in text and " V " not in text and "2 media" not in text)
        before = self.raw
        self.key("-k", "Down")
        self.snapshot("preview-scrolled", lambda text: self.cursor_is("01-text.txt") and self.raw != before)
        self.key("-k", "Escape")
        self.key("-k", "Down")
        self.snapshot("preview-focus-restored", lambda text: self.cursor_is("02-pages.pdf") and "Ctrl+Space to load preview" in text)
        self.chord("p", "alt")
        self.snapshot("preview-column-hidden", lambda text: "Ctrl+Space to load preview" not in text)
        self.geometry("preview-column-hidden", False)
        self.chord("p", "alt")
        self.snapshot("preview-column-restored", lambda text: "Ctrl+Space to load preview" in text)
        self.geometry("preview-column-restored", True)
        offset = (self.case / "output.bin").stat().st_size
        self.chord("space", "ctrl")
        self.snapshot("pdf-loaded", lambda text: "Page 1 / 2" in text and "100%" in text)
        self.graphics("pdf-loaded", offset)
        self.chord("Tab", "ctrl")
        self.pdf_controls()
        self.key("-k", "Escape")
        for name in ("03-audio.wav", "04-video.mp4"):
            self.key("-k", "Down")
            self.snapshot(name + "-unloaded", lambda text: self.cursor_is(name) and "Ctrl+Space to load preview" in text)
            offset = (self.case / "output.bin").stat().st_size
            self.chord("space", "ctrl")
            self.snapshot(name + "-loaded", lambda text: "Play" in text and "0:00 / 0:12" in text)
            if name.endswith("mp4"):
                self.graphics("video-loaded", offset)
            self.chord("Tab", "ctrl")
            self.media_controls(name)
        self.preview_failures(directory)
        self.navigate(self.case / "listing", "previews-returned", "5 items")

    def preview_failures(self, media):
        directory = guard(self.case, self.case / "preview-failures")
        directory.mkdir()
        image = guard(self.case, directory / "01-image.png")
        command(["magick", "-size", "16x16", "xc:white", image])
        for source, name in [("02-pages.pdf", "02-pages.pdf"), ("03-audio.wav", "03-audio.wav")]:
            shutil.copyfile(guard(self.case, media / source), guard(self.case, directory / name))
        for index, name in enumerate(["01-image.png", "02-pages.pdf", "03-audio.wav"]):
            path = guard(self.case, directory / name)
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            path.chmod(0)
            try:
                try:
                    path.read_bytes()
                except PermissionError:
                    pass
                else:
                    raise RuntimeError("unreadable preview fixture still permits reads")
                label = "preview-denied-" + path.stem
                self.navigate(directory, label + "-listing", "3 items")
                self.key("-k", "Home")
                for _ in range(index):
                    self.key("-k", "Down")
                self.snapshot(label + "-unloaded", lambda text: self.cursor_is(name) and "Ctrl+Space to load preview" in text)
                self.chord("space", "ctrl")
                self.snapshot(label, lambda text: re.search("permission denied", text, re.IGNORECASE) and "Esc dismisses" in text
                              and (index != 1 or "Page 1 / ?" in text))
                expected = ("Image preview: " if index == 0 else "Media preview: " if index == 2 else "") + "permission denied"
                if expected not in self.text or "(os error " in self.text:
                    raise RuntimeError(f"preview error copy is not plain words: {self.text.splitlines()[-1]}")
                self.chord("Tab", "ctrl")
                self.chord("Tab", "ctrl")
                self.snapshot(label + "-persists", lambda text: expected in text and "Esc dismisses" in text and "(os error " not in text)
                self.key("-k", "Escape")
                self.snapshot(label + "-acknowledged", lambda text: "Esc dismisses" not in text)
            finally:
                guard(self.case, path).chmod(0o600)
            if hashlib.sha256(path.read_bytes()).hexdigest() != digest:
                raise RuntimeError("preview refusal changed fixture bytes")
            self.navigate(self.case / "listing", label + "-left", "5 items")
            self.navigate(directory, label + "-reopened", "3 items")
            self.key("-k", "Home")
            for _ in range(index):
                self.key("-k", "Down")
            self.snapshot(label + "-recovery-unloaded", lambda text: self.cursor_is(name) and "Ctrl+Space to load preview" in text)
            offset = (self.case / "output.bin").stat().st_size
            self.chord("space", "ctrl")
            def recovered(text):
                return self.cursor_is(name) and "Ctrl+Space to load preview" not in text and "Esc dismisses" not in text \
                    and (index != 1 or "Page 1 / 2" in text) and (index != 2 or "Play" in text and "0:00 / 0:12" in text)
            self.snapshot(label + "-recovered", recovered)
            if index != 2:
                self.graphics(label + "-recovered", offset)
            self.navigate(self.case / "listing", label + "-finished", "5 items")

    def pdf_controls(self):
        controls = ("\u2039", "\u203a", "\u2212", "+", "\u2197")
        for direction, sequence in [("next", range(1, 6)), ("previous", range(4, -1, -1))]:
            for index in sequence:
                self.chord("Tab", "shift") if direction == "previous" else self.key("-k", "Tab")
                self.snapshot(f"pdf-focus-{direction}-{index}", lambda text: "[" + controls[index % 5] + "]" in text)
        self.key("-k", "Tab")
        self.key("-k", "Return")
        self.snapshot("pdf-next-enter", lambda text: "Page 2 / 2" in text)
        self.key("-k", "space")
        self.snapshot("pdf-last-page-boundary", lambda text: "Page 2 / 2" in text)
        self.key("-k", "Left")
        self.snapshot("pdf-previous-arrow", lambda text: "Page 1 / 2" in text)
        self.key("l")
        self.snapshot("pdf-next-l", lambda text: "Page 2 / 2" in text)
        self.key("h")
        self.snapshot("pdf-previous-h", lambda text: "Page 1 / 2" in text)
        self.key("-k", "Tab")
        self.key("-k", "space")
        self.snapshot("pdf-zoom-out-space", lambda text: "75%" in text)
        self.key("-k", "Tab")
        self.key("-k", "Return")
        self.snapshot("pdf-zoom-in-enter", lambda text: "100%" in text)
        self.key("-k", "minus")
        self.snapshot("pdf-zoom-out-minus", lambda text: "75%" in text)
        self.key("+")
        self.snapshot("pdf-zoom-in-plus", lambda text: "100%" in text)
        self.key("r")
        self.key("v")
        self.snapshot("pdf-listing-keys-contained", lambda text: "Page 1 / 2" in text and "Enter saves" not in text and " V " not in text)
        self.key("-k", "Escape")
        self.key("-k", "space")
        self.snapshot("pdf-quicklook", lambda text: "Page 1 / 2" in text and "\u00d7" in text and "\u2502" not in text)
        quick_controls = controls + ("\u00d7",)
        current = next(index for index, control in enumerate(quick_controls) if "[" + control + "]" in self.text)
        for direction in (1, -1):
            for step in range(1, 7):
                self.chord("Tab", "shift") if direction < 0 else self.key("-k", "Tab")
                target = quick_controls[(current + step * direction) % 6]
                self.snapshot(f"pdf-quicklook-cycle-{direction}-{step}", lambda text: "[" + target + "]" in text)
        for step in range(6):
            if "[\u00d7]" in self.text:
                break
            self.key("-k", "Tab")
            self.snapshot("pdf-close-focus-" + str(step), lambda text: "\u2502" not in text)
        if "[\u00d7]" not in self.text:
            raise RuntimeError("PDF Close was not reachable within one control cycle")
        self.key("-k", "space")
        self.snapshot("pdf-close-space", lambda text: "\u2502" in text and self.cursor_is("02-pages.pdf"))
        self.chord("Tab", "ctrl")
        self.snapshot("pdf-inline-focus-after-close", lambda text: "[\u2039]" in text)
        self.key("-k", "Escape")
        self.key("-k", "space")
        self.snapshot("pdf-quicklook-reopened", lambda text: "\u2502" not in text and "Page 1 / 2" in text)
        self.chord("Tab", "shift")
        self.key("-k", "Return")
        self.snapshot("pdf-close-enter", lambda text: "\u2502" in text and self.cursor_is("02-pages.pdf"))
        self.pdf_narrow_controls(controls)

    def pdf_narrow_controls(self, controls):
        original, cells, (cell_x, _) = self.cell_geometry()
        inline_width = len("  ".join(controls)) + 2
        inline_columns = max(columns for columns in range(12, cells[1])
                             if columns - columns * 22 // 100 - columns * 40 // 100 - 2 < inline_width)
        quick_columns = len("  ".join(controls + ("\u00d7",))) + 3

        def resize(columns, label):
            self.resize_window(round(original["size"][0] + (columns - cells[1]) * cell_x), original["size"][1], label)
            self.wait(label + "-cells", lambda: self.terminal_size()[:2] == (cells[0], columns))

        def toolbar(quicklook):
            line = self.text.splitlines()[self.size[0] - 4]
            if len(line) != self.size[1]:
                raise RuntimeError("PDF toolbar row escaped the measured terminal columns")
            pane = line if quicklook else line.rsplit("\u2502", 1)[-1]
            return pane[1:-1] if len(pane) >= 7 else pane

        def focused(index, quicklook):
            labels = controls + (("\u00d7",) if quicklook else ())
            return "[" + labels[index] + "]" in toolbar(quicklook)

        def cycle(quicklook, label):
            count = 6 if quicklook else 5
            for direction in (1, -1):
                for step in range(1, count + 1):
                    self.chord("Tab", "shift") if direction < 0 else self.key("-k", "Tab")
                    self.snapshot(f"{label}-focus-{direction}-{step}", lambda text: focused(step * direction % count, quicklook))

        def click(glyph, quicklook, label):
            line = toolbar(quicklook)
            outer = self.size[1] if quicklook else self.size[1] - self.size[1] * 22 // 100 - self.size[1] * 40 // 100
            padding = int(outer >= 7)
            self.click_cell(self.size[1] - padding - len(line) + line.index(glyph) + 1, self.size[0] - 3, label)

        def facts(quicklook):
            line = self.text.splitlines()[self.size[0] - 5]
            pane = line if quicklook else line.rsplit("\u2502", 1)[-1]
            return pane[1:-1] if len(pane) >= 7 else pane

        def pointer_controls(quicklook, label):
            for glyph, control, page, zoom, action in [
                    (controls[0], 0, 1, 100, "previous"), (controls[0], 0, 1, 100, "first-boundary"),
                    (controls[1], 1, 2, 100, "next"), (controls[1], 1, 2, 100, "last-boundary"),
                    (controls[2], 2, 2, 75, "zoom-out"), (controls[3], 3, 2, 100, "zoom-in")]:
                click(glyph, quicklook, label + "-" + action)
                expected = f"Page {page} / 2 \u00b7 {zoom}%  "
                self.snapshot(label + "-" + action, lambda text: focused(control, quicklook)
                              and facts(quicklook) == expected[:len(facts(quicklook))].ljust(len(facts(quicklook))))
            for _ in range(3):
                self.chord("Tab", "shift")
            self.snapshot(label + "-focus-restored", lambda text: focused(0, quicklook))

        def inert_cell(column, row, label):
            before = (facts(True), toolbar(True))
            self.click_cell(column, row, label)
            self.key("-k", "Tab")
            self.snapshot(label + "-processed", lambda text: focused(1, True) and facts(True) == before[0])
            self.chord("Tab", "shift")
            self.snapshot(label, lambda text: "\u2502" not in text and (facts(True), toolbar(True)) == before)

        self.chord("Tab", "ctrl")
        self.key("-k", "Right")
        self.snapshot("pdf-narrow-page-two", lambda text: "Page 2 / 2" in text and "100%" in text)
        try:
            resize(inline_columns, "pdf-narrow-inline-window")
            self.snapshot("pdf-narrow-inline", lambda text: focused(0, False) and "\u2192" in toolbar(False))
            cycle(False, "pdf-narrow-inline")
            pointer_controls(False, "pdf-narrow-inline-pointer")
            click("\u2192", False, "pdf-narrow-inline-overflow-next")
            self.snapshot("pdf-narrow-expand-revealed", lambda text: focused(4, False) and "\u2502" in text and "Page 2 / 2" in text)
            click("\u2190", False, "pdf-narrow-inline-overflow-previous")
            self.snapshot("pdf-narrow-previous-revealed", lambda text: focused(0, False) and "Page 2 / 2" in text)
            self.chord("Tab", "shift")
            self.snapshot("pdf-narrow-expand-focused", lambda text: focused(4, False))
            click("\u2197", False, "pdf-narrow-expand-pointer")
            self.snapshot("pdf-narrow-expanded", lambda text: "\u2502" not in text and "Page 2 / 2" in text)
            resize(quick_columns, "pdf-narrow-quicklook-window")
            self.key("-k", "Tab")
            self.key("-k", "Tab")
            self.snapshot("pdf-narrow-quicklook", lambda text: focused(0, True) and "\u2192" in toolbar(True))
            cycle(True, "pdf-narrow-quicklook")
            pointer_controls(True, "pdf-narrow-quicklook-pointer")
            inert_cell(1, self.size[0] - 3, "pdf-narrow-inert-left-gutter")
            inert_cell(toolbar(True).index("]") + 3, self.size[0] - 3, "pdf-narrow-inert-control-gap")
            inert_cell(self.size[1], self.size[0] - 4, "pdf-narrow-inert-above-toolbar")
            inert_cell(self.size[1], self.size[0] - 2, "pdf-narrow-inert-below-toolbar")
            minimum_label = "pdf-minimum-window"
            try:
                resize(12, minimum_label)
            except RuntimeError as error:
                expected_errors = [f"native TUI did not reach {minimum_label}{suffix} within {WAIT_SECONDS}s"
                                   for suffix in ("", "-cells")]
                if str(error) not in expected_errors and not str(error).startswith("compositor refused native TUI resize:"):
                    raise
                evidence = guard(self.case, self.case / "evidence/pdf-minimum-blocker.json")
                gap = {"requirement": "Tui.html PDF controls at the renderer's admitted 12-column minimum", "state": "undriven",
                       "reason": str(error), "requested_columns": 12, "window": self.identity(),
                       "pty_rows_columns_pixels": self.terminal_size(), "evidence": str(evidence)}
                evidence.write_text(json.dumps(gap))
                shot = guard(self.case, self.case / "evidence/pdf-minimum-blocker.png")
                if shot.exists():
                    raise RuntimeError("refused stale PDF minimum screenshot")
                self.drive("shot", shot, self.address)
                if not shot.is_file() or shot.stat().st_size == 0:
                    raise RuntimeError("native PDF minimum screenshot returned no new image")
                self.undriven.append(gap)
                guard(self.case, self.case / "evidence/undriven.json").write_text(json.dumps(self.undriven))
                print("TUI_UNDRIVEN " + json.dumps(gap), flush=True)
            else:
                self.snapshot("pdf-minimum-toolbar", lambda text: focused(0, True) and "\u2192" in toolbar(True))
                cycle(True, "pdf-minimum-toolbar")
                self.key("-k", "Escape")
                self.chord("Tab", "ctrl")
                self.snapshot("pdf-minimum-inline", lambda text: focused(0, False) and "\u2502" in text)
                cycle(False, "pdf-minimum-inline")
                self.chord("Tab", "shift")
                self.snapshot("pdf-minimum-expand-focused", lambda text: focused(4, False))
                self.key("-k", "Return")
                self.key("-k", "Tab")
                self.key("-k", "Tab")
                self.snapshot("pdf-minimum-quicklook-restored", lambda text: focused(0, True) and "\u2502" not in text)
            resize(quick_columns, "pdf-minimum-returned")
            self.snapshot("pdf-minimum-state-retained", lambda text: focused(0, True) and "Page 2 / 2" in text and "100%" in text)
            click("\u2192", True, "pdf-narrow-quicklook-overflow-next")
            self.snapshot("pdf-narrow-close-revealed", lambda text: focused(5, True) and "\u2502" not in text)
            click("\u2190", True, "pdf-narrow-quicklook-overflow-previous")
            self.snapshot("pdf-narrow-quicklook-previous", lambda text: focused(0, True) and "Page 2 / 2" in text)
            self.chord("Tab", "shift")
            self.snapshot("pdf-narrow-close-focused", lambda text: focused(5, True))
            click("\u00d7", True, "pdf-narrow-close-pointer")
            self.snapshot("pdf-narrow-closed", lambda text: "\u2502" in text and focused(0, False))
        finally:
            self.resize_window(*original["size"], "pdf-narrow-window-restored")
            if not original["floating"]:
                self.drive("window", "float", self.address)
            self.wait("pdf-narrow-cells-restored", lambda: self.terminal_size()[:2] == cells)
        self.snapshot("pdf-narrow-restored", lambda text: self.cursor_is("02-pages.pdf") and "Page 2 / 2" in text and "100%" in text)
        self.chord("Tab", "ctrl")
        self.key("-k", "Left")
        self.snapshot("pdf-narrow-page-restored", lambda text: "Page 1 / 2" in text)

    def media_controls(self, label):
        self.key("-k", "space")
        self.snapshot(label + "-playing", lambda text: "Pause" in text)
        self.key("-k", "space")
        self.snapshot(label + "-paused", lambda text: "Play" in text)
        self.key("-k", "Tab")
        self.snapshot(label + "-seek-focused", lambda text: "\u25c6" in text)
        self.key("-k", "Right")
        self.snapshot(label + "-seek-forward", lambda text: re.search(r"0:0[5-9] / 0:12", text))
        self.key("h")
        self.snapshot(label + "-seek-back", lambda text: re.search(r"0:0[0-4] / 0:12", text))
        self.chord("Tab", "shift")
        self.key("-k", "Return")
        self.snapshot(label + "-enter-playing", lambda text: "[Pause]" in text)
        self.key("-k", "Escape")
        self.key("-k", "space")
        self.snapshot(label + "-quicklook", lambda text: "Pause" in text and "\u2502" not in text)
        self.key("-k", "space")
        self.snapshot(label + "-quicklook-paused", lambda text: "Play" in text and "\u2502" not in text)
        self.key("-k", "Escape")
        self.snapshot(label + "-quicklook-closed", lambda text: "\u2502" in text and self.cursor_is(label))

    def cleanup(self):
        owned = lambda: [int(path.name) for path in Path("/proc").iterdir() if path.name.isdigit() and self.owned_process(int(path.name))]
        remaining = owned()
        for pid in remaining:
            if self.owned_process(pid):
                try:
                    os.kill(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
        deadline = time.monotonic() + 30  # The backend's drain limit is 25 seconds.
        while remaining and time.monotonic() < deadline:
            time.sleep(POLL_SECONDS)
            remaining = owned()
        for pid in remaining:
            if self.owned_process(pid):
                try:
                    os.kill(pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
        if hasattr(self, "launcher"):
            self.launcher.wait(timeout=5)
        if hasattr(self, "provider_gate"):
            os.close(self.provider_gate)
        self.log.close()
        if remaining:
            raise RuntimeError(f"owned TUI processes failed to drain: {remaining}")


def main():
    if len(ARGS) >= 3 and ARGS[0] == "--provider-helper":
        raise SystemExit(provider_helper(ARGS[1], ARGS[2], ARGS[3:]))
    if ARGS == ["--providers-self-check"]:
        provider_self_check()
        return
    if len(ARGS) == 2 and ARGS[0] == "--calibration-check":
        path = Path(ARGS[1]).resolve(strict=True)
        dimensions = tuple(map(int, command(["magick", "identify", "-format", "%w %h", path]).split()))
        calibration_check(path, dimensions, command(["magick", path, "-depth", "8", "rgb:-"]))
        return
    if ARGS == ["--self-check"]:
        assert frame(b"\x1b[Hbefore\x1b[2;1H1234567890\x1b[Hpartial", 2, 10) == (b"", "")
        assert frame(b"\x1b[Hheader\x1b[2;1Hsearch:ab", 2, 10) == (b"", "")
        assert frame(b"\x1b[Hheader\x1b[2;1H\x1b[7msearch:abc ", 2, 10)[1] == "header\nsearch:abc "
        assert frame(b"\x1b[Hheader\x1b[2;1H1234567890\x1b_Ga=T;DATA\x1b\\", 2, 10)[1] == "header\n1234567890"
        assert frame(b"\x1b[Hheader\x1b[2;1H1234567890\x1bPqPARTIAL", 2, 10)[1] == "header\n1234567890"
        assert frame("\x1b[Hheader\x1b[2;1H: am\u258f\x1b[38;2;1;2;3mber/ ".encode(), 2, 10)[1].splitlines()[-1].startswith(": am\u258fber/")
        print("TUI_OBSERVER_SELF_CHECK 6 passed; native coverage not exercised")
        return
    if len(ARGS) == 3 and ARGS[0] == "--child":
        raise SystemExit(child(ARGS[1], ARGS[2]))
    providers = bool(ARGS and ARGS[0] == "--providers")
    presets = (ARGS[1:] if providers else ARGS) or list(PRESETS)
    terminals = [os.environ["FLEA_TUI_TERMINAL"]] if "FLEA_TUI_TERMINAL" in os.environ else list(TERMINALS)
    if len(set(presets)) != len(presets) or any(preset not in PRESETS for preset in presets) or any(terminal not in TERMINALS for terminal in terminals):
        raise RuntimeError("usage: [FLEA_TUI_TERMINAL=foot|kitty] tests/ui-tui.sh [--providers] [default|vim|mac|windows ...]")
    os.environ["PATH"] = str(Path.home() / ".local/bin") + os.pathsep + os.environ["PATH"]
    helpers = ["omarchy-drive", "xdg-terminal-exec", "script", "stty", "hyprctl", "magick", *terminals]
    if not providers:
        helpers.extend(["ffmpeg", "pdfinfo", "pdftoppm", "mpv"])
    for helper in helpers:
        if not shutil.which(helper):
            raise RuntimeError(f"native TUI prerequisite missing: {helper}")
    repo = SCRIPT.parent.parent
    binary = Path(os.environ.get("FLEA_BIN", repo / "target/release/flea")).resolve(strict=True)
    head = command(["git", "-C", repo, "rev-parse", "HEAD"]).decode().strip()
    if os.environ.get("FLEA_EXPECTED_SHA") != head:
        raise RuntimeError("FLEA_EXPECTED_SHA must identify the current candidate before native testing")
    inputs = list((repo / "src").rglob("*.rs")) + [repo / name for name in ["Cargo.toml", "Cargo.lock", "keys.toml"]]
    if (repo / "build.rs").is_file():
        inputs.append(repo / "build.rs")
    if max(path.stat().st_mtime_ns for path in inputs) > binary.stat().st_mtime_ns:
        raise RuntimeError("candidate binary predates a Rust source, keymap or manifest")
    session = command(["omarchy-drive", "env"]).decode().splitlines()
    required = {"XDG_RUNTIME_DIR", "WAYLAND_DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "YDOTOOL_SOCKET", "OMARCHY_PATH", "QT_LINUX_ACCESSIBILITY_ALWAYS_ON"}
    values = {}
    for row in session:
        match = re.fullmatch(r"export ([A-Z_]+)=([A-Za-z0-9_./-]+)", row)
        if not match or match[1] not in required or match[1] in values:
            raise RuntimeError("omarchy-drive returned an unexpected session row")
        values[match[1]] = match[2]
    if values.keys() != required or values["XDG_RUNTIME_DIR"] != f"/run/user/{os.getuid()}" or values["OMARCHY_PATH"] != "/usr/share/omarchy" \
            or values["YDOTOOL_SOCKET"] != values["XDG_RUNTIME_DIR"] + "/.ydotool_socket" \
            or values["QT_LINUX_ACCESSIBILITY_ALWAYS_ON"] != "1" or not re.fullmatch(r"wayland-[0-9]+", values["WAYLAND_DISPLAY"]):
        raise RuntimeError("native TUI session identity is incomplete or unexpected")
    os.environ.update(values)
    with open(Path(values["XDG_RUNTIME_DIR"]) / "flea-display.lock", "a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        root = Path(tempfile.mkdtemp(prefix="flea-tui-native.", dir="/tmp"))
        (root / ".flea-test-sandbox").write_text("Flea TUI native evidence\n")
        print(f"TUI_NATIVE_ROOT={root}", flush=True)
        (root / "candidate.json").write_text(json.dumps({"head": head, "binary": str(binary),
            "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(), "session": values,
            "source_status": command(["git", "-C", repo, "status", "--porcelain"]).decode(),
            "harness_sha256": hashlib.sha256(SCRIPT.read_bytes()).hexdigest(),
            "keymap_sha256": hashlib.sha256((repo / "keys.toml").read_bytes()).hexdigest()}))
        undriven = 0
        for terminal in terminals:
            for preset in presets:
                native = Native(root, binary, preset, terminal)
                try:
                    native.providers() if providers else native.smoke()
                finally:
                    native.cleanup()
                undriven += len(native.undriven)
                print(f"TUI_NATIVE group={'providers' if providers else 'smoke'} terminal={terminal} preset={preset} checks={native.checks} failed=0 undriven={len(native.undriven)} visual_inspection=pending", flush=True)
        if undriven:
            print(f"TUI_INCOMPLETE required_undriven={undriven}; independent coverage finished", flush=True)
            raise SystemExit(2)


try:
    main()
except Exception as error:
    print(f"TUI_FAIL {error}", file=sys.stderr, flush=True)
    raise SystemExit(1)
PY
