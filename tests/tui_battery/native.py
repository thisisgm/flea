"""Owned headless-output lifecycle and compositor-delivered input."""
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import time

from .evidence import Blocked, digest, save
from .geometry import COLUMNS, ROWS
from .keys import shortcut
from . import relay, png


class Deadline(Blocked):
    """A wait that ran out of time, so callers can tell it apart from a check that failed."""


class DisplayLost(Blocked):
    """The owned output cannot show the owned terminal, so every later state would fail the same way; the run stops."""


def wait_for(check, timeout, description):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(.03)
    raise Deadline(f"deadline waiting for {description}")


HYPR_HELPER = Path(__file__).resolve().parents[2] / "tests/lib/hypr-dispatch.sh"
# Each owned terminal gets this corner radius, so edge samples keep this far from its corners.
WINDOW_ROUNDING = 8
# Hyprland 0.56.2 applies exec_cmd's workspace, monitor and float rules to the window whose process holds this token.
EXEC_RULE_TOKEN = "HL_EXEC_RULE_TOKEN"
# Hyprland 0.56.2 reports a new headless output at its default mode for about a second before the set mode applies.
MODE_DEADLINE = 5
# Hyprland switches the owned output to the owned workspace when the first terminal maps there.
SHOW_DEADLINE = 3
# The candidate gets this many quit attempts through the compositor, each this long, before the relay stops it.
QUIT_ROUNDS = 3
QUIT_SECONDS = 2
# tmux reads an Escape followed within its escape-time (500 ms by default) as Alt, so the quit key waits this long.
ESCAPE_PAUSE = .6
ESCAPE = {"key": "Escape"}


def locked(proc=Path("/proc")):
    """True when this user's hyprlock runs, because a session lock covers every output, the owned one included."""
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid == os.getuid() and (entry / "comm").read_text().strip() == "hyprlock":
                return True
        except OSError:
            continue
    return False


# Every compositor command runs as one bash call that sources the typed helper.
def compositor(commands, function, *arguments, check=True):
    return commands.run(["bash", "-c", 'source "$1" && shift && "$@"', "hypr", str(HYPR_HELPER), function, *arguments],
                        check=check)


class Display:
    def __init__(self, commands, identity):
        self.commands, self.identity = commands, identity
        self.output = "FLEA-" + identity
        self.workspace = "name:" + identity
        self.created = False
        self.original_rounding = None

    def data(self, function):
        return json.loads(compositor(self.commands, function).stdout)

    def __enter__(self):
        # hyprctl 0.56.2 prints its help and exits 1, so the advertised commands decide, not the exit code.
        help_result = compositor(self.commands, "hypr_query_help", check=False)
        compositor(self.commands, "hypr_query_version")
        compositor(self.commands, "hypr_query_output_help", check=False)
        if "output" not in (help_result.stdout + help_result.stderr).decode(errors="replace"):
            raise Blocked("installed hyprctl does not advertise output control")
        self.original_rounding = json.loads(compositor(self.commands, "hypr_query_option", "decoration:rounding").stdout)
        if any(m["name"] == self.output for m in self.data("hypr_query_monitors")):
            raise Blocked("headless output identity is already in use")
        compositor(self.commands, "hypr_output_create", self.output)
        self.created = True
        try:
            compositor(self.commands, "hypr_monitor_mode", self.output, "2560x1440@60")
            seen = []

            def applied():
                monitor = next((m for m in self.data("hypr_query_monitors") if m["name"] == self.output), None)
                seen[:] = [monitor]
                return monitor if monitor and (monitor["scale"], monitor["width"], monitor["height"]) == (1, 2560, 1440) else None
            try:
                self.monitor = wait_for(applied, MODE_DEADLINE, "owned output mode")
            except Deadline:
                last = seen[0] if seen else None
                observed = f"{last['width']}x{last['height']} scale {last['scale']}" if last else "no owned output"
                raise Blocked("owned output mode/scale could not be verified: last seen " + observed) from None
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, *_):
        if self.created:
            compositor(self.commands, "hypr_output_remove", self.output)
            self.created = False

    def shown(self):
        """The workspace the owned output shows, which is the only one its screencopy can capture."""
        monitor = next((m for m in self.data("hypr_query_monitors") if m["name"] == self.output), None)
        if not monitor:
            raise DisplayLost(f"owned output {self.output} is gone")
        return (monitor.get("activeWorkspace") or {}).get("name")

    def require_shown(self):
        if locked():
            raise DisplayLost("the session is locked, so a capture would show the lock screen")
        name = self.shown()
        if name != self.workspace[5:]:
            raise DisplayLost(f"owned output {self.output} shows workspace {name}, not the owned workspace {self.workspace[5:]}")

    def client(self, identity):
        matches = [c for c in self.data("hypr_query_clients") if c.get("class") == identity and c.get("title") == identity]
        if len(matches) > 1:
            raise Blocked("ambiguous owned terminal identity")
        if matches:
            client = matches[0]
            if client.get("monitor") != self.monitor["id"] or client.get("workspace", {}).get("name") != self.workspace[5:]:
                raise Blocked("owned terminal escaped its headless workspace")
            return client

    def park(self):
        """Puts the pointer on the owned output's last pixel, outside every window on it, the way the GUI capture tools do."""
        x, y = self.monitor["x"] + self.monitor["width"] - 1, self.monitor["y"] + self.monitor["height"] - 1
        for client in self.data("hypr_query_clients"):
            (cx, cy), (cw, ch) = client.get("at", (0, 0)), client.get("size", (0, 0))
            if client.get("monitor") == self.monitor["id"] and cx <= x < cx + cw and cy <= y < cy + ch:
                raise Blocked(f"window {client.get('class')} covers the pointer's parking pixel {x},{y}")
        compositor(self.commands, "hypr_cursor_move", str(x), str(y))
        # Sample input, hyprctl -j cursorpos: {"x": 5119, "y": 1439}.
        cursor = json.loads(compositor(self.commands, "hypr_query_cursor").stdout)
        if (cursor.get("x"), cursor.get("y")) != (x, y):
            raise Blocked(f"pointer parked at {x},{y} reads back at {cursor.get('x')},{cursor.get('y')}")

    def capture(self, path):
        self.require_shown()
        self.park()
        self.commands.run(["grim", "-o", self.output, path])
        image = png.read(path)
        if image[:2] != (self.monitor["width"], self.monitor["height"]):
            raise Blocked("screencopy dimensions differ from owned output")
        return image


class Terminal:
    def __init__(self, display, adapter, case, commands, timeout, quit=None):
        self.display, self.adapter, self.case = display, adapter, case
        self.commands, self.timeout, self.quit = commands, timeout, quit
        self.client = None
        self.launched = False

    def launch(self, argv, environment, graphics_negative=False):
        root = str(Path(__file__).resolve().parents[1])
        environment = dict(environment, PYTHONPATH=root)
        save(self.case / "relay-spec.json", {"argv": argv, "environment": environment, "graphics_negative": graphics_negative})
        relay_command = [sys.executable, "-m", "tui_battery.relay", str(self.case / "relay-spec.json")]
        terminal_argv = self.adapter.argv(relay_command)
        if self.adapter.tmux_socket and len(os.fsencode(self.adapter.tmux_socket)) >= 100:
            raise Blocked("private tmux socket path is too long; use a shorter marked evidence root")
        terminal_environment = dict(self.adapter.environment(), PYTHONPATH=root, PYTHONDONTWRITEBYTECODE="1")
        save(self.case / "launch-spec.json", {"argv": terminal_argv, "environment": terminal_environment})
        wrapper = ["env", f"PYTHONPATH={root}", sys.executable, "-m", "tui_battery.native", "--launch", str(self.case / "launch-spec.json")]
        # The window rules run a script file, so no argv byte reaches the Lua string.
        launcher = self.case / "launch.sh"
        descriptor = os.open(launcher, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o700)
        with os.fdopen(descriptor, "w") as handle:
            handle.write("#!/bin/bash\nexec " + shlex.join(wrapper) + "\n")
        compositor(self.commands, "hypr_session_exec_rules", self.display.workspace, self.display.output, str(launcher.absolute()))
        self.launched = True
        self.client = wait_for(lambda: self.display.client(self.adapter.identity), self.timeout, "owned native client")
        try:
            wait_for(lambda: self.display.shown() == self.display.workspace[5:], SHOW_DEADLINE, "owned workspace on the owned output")
        except Deadline:
            self.display.require_shown()
        wait_for(lambda: (self.case / "relay-start.json").is_file(), self.timeout, "controlling PTY")
        start = json.loads((self.case / "relay-start.json").read_text())
        if not start["stdin_tty"] or not start["stdout_tty"]:
            raise Blocked("candidate stdin/stdout are not real TTYs")
        compositor(self.commands, "hypr_window_rounding", self.client["address"], str(WINDOW_ROUNDING))
        self.fit(COLUMNS, ROWS)
        return start

    def fit(self, columns, rows):
        """Sizes a window its terminal opened at another size, from the terminal's own cell metrics."""
        snapshot = self.snapshot()
        height_cells, width_cells, width, height = snapshot["pty"]
        if (height_cells, width_cells) == (rows, columns):
            return snapshot
        if not (height_cells and width_cells and width and height):
            raise Blocked(f"terminal opened at {width_cells}x{height_cells} cells with no pixel metrics, so it cannot be sized to {columns}x{rows}")
        size = self.client["size"]
        calibration = {"cell": [width // width_cells, height // height_cells], "padding": [size[0] - width, size[1] - height]}
        return self.resize(columns, rows, calibration)

    def snapshot(self):
        self.client = self.display.client(self.adapter.identity)
        if not self.client:
            raise Blocked("terminal exited before its predicate")
        return relay.rpc(self.case, {"op": "snapshot"})

    def key(self, key):
        mods, name = shortcut(key)
        compositor(self.commands, "hypr_window_send_shortcut", self.client["address"], mods, name)

    def resize(self, columns, rows, calibration):
        cw, ch = calibration["cell"]
        px, py = calibration["padding"]
        compositor(self.commands, "hypr_window_resize", self.client["address"], str(columns * cw + px), str(rows * ch + py))
        return wait_for(lambda: self._resized(columns, rows), self.timeout, "native PTY resize")

    def _resized(self, columns, rows):
        snapshot = self.snapshot()
        return snapshot if snapshot["pty"][:2] == [rows, columns] else None

    def close(self):
        exit_path = self.case / "launch-exit.json"
        # Sample input: {"exit_code":0,"end_ns":1792330118607080126}, visible only after its JSON write completes.
        def exit_receipt():
            try:
                value = json.loads(exit_path.read_text())
            except (FileNotFoundError, json.JSONDecodeError):
                return None
            if not isinstance(value, dict):
                raise Blocked("terminal exit receipt must be an object: " + str(exit_path))
            return value
        try:
            if (self.case / "relay.sock").exists() and not (self.case / "relay-exit.json").exists() and not self.quit_by_key():
                try:
                    relay.rpc(self.case, {"op": "stop"})
                except (ConnectionError, OSError):
                    pass
            return wait_for(exit_receipt, 5, "terminal exit and child reap: " + str(exit_path))
        except BaseException:
            # The caller reaps the group under an ownership anchor; a bare pid here could name a recycled group.
            raise
        finally:
            self.adapter.close(self.commands)

    def quit_by_key(self):
        """Quits the candidate the way a person does, so the receipt records its own exit and terminal restore."""
        if not self.quit or not self.client:
            return False
        for attempt in range(QUIT_ROUNDS):
            try:
                if attempt:
                    self.key(ESCAPE)
                    time.sleep(ESCAPE_PAUSE)
                self.key(self.quit)
                wait_for(lambda: (self.case / "relay-exit.json").exists(), QUIT_SECONDS, "candidate quit")
                return True
            except Deadline:
                continue
            except (Blocked, OSError):
                return False
        return False


def launch(spec_path):
    spec_path = Path(spec_path)
    spec = json.loads(spec_path.read_text())
    directory = spec_path.parent
    # The terminal keeps its allowlist plus the token Hyprland gave this exec, so the rules match its window.
    environment = dict(spec["environment"])
    if os.environ.get(EXEC_RULE_TOKEN):
        environment[EXEC_RULE_TOKEN] = os.environ[EXEC_RULE_TOKEN]
    with (directory / "terminal.stdout").open("xb") as stdout, (directory / "terminal.stderr").open("xb") as stderr:
        child = subprocess.Popen(spec["argv"], env=environment, stdout=stdout, stderr=stderr, start_new_session=True)
        save(directory / "launch-start.json", {"pid": child.pid, "argv": spec["argv"], "start_ns": time.time_ns(),
                                               "exec_rule_token": EXEC_RULE_TOKEN in environment})
        code = child.wait()
        save(directory / "launch-exit.json", {"exit_code": code, "end_ns": time.time_ns()})
        return code


if __name__ == "__main__":
    if sys.argv[1] != "--launch":
        raise SystemExit("expected --launch SPEC")
    sys.exit(launch(sys.argv[2]))
