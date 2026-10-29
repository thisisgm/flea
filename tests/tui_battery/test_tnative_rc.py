"""Why the native battery accepted none of 325 variants on minipc, one regression per cause, plus its smoke and preflight."""
from contextlib import ExitStack, redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import pty
import re
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import MagicMock, patch

from . import adapters, driver, dynamic, fixtures, native, offline, png
from .bundle import build as bundle
from .evidence import Blocked, Commands, MARKER
from .review_support import pair

HERE = Path(__file__).resolve().parent
HELPER = HERE.parent / "lib/hypr-dispatch.sh"
REAL_RUN = subprocess.run
# GLib's g_application_id_is_valid, restated so the check does not lean on the harness's own pattern.
GTK_ID = re.compile(r"(?=.{1,255}$)[A-Za-z_-][A-Za-z0-9_-]*(\.[A-Za-z_-][A-Za-z0-9_-]*)+")
# alacritty 0.17 on minipc drew a 72x22 grid: winit's 800x600 window in 11x26 px cells with 8 and 28 px left over.
CELL, PADDING, OPENED = (11, 26), (8, 28), (800, 600)
IDENTITY = "flea.battery.tfixture"
QUIT = {"key": "q", "mods": "tui", "action": "quit"}
PALETTE = {"fg": "#c6b395", "bg": "#1c120d"}
# Stands in for bwrap at the relay's exec: it keeps the argv it was given, the sandbox the candidate would get.
FAKE_BWRAP = """#!/usr/bin/python3
import json, os, sys
with open(os.environ["BWRAP_RECORD"], "x") as record:
    json.dump(sys.argv[1:], record)
"""


class FakeHyprland:
    """Hyprland 0.56.2 as the battery sees it: a new headless output shows workspace 2, a silent exec rule maps off it."""

    def __init__(self, root, case):
        self.root, self.case = root, case
        self.monitors = [{"name": "DP-1", "id": 0, "x": 0, "y": 0, "width": 2560, "height": 1440, "scale": 1,
                          "activeWorkspace": {"id": 1, "name": "1"}}]
        self.clients, self.cursor = [], {"x": 1280, "y": 720}
        self.rules, self.shortcuts, self.resizes, self.rpcs = [], [], [], []
        self.size, self.pixels, self.quit_after_escape, self.grims = list(OPENED), True, False, 0
        self.pointer_at_grim = []

    def run(self, argv, **_):
        argv = [str(a) for a in argv]
        if argv[0] == "grim":
            self.grims += 1
            self.pointer_at_grim.append(dict(self.cursor))
            output = next(m for m in self.monitors if m["name"] == argv[2])
            png.write(Path(argv[3]), output["width"], output["height"], bytes(output["width"] * output["height"] * 4))
            return subprocess.CompletedProcess(argv, 0, b"", b"")
        code, stdout = getattr(self, argv[5].removeprefix("hypr_"))(*argv[6:])
        return subprocess.CompletedProcess(argv, code, stdout if isinstance(stdout, bytes) else json.dumps(stdout).encode(), b"")

    def query_help(self):
        return 1, b"commands: monitors clients output version"

    def query_output_help(self):
        return 1, b"output create remove"

    def query_version(self):
        return 0, b"Hyprland 0.56.2"

    def query_option(self, _):
        return 0, {"int": 0}

    def query_monitors(self):
        return 0, self.monitors

    def query_clients(self):
        return 0, self.clients

    def query_cursor(self):
        return 0, self.cursor

    def cursor_move(self, x, y):
        self.cursor = {"x": int(x), "y": int(y)}
        return 0, b""

    def output_create(self, name):
        self.monitors.append({"name": name, "id": 7, "x": 2560, "y": 0, "width": 2560, "height": 1440, "scale": 1,
                              "activeWorkspace": {"id": 2, "name": "2"}})
        return 0, b""

    def monitor_mode(self, *_):
        return 0, b""

    def output_remove(self, name):
        self.monitors = [m for m in self.monitors if m["name"] != name]
        return 0, b""

    def session_exec_rules(self, workspace, output, script):
        # The real helper writes the Lua; only hyprctl is a stand-in, so the rule text is the one minipc received.
        bin_dir, log = self.root / "fakebin", self.root / "hyprctl.log"
        bin_dir.mkdir(exist_ok=True)
        (bin_dir / "hyprctl").write_text('#!/bin/sh\nprintf "%s\\n" "$2" >> "$HYPR_LOG"\nprintf "ok\\n"\n')
        (bin_dir / "hyprctl").chmod(0o700)
        result = REAL_RUN(["bash", "-c", 'source "$1" && shift && "$@"', "hypr", str(HELPER), "hypr_session_exec_rules",
                           workspace, output, script], env={"PATH": f"{bin_dir}:/usr/bin:/bin", "HYPR_LOG": str(log)},
                          capture_output=True, timeout=10)
        rule = re.search(r'workspace = "([^"]*)"', log.read_text().splitlines()[-1]).group(1)
        self.rules.append(rule)
        name = rule.split()[0].removeprefix("name:")
        monitor = next(m for m in self.monitors if m["name"] == output)
        if not rule.endswith(" silent"):
            monitor["activeWorkspace"] = {"id": -1337, "name": name}
        self.clients.append({"class": IDENTITY, "title": IDENTITY, "address": "0xabc", "monitor": monitor["id"],
                             "workspace": {"id": -1337, "name": name}, "at": [monitor["x"] + 880, 420],
                             "size": self.size, "focusHistoryID": 0})
        (self.case / "relay.sock").write_text("")
        (self.case / "relay-start.json").write_text(json.dumps({"pid": 4242, "pty": self.pty(), "stdin_tty": True, "stdout_tty": True}))
        return result.returncode, result.stdout

    def window_rounding(self, *_):
        return 0, b""

    def window_resize(self, _, width, height):
        self.resizes.append((int(width), int(height)))
        self.size[:] = [int(width), int(height)]
        return 0, b""

    def window_send_shortcut(self, _, mods, key):
        self.shortcuts.append((mods, key))
        if key == "q" and (not self.quit_after_escape or ("", "Escape") in self.shortcuts):
            self.exit(0)
        return 0, b""

    def pty(self):
        columns, rows = ((self.size[i] - PADDING[i]) // CELL[i] for i in (0, 1))
        return [rows, columns, columns * CELL[0] if self.pixels else 0, rows * CELL[1] if self.pixels else 0]

    def rpc(self, _, request, timeout=None):
        self.rpcs.append(request["op"])
        if request["op"] == "stop":
            self.exit(-15)
        rows, columns = self.pty()[:2]
        return {"pty": self.pty(), "grid": [], "columns": columns, "rows": rows}

    def exit(self, code):
        (self.case / "relay-exit.json").write_text(json.dumps({"candidate_exit": code, "termios_restored": code == 0,
                                                               "relay_termios_restored": True}))
        (self.case / "launch-exit.json").write_text(json.dumps({"exit_code": 0 if code == 0 else 143, "end_ns": 1}))


class NativeRootCauseTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="tnative-rc.", dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text("owned native root-cause regressions\n")
        self.case = self.root / "case"
        self.case.mkdir()
        self.fake = FakeHyprland(self.root, self.case)
        stack = ExitStack()
        self.addCleanup(stack.close)
        stack.enter_context(patch("tui_battery.evidence.subprocess.run", side_effect=self.fake.run))
        stack.enter_context(patch.object(native.relay, "rpc", side_effect=self.fake.rpc))
        stack.enter_context(patch.object(native, "locked", return_value=False, create=True))
        for name, value in (("SHOW_DEADLINE", .3), ("QUIT_SECONDS", .3), ("ESCAPE_PAUSE", 0)):
            stack.enter_context(patch.object(native, name, value, create=True))
        self.display = stack.enter_context(native.Display(Commands(), "id"))

    def terminal(self):
        adapter = SimpleNamespace(identity=IDENTITY, argv=lambda command: ["fake-terminal", *command], environment=lambda: {},
                                  tmux_socket=None, close=lambda commands: None)
        terminal = native.Terminal(self.display, adapter, self.case, Commands(), 2)
        terminal.quit = QUIT
        return terminal

    def owned_workspace(self):
        return next(m for m in self.fake.monitors if m["name"] == "FLEA-id")["activeWorkspace"]["name"]

    def test_the_terminal_maps_onto_the_workspace_its_output_shows(self):
        # On 795592aa0 every kitty, foot and tmux capture showed the owned output's first workspace, 2, with no window on it.
        self.terminal().launch(["candidate"], {})
        self.assertNotIn("silent", self.fake.rules[-1])
        self.assertEqual(self.owned_workspace(), "id")

    def test_a_capture_of_an_output_showing_another_workspace_stops_the_run(self):
        lost = getattr(native, "DisplayLost", Blocked)
        with self.assertRaisesRegex(lost, "shows workspace 2, not the owned workspace id"):
            self.display.capture(self.case / "calibration.png")
        self.assertEqual(self.fake.grims, 0)

    def test_each_capture_parks_the_pointer_on_the_owned_outputs_last_pixel_first(self):
        # The 20261019T042958Z smoke drew the pointer inside alacritty's grid, at about 715,418 of its grid-crop.png.
        self.terminal().launch(["candidate"], {})
        self.fake.cursor = {"x": 2560 + 880 + 300, "y": 420 + 200}
        self.display.capture(self.case / "calibration.png")
        self.assertEqual(self.fake.pointer_at_grim, [{"x": 2560 + 2559, "y": 1439}])

    def test_a_window_over_the_parking_pixel_stops_the_capture_before_grim(self):
        self.terminal().launch(["candidate"], {})
        self.fake.clients[0]["size"] = [2560 - 880, 1440 - 420]
        with self.assertRaisesRegex(Blocked, "covers the pointer's parking pixel 5119,1439"):
            self.display.capture(self.case / "calibration.png")
        self.assertEqual(self.fake.grims, 0)

    def test_a_terminal_that_opens_at_another_size_is_sized_to_the_battery_grid_from_its_own_cells(self):
        # alacritty's 72x22 PTY failed every state with "diagnostic PTY must report 130x31 cells".
        terminal = self.terminal()
        terminal.launch(["candidate"], {})
        self.assertEqual(self.fake.resizes, [(130 * CELL[0] + PADDING[0], 31 * CELL[1] + PADDING[1])])
        self.assertEqual(terminal.snapshot()["pty"], [31, 130, 130 * CELL[0], 31 * CELL[1]])

    def test_a_terminal_with_no_pixel_metrics_is_blocked_by_name_not_resized(self):
        self.fake.pixels = False
        with self.assertRaisesRegex(Blocked, "opened at 72x22 cells with no pixel metrics"):
            self.terminal().launch(["candidate"], {})
        self.assertEqual(self.fake.resizes, [])

    def test_the_candidate_quits_through_its_own_key_so_its_exit_and_terminal_restore_are_its_own(self):
        # Every launched state on 795592aa0 ended with candidate_exit -15 and termios_restored false: the relay stopped it.
        terminal = self.terminal()
        terminal.launch(["candidate"], {})
        self.assertEqual(terminal.close(), {"exit_code": 0, "end_ns": 1})
        self.assertNotIn("stop", self.fake.rpcs)
        self.assertEqual(self.fake.shortcuts, [("", "q")])
        self.assertEqual(json.loads((self.case / "relay-exit.json").read_text())["candidate_exit"], 0)

    def test_a_modal_state_gets_escape_then_quit_and_only_then_the_relay_stop(self):
        self.fake.quit_after_escape = True
        terminal = self.terminal()
        terminal.launch(["candidate"], {})
        terminal.close()
        self.assertEqual(self.fake.shortcuts, [("", "q"), ("", "Escape"), ("", "q")])
        self.assertNotIn("stop", self.fake.rpcs)


class IdentityAndShutdownTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="tnative-id.", dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text("owned native identity regressions\n")

    def test_each_terminal_gets_an_identity_ghostty_keeps_as_its_class(self):
        # ghostty 1.3 drops a class GTK refuses and maps as com.mitchellh.ghostty, so "flea-<hex>" never matched a client.
        created = []
        root = self.root / "pair"
        root.mkdir()
        pair(root, adapters=created)
        identity = created[0][2]
        self.assertTrue(GTK_ID.fullmatch(identity), identity)

    def test_ghostty_refuses_a_class_it_would_ignore(self):
        adapter = adapters.Adapter("ghostty", self.root, "flea-0123abcd", PALETTE)
        with self.assertRaisesRegex(Blocked, "valid GTK application ID"):
            adapter.argv(["relay"])

    def test_a_display_lost_mid_state_ends_the_run_after_recording_the_state(self):
        lost = getattr(native, "DisplayLost", None)
        self.assertIsNotNone(lost, "the harness has no run-ending display failure")
        def capture(path):
            raise lost("owned output FLEA-id shows workspace 2, not the owned workspace id")
        root = self.root / "lost"
        root.mkdir()
        with self.assertRaises(lost):
            pair(root, capture=capture)
        verdict = json.loads((root / "foot/tui-list-text/primary/verdict.json").read_text())
        self.assertEqual(verdict["reason"], "owned output FLEA-id shows workspace 2, not the owned workspace id")

    def test_tmux_close_accepts_a_server_that_already_exited_with_its_session(self):
        # Every tmux state failed on "tmux: no server running": exit-empty had already ended the private server.
        adapter = adapters.Adapter("tmux", self.root, IDENTITY, PALETTE)
        adapter.tmux_socket = str(self.root / "tmux.sock")
        Path(adapter.tmux_socket).write_text("")
        for stderr, refused in ((b"no server running on " + adapter.tmux_socket.encode() + b"\n", False),
                                (b"error connecting to " + adapter.tmux_socket.encode() + b" (Connection refused)\n", False),
                                (b"lost server\n", True)):
            with self.subTest(stderr=stderr), patch("tui_battery.evidence.subprocess.run",
                                                     return_value=subprocess.CompletedProcess([], 1, b"", stderr)):
                if refused:
                    with self.assertRaisesRegex(Blocked, "lost server"):
                        adapter.close(Commands())
                else:
                    adapter.close(Commands())


class TmuxSocketTests(unittest.TestCase):
    """Flea asks its tmux server for #{client_termfeatures}, so inside the sandbox the socket TMUX names must be there."""

    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="tnative-tmux.", dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text("owned tmux socket regressions\n")
        self.case = self.root / "case"
        self.case.mkdir()
        (self.case / MARKER).write_text("owned relay case\n")
        # The battery's tmux server listens in its case directory; binding through the directory fd keeps a long root legal.
        self.socket = self.case / "tmux.sock"
        server = socket.socket(socket.AF_UNIX)
        self.addCleanup(server.close)
        directory = os.open(self.case, os.O_RDONLY | os.O_DIRECTORY)
        try:
            server.bind(f"/proc/self/fd/{directory}/tmux.sock")
        finally:
            os.close(directory)
        server.listen(1)
        tools = self.root / "bin"
        tools.mkdir()
        (tools / "bwrap").write_text(FAKE_BWRAP)
        (tools / "bwrap").chmod(0o755)
        self.record = self.root / "bwrap.json"
        with patch("tui_battery.fixtures.shutil.which", return_value="/usr/bin/bwrap"):
            self.argv = fixtures.namespace("/opt/candidate/flea", self.root / "fx", "/fx/v/work", {})
        (self.case / "relay-spec.json").write_text(json.dumps(
            {"argv": self.argv, "environment": {"PATH": f"{tools}:/usr/bin:/bin", "BWRAP_RECORD": str(self.record)}}))

    def relay(self, tmux):
        """Runs the real relay on a real PTY inside a tmux pane's environment, and returns its exit and stderr."""
        master, slave = pty.openpty()
        try:
            result = REAL_RUN([sys.executable, "-m", "tui_battery.relay", str(self.case / "relay-spec.json")],
                              stdin=slave, stdout=slave, stderr=subprocess.PIPE, timeout=30,
                              env={"PATH": "/usr/bin:/bin", "PYTHONPATH": str(HERE.parent), "LANG": "C.UTF-8",
                                   "TERM": "tmux-256color", "TMUX": tmux})
        finally:
            os.close(slave)
            os.close(master)
        return result.returncode, result.stderr.decode(errors="replace")

    def test_the_sandbox_gets_tmux_and_its_server_socket_and_nothing_else(self):
        tmux = f"{self.socket},4242,0"
        code, stderr = self.relay(tmux)
        self.assertEqual(code, 0, stderr)
        index = self.argv.index("--chdir")
        forwarded = ["--setenv", "TERM", "tmux-256color", "--setenv", "TMUX", tmux, "--ro-bind", str(self.socket), str(self.socket)]
        # bwrap is argv[0], so its record starts at the first option.
        self.assertEqual(json.loads(self.record.read_text()), (self.argv[:index] + forwarded + self.argv[index:])[1:])

    def test_a_tmux_variable_naming_no_socket_stops_the_relay_before_the_sandbox(self):
        stray = self.case / "not-a-socket"
        stray.write_text("")
        code, stderr = self.relay(f"{stray},4242,0")
        self.assertNotEqual(code, 0)
        self.assertIn("TMUX names no tmux server socket: " + str(stray), stderr)
        self.assertFalse(self.record.exists())


def remove(root):
    """Removes a marked test root, opening any folder a fixture closed first."""
    if (root / MARKER).is_file():
        for path, directories, _ in os.walk(root):
            for name in directories:
                if not os.path.islink(os.path.join(path, name)):
                    os.chmod(os.path.join(path, name), 0o700)
        shutil.rmtree(root)


def total(path):
    """A folder's size the way Flea's dirsize walk adds it: its own and every entry's lstat size below it."""
    size = path.lstat().st_size
    if path.is_dir() and not path.is_symlink():
        size += sum(total(child) for child in path.iterdir())
    return size


class FontCacheTests(unittest.TestCase):
    def test_the_sandbox_reads_the_system_font_cache_read_only(self):
        # Without the font cache, fc-list outlasts the 150 ms icon probe, so folders draw text marks instead of icons.
        with patch("tui_battery.fixtures.shutil.which", return_value="/usr/bin/bwrap"), \
                patch("tui_battery.fixtures.os.path.isdir", side_effect=lambda path: path == "/var/cache/fontconfig"):
            argv = fixtures.namespace("/opt/candidate/flea", Path("/owned/fx"), "/fx/v/work", {})
        binds = [argv[i + 1:i + 3] for i, option in enumerate(argv) if option == "--ro-bind"]
        self.assertIn(["/var/cache/fontconfig", "/var/cache/fontconfig"], binds)
        self.assertNotIn("/var/cache/fontconfig", [argv[i + 1] for i, option in enumerate(argv) if option == "--bind"])


class TmpfsFixtureTests(unittest.TestCase):
    """The boards' folder totals are the bundle's work tree on tmpfs, so the battery builds each case's fixture there."""

    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="tnative-fx.", dir=parent))
        (self.root / MARKER).write_text("owned tmpfs fixture regressions\n")
        self.addCleanup(remove, self.root)
        self.case = self.root / "case"
        self.case.mkdir()
        (self.case / MARKER).write_text("owned fixture case\n")
        self.shm = Path(tempfile.mkdtemp(prefix="tnative-fx.", dir="/dev/shm"))
        (self.shm / MARKER).write_text("owned tmpfs fixture parent\n")
        self.addCleanup(remove, self.shm)

    def test_the_bundle_folders_on_the_case_fixture_total_the_boards_digits(self):
        # Folder totals on btrfs differ from tmpfs, so the digits are checked against a tmpfs fixture.
        fx, _ = fixtures.build(self.case, "work", dry=True, tmpfs=self.shm)
        seeds = dict.fromkeys(bundle.SEEDS, b"") | {"report.pdf": bytes(bundle.REPORT_BYTES)}
        board = fx / "tmp/board"
        for name, (kind, data, _) in sorted(bundle.work_files(seeds, "v0312").items()):
            if name.split("/")[0] not in ("A", "Code", "Docs", "Empty", "Locked", "Many"):
                continue
            (board / name).parent.mkdir(parents=True, exist_ok=True)
            if kind == "directory":
                (board / name).mkdir()
            else:
                (board / name).write_bytes(data)
        drawn = {name: dynamic.size(total(board / name)) for name in ("A", "Code", "Docs", "Empty", "Many")}
        self.assertEqual(drawn, {"A": "462 B", "Code": "216 B", "Docs": "22.1 kB", "Empty": "40 B", "Many": "2.2 kB"})
        # Flea cannot open Locked, so its total is the folder's own size, drawn as a floor.
        self.assertEqual((board / "Locked").lstat().st_size, 60)

    def test_a_fixture_parent_that_is_not_tmpfs_is_refused_and_left_clean(self):
        with self.assertRaisesRegex(Blocked, r"reports \d+ bytes for an empty folder, not tmpfs's 40"):
            fixtures.build(self.case, "work", dry=True, tmpfs=self.root)
        self.assertEqual([p.name for p in self.root.glob("flea-battery-fx.*")], [])
        self.assertFalse((self.case / "fixtures.json").exists())

    def test_retain_copies_the_fixture_into_the_case_and_frees_the_tmpfs(self):
        fx, _ = fixtures.build(self.case, "work", dry=True, tmpfs=self.shm)
        (fx / "v/work/Locked/secret.txt").write_bytes(b"locked away\n")
        (fx / "v/work/Locked").chmod(0)
        (fx / "v/work/link.txt").symlink_to("notes.txt")
        os.mkfifo(fx / "runtime/pipe")
        fixtures.retain(fx, self.case)
        kept = self.case / "fx/v/work"
        self.assertFalse(fx.parent.exists())
        self.assertEqual(stat.S_IMODE((kept / "Locked").lstat().st_mode), 0)
        (kept / "Locked").chmod(0o700)
        self.assertEqual((kept / "Locked/secret.txt").read_bytes(), b"locked away\n")
        self.assertEqual(os.readlink(kept / "link.txt"), "notes.txt")
        self.assertEqual((kept / "notes.txt").read_bytes(), fixtures.NOTES)
        self.assertFalse((self.case / "fx/runtime/pipe").exists())

    def test_a_failed_copy_keeps_the_tmpfs_copy_and_refuses_to_free_it(self):
        fx, _ = fixtures.build(self.case, "work", dry=True, tmpfs=self.shm)
        self.addCleanup(remove, fx.parent)
        (fx / "v/work/notes.txt").chmod(0)
        with self.assertRaises(Blocked):
            fixtures.retain(fx, self.case)
        self.assertTrue((fx.parent / MARKER).is_file(), "retain freed the only copy of the run's fixture after a failed copy")

    def test_a_retain_failure_downgrades_a_pass_verdict_to_blocked(self):
        copy = driver.retain
        def copy_then_fail(fx, case):
            copy(fx, case)
            raise OSError("copy into the case failed")
        root = self.root / "retained"
        root.mkdir()
        with patch.object(driver, "retain", side_effect=copy_then_fail):
            verdict, _ = pair(root)
        self.assertEqual((verdict["status"], verdict["fixture_retain_reason"]), ("BLOCKED", "copy into the case failed"))

    def test_the_default_build_places_the_fixture_on_tmpfs(self):
        fx, _ = fixtures.build(self.case, "work", dry=True)
        self.addCleanup(remove, fx.parent)
        self.assertEqual(fx.parent.parent, fixtures.TMPFS)

    def test_the_offline_suite_names_a_case_fixture_its_process_left_on_tmpfs(self):
        fx, _ = fixtures.build(self.case, "work", dry=True, tmpfs=self.shm)
        marker = f"battery fixture {os.getpid()}\n"
        with patch.object(offline, "TMPFS", self.shm):
            self.assertEqual(offline.leaked_fixtures(marker), [str(fx.parent)])
            fixtures.retain(fx, self.case)
            self.assertEqual(offline.leaked_fixtures(marker), [])


class PreflightAndSmokeTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="tnative-pre.", dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text("owned native preflight regressions\n")
        self.fake = FakeHyprland(self.root, self.root)

    def preflight(self, during=None, locked=False):
        out, err = io.StringIO(), io.StringIO()
        with ExitStack() as stack:
            stack.enter_context(patch("tui_battery.evidence.subprocess.run", side_effect=self.fake.run))
            stack.enter_context(patch.object(native, "locked", return_value=locked, create=True))
            stack.enter_context(patch("time.sleep", side_effect=lambda _: during and during()))
            stack.enter_context(redirect_stdout(out))
            stack.enter_context(redirect_stderr(err))
            code = driver.main(["--boards", str(self.root), "--preflight"])
        return code, (out.getvalue() or err.getvalue())

    def test_a_still_display_passes_in_seconds(self):
        code, text = self.preflight()
        self.assertEqual(code, 0, text)
        self.assertEqual(json.loads(text)["status"], "PASS")

    def test_a_display_someone_is_using_refuses_before_any_state(self):
        def move():
            self.fake.cursor = {"x": 1290, "y": 700}
        def open_window():
            self.fake.clients.append({"class": "firefox", "address": "0xdef"})
        for name, during, reason in (("pointer", move, "display in use: pointer moved"),
                                     ("window", open_window, "display in use: a window opened or closed")):
            with self.subTest(name):
                self.fake.cursor, self.fake.clients = {"x": 1280, "y": 720}, []
                code, text = self.preflight(during)
                self.assertEqual(code, 2)
                self.assertIn(reason, json.loads(text)["reason"])

    def test_a_locked_or_already_owned_display_refuses(self):
        code, text = self.preflight(locked=True)
        self.assertEqual((code, json.loads(text)["reason"]), (2, "display is locked by hyprlock, so every capture would show the lock screen"))
        self.fake.output_create("FLEA-stale")
        code, text = self.preflight()
        self.assertEqual(code, 2)
        self.assertIn("owned output FLEA-stale from another run", json.loads(text)["reason"])
        self.fake.output_remove("FLEA-stale")
        self.fake.clients.append({"class": "flea.battery.tstale", "address": "0x1"})
        code, text = self.preflight()
        self.assertIn("owned terminal flea.battery.tstale", json.loads(text)["reason"])

    def smoke(self, verdicts, extra=()):
        binary = self.root / "candidate"
        binary.write_bytes(b"\x7fELFprobe")
        binary.chmod(0o700)
        state = {"id": "tui-list-text", "steps": [], "variants": [{"id": "primary", "columns": 130, "rows": 31}]}
        spec = dict(states=[state], primary_ids=[state["id"]], additional_states=[], excluded=[], reserved=[])
        requested = []
        output = self.root / ("smoke-" + str(len(list(self.root.glob("smoke-*")))))
        with ExitStack() as stack:
            stack.enter_context(patch.object(driver, "contract", side_effect=lambda boards, states: requested.append(states) or spec))
            for name, value in (("palette", {}), ("source_identity", {"elf_sha256": "elf"}), ("supplement", [])):
                stack.enter_context(patch.object(driver, name, return_value=value))
            stack.enter_context(patch.object(driver.os, "geteuid", return_value=1000))
            stack.enter_context(patch.object(driver, "Display", return_value=MagicMock()))
            stack.enter_context(patch("tui_battery.preflight.display", return_value={"status": "PASS"}))
            stack.enter_context(patch.object(driver, "execute_pair", side_effect=verdicts))
            stack.enter_context(redirect_stdout(io.StringIO()))
            stack.enter_context(redirect_stderr(io.StringIO()))
            code = driver.main(["--boards", str(self.root), "--binary", str(binary), "--output", str(output), "--smoke", *extra])
        manifest = json.loads((output / "manifest.json").read_text())
        return code, manifest, requested

    def test_smoke_runs_one_listing_per_terminal_and_passes_when_only_the_visual_review_is_left(self):
        def pending(args, root, display, terminal, state, variant, *_):
            return {"terminal": terminal, "state": state["id"], "variant": variant["id"], "status": "BLOCKED",
                    "behavior": {"status": "PASS"}, "cells": {"geometry": {"status": "PASS"}, "color": {"status": "PASS"}},
                    "media": {"status": "PASS"}, "edges": {"status": "PASS", "sides": {}}, "pixels": {"status": "REVIEW"},
                    "visual": {"status": "BLOCKED", "reason": "controller native visual review required"}}
        code, manifest, requested = self.smoke(pending)
        self.assertEqual(requested, ["tui-list-text", None])
        self.assertEqual([r["terminal"] for r in manifest["records"]], list(adapters.TERMINALS))
        self.assertEqual((code, manifest["status"], manifest["smoke"], manifest["native_acceptance"]), (0, "SMOKE-PASS", True, False))
        def failing(*arguments):
            verdict = pending(*arguments)
            if verdict["terminal"] == "kitty":
                verdict.update(status="FAIL", shutdown_reason="candidate exit or TTY restoration failed")
            return verdict
        code, manifest, _ = self.smoke(failing)
        self.assertEqual((code, manifest["status"]), (1, "FAIL"))

    def test_a_smoke_run_never_promotes_a_verdict_whose_fixture_was_not_retained(self):
        def unretained(args, root, display, terminal, state, variant, *_):
            return {"terminal": terminal, "state": state["id"], "variant": variant["id"], "status": "BLOCKED",
                    "behavior": {"status": "PASS"}, "cells": {"geometry": {"status": "PASS"}, "color": {"status": "PASS"}},
                    "media": {"status": "PASS"}, "edges": {"status": "PASS", "sides": {}}, "pixels": {"status": "REVIEW"},
                    "visual": {"status": "BLOCKED", "reason": "controller native visual review required"},
                    "fixture_retain_reason": "copy into the case failed"}
        code, manifest, _ = self.smoke(unretained)
        self.assertEqual((code, manifest["status"]), (2, "BLOCKED"))

    def test_smoke_refuses_a_state_filter(self):
        with redirect_stderr(io.StringIO()) as err:
            code = driver.main(["--boards", str(self.root), "--smoke", "--states", "tui-empty", "--list"])
        self.assertEqual(code, 2)
        self.assertIn("--smoke selects its own state", err.getvalue())


if __name__ == "__main__":
    unittest.main()
