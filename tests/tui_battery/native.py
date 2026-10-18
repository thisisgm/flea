"""Owned headless-output lifecycle and compositor-delivered input."""
import json
import os
from pathlib import Path
import shlex
import signal
import subprocess
import sys
import time

from .evidence import Blocked, digest, save
from .keys import shortcut
from . import relay, png


def wait_for(check, timeout, description):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        result = check()
        if result:
            return result
        time.sleep(.03)
    raise Blocked(f"deadline waiting for {description}")


HYPR_HELPER = Path(__file__).resolve().parents[2] / "tests/lib/hypr-dispatch.sh"


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
        help_result = compositor(self.commands, "hypr_query_help")
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
            monitors = self.data("hypr_query_monitors")
            monitor = next((m for m in monitors if m["name"] == self.output), None)
            if not monitor or monitor["scale"] != 1 or monitor["width"] != 2560 or monitor["height"] != 1440:
                raise Blocked("owned output mode/scale could not be verified")
            self.monitor = monitor
            return self
        except BaseException:
            self.__exit__(None, None, None)
            raise

    def __exit__(self, *_):
        if self.created:
            compositor(self.commands, "hypr_output_remove", self.output)
            self.created = False

    def client(self, identity):
        matches = [c for c in self.data("hypr_query_clients") if c.get("class") == identity and c.get("title") == identity]
        if len(matches) > 1:
            raise Blocked("ambiguous owned terminal identity")
        if matches:
            client = matches[0]
            if client.get("monitor") != self.monitor["id"] or client.get("workspace", {}).get("name") != self.workspace[5:]:
                raise Blocked("owned terminal escaped its headless workspace")
            return client

    def capture(self, path):
        self.commands.run(["grim", "-o", self.output, path])
        image = png.read(path)
        if image[:2] != (self.monitor["width"], self.monitor["height"]):
            raise Blocked("screencopy dimensions differ from owned output")
        return image


class Terminal:
    def __init__(self, display, adapter, case, commands, timeout):
        self.display, self.adapter, self.case = display, adapter, case
        self.commands, self.timeout = commands, timeout
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
        wait_for(lambda: (self.case / "relay-start.json").is_file(), self.timeout, "controlling PTY")
        start = json.loads((self.case / "relay-start.json").read_text())
        if not start["stdin_tty"] or not start["stdout_tty"]:
            raise Blocked("candidate stdin/stdout are not real TTYs")
        compositor(self.commands, "hypr_window_rounding", self.client["address"], "8")
        return start

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
            if (self.case / "relay.sock").exists() and not (self.case / "relay-exit.json").exists():
                try:
                    relay.rpc(self.case, {"op": "stop"})
                except (ConnectionError, OSError):
                    pass
            return wait_for(exit_receipt, 5, "terminal exit and child reap: " + str(exit_path))
        except BaseException:
            start = self.case / "launch-start.json"
            if start.is_file():
                pid = json.loads(start.read_text())["pid"]
                if type(pid) is not int or pid <= 1:
                    raise Blocked("invalid terminal process group in " + str(start))
                try:
                    os.killpg(pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
            raise
        finally:
            self.adapter.close(self.commands)


def launch(spec_path):
    spec_path = Path(spec_path)
    spec = json.loads(spec_path.read_text())
    directory = spec_path.parent
    with (directory / "terminal.stdout").open("xb") as stdout, (directory / "terminal.stderr").open("xb") as stderr:
        child = subprocess.Popen(spec["argv"], env=spec["environment"], stdout=stdout, stderr=stderr, start_new_session=True)
        save(directory / "launch-start.json", {"pid": child.pid, "argv": spec["argv"], "start_ns": time.time_ns()})
        code = child.wait()
        save(directory / "launch-exit.json", {"exit_code": code, "end_ns": time.time_ns()})
        return code


if __name__ == "__main__":
    if sys.argv[1] != "--launch":
        raise SystemExit("expected --launch SPEC")
    sys.exit(launch(sys.argv[2]))
