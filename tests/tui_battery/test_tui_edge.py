"""Native window placement and the TUI ground against the terminal's own background."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from . import native
from .adapters import Adapter
from .edges import ground_edges
from .evidence import Blocked, Commands, MARKER
from .review_support import pair

HERE = Path(__file__).resolve().parent
# The synthetic window: 4 px of padding left, 10 top, a 13 px strip right and 20 px bottom around a 1040x682 grid.
GRID = [4, 10, 1040, 682]
CLIENT = [GRID[0] + GRID[2] + 13, GRID[1] + GRID[3] + 20]
GROUND = (18, 18, 18)


def capture(padding=GROUND, side=None, corners=None):
    """A capture of one owned window whose padding takes `padding` on `side`, or on every side when `side` is None."""
    width, height = CLIENT
    left, top, grid_width, grid_height = GRID
    right, bottom = left + grid_width, top + grid_height
    strips = {"left": (0, left, top, bottom), "top": (left, right, 0, top), "right": (right, width, top, bottom),
              "bottom": (left, right, bottom, height)}
    # A band on every side also takes the four corner blocks between the strips.
    painted = [strips[side]] if side else [*strips.values(), (0, left, 0, top), (right, width, 0, top),
                                           (0, left, bottom, height), (right, width, bottom, height)]
    pixels = bytearray(bytes((*GROUND, 255)) * (width * height))
    for x0, x1, y0, y1 in painted:
        for y in range(y0, y1):
            pixels[(y * width + x0) * 4:(y * width + x1) * 4] = bytes((*padding, 255)) * (x1 - x0)
    if corners:
        radius = native.WINDOW_ROUNDING
        for y in (*range(radius), *range(height - radius, height)):
            for x in (*range(radius), *range(width - radius, width)):
                pixels[(y * width + x) * 4:(y * width + x) * 4 + 4] = bytes((*corners, 255))
    return (width, height, bytes(pixels)), CLIENT, GRID


class TuiEdgeTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="tui-edge.", dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text("owned TUI edge regressions\n")

    def launch(self, extra):
        case = self.root / ("case-" + str(len(list(self.root.glob("case-*")))))
        case.mkdir()
        spec = case / "launch-spec.json"
        probe = [sys.executable, "-I", "-c", "import json, os; print(json.dumps(dict(os.environ)))"]
        spec.write_text(json.dumps({"argv": probe, "environment": {"PATH": "/usr/bin:/bin", "HOME": str(case)}}))
        environment = dict(PATH="/usr/bin:/bin", PYTHONPATH=str(HERE.parent), PYTHONDONTWRITEBYTECODE="1",
                           FLEA_PARENT_ONLY="never reaches the terminal", **extra)
        result = subprocess.run([sys.executable, "-B", "-m", "tui_battery.native", "--launch", str(spec)], env=environment,
                                stdin=subprocess.DEVNULL, capture_output=True, timeout=60)
        self.assertEqual(result.returncode, 0, result.stderr.decode(errors="replace"))
        return json.loads((case / "terminal.stdout").read_text()), json.loads((case / "launch-start.json").read_text())

    def test_launch_hands_hyprlands_exec_rule_token_to_the_terminal_and_nothing_else(self):
        # Hyprland 0.56.2 matches exec_cmd's workspace, monitor and float rules by this variable in the window's process.
        token = "4f1c2b7e-9d3a-4c55-8e21-0b6a7d9f3c10"
        child, start = self.launch({"HL_EXEC_RULE_TOKEN": token})
        self.assertEqual(child.get("HL_EXEC_RULE_TOKEN"), token)
        self.assertNotIn("FLEA_PARENT_ONLY", child)
        self.assertEqual(child["HOME"], str(self.root / "case-0"))
        self.assertIs(start["exec_rule_token"], True)
        child, start = self.launch({})
        self.assertNotIn("HL_EXEC_RULE_TOKEN", child)
        self.assertNotIn("FLEA_PARENT_ONLY", child)
        self.assertIs(start["exec_rule_token"], False)

    def verdict(self, name, frame):
        root = self.root / name
        root.mkdir()
        return pair(root, frame=frame)[0]

    def test_padding_that_matches_the_ground_passes_even_past_rounded_corners(self):
        for name, frame in (("equal", capture()), ("within-two", capture((20, 16, 19))),
                            ("rounded-corners", capture(corners=(120, 60, 30)))):
            with self.subTest(name):
                verdict = self.verdict(name, frame)
                self.assertEqual(verdict["edges"]["status"], "PASS", verdict["edges"])
                self.assertEqual(verdict["status"], "PASS", verdict.get("reason"))
                self.assertEqual(verdict["edges"]["ground"], list(GROUND))
                self.assertTrue(all(side["pixels"] > 0 for side in verdict["edges"]["sides"].values()))

    def test_a_band_on_any_side_fails_the_state_and_names_its_edge(self):
        for side in ("left", "top", "right", "bottom"):
            with self.subTest(side):
                verdict = self.verdict("band-" + side, capture((21, 18, 18), side))
                self.assertEqual(verdict["status"], "FAIL")
                self.assertEqual(verdict["reason"], f"TUI ground differs from the terminal background at the {side} edge")
                self.assertEqual(verdict["edges"]["sides"][side]["delta"], 3)
        verdict = self.verdict("band-all", capture((36, 36, 36)))
        self.assertEqual(verdict["reason"], "TUI ground differs from the terminal background at the left edge")
        self.assertEqual({side: value["delta"] for side, value in verdict["edges"]["sides"].items()},
                         {"left": 18, "top": 18, "right": 18, "bottom": 18})

    def test_a_grid_past_every_client_edge_is_blocked_not_passed(self):
        image = (40, 30, bytes((*GROUND, 255)) * (40 * 30))
        with self.assertRaises(Blocked):
            ground_edges(image, [0, 0, 40, 30], [-5, -5, 50, 40], (8, 22), native.WINDOW_ROUNDING)
        self.assertEqual(ground_edges(image, [0, 0, 40, 30], [0, 0, 40, 30], (8, 22), native.WINDOW_ROUNDING),
                         {"status": "PASS", "sides": {}})


# Stand-ins for each terminal's own checker: foot 1.28's verdict on a [colors] section, ghostty's stdout diagnostic, kitty's stderr log.
FAKE_CHECKERS = {
    "foot": ('[ "$1" = --check-config ] || exit 2\n'
             'if grep -qx "\\[colors\\]" "$3"; then echo "warn: a warning first" >&2\n'
             '  echo "err: config.c:3283: $3:5: [colors]: invalid section name: colors" >&2; exit 230; fi\n'),
    "ghostty": ('[ "$1" = +validate-config ] || exit 2\n'
                'if grep -q "^bogus" "${2#--config-file=}"; then echo "${2#--config-file=}:9:bogus: unknown field"; exit 1; fi\n'),
    "kitty": ('[ "$1" = +runpy ] || exit 2\n'
              'if grep -q "^bogus" "$3"; then echo "[0.012] Ignoring unknown config key: bogus" >&2; fi\n'),
}


class TerminalConfigTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="terminal-config.", dir=parent))
        self.addCleanup(shutil.rmtree, self.root, ignore_errors=True)
        (self.root / MARKER).write_text("owned terminal config regressions\n")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name, body in FAKE_CHECKERS.items():
            (self.bin / name).write_text("#!/bin/sh\n" + body)
            (self.bin / name).chmod(0o700)

    def adapter(self, name):
        directory = self.root / name
        directory.mkdir()
        adapter = Adapter(name, directory, "owned", {"fg": "#c6b395", "bg": "#1c120d"})
        adapter.configure()
        return adapter

    def validate(self, adapter, commands=None):
        return adapter.validate(commands or Commands(), str(self.bin), {"PATH": str(self.bin) + ":/usr/bin:/bin"})

    def test_foot_and_tmux_write_the_colour_section_foot_1_28_reads(self):
        for name in ("foot", "tmux"):
            with self.subTest(name):
                lines = self.adapter(name).config.read_text().splitlines()
                self.assertIn("[colors-dark]", lines)
                self.assertNotIn("[colors]", lines)
                section = lines[lines.index("[colors-dark]") + 1:]
                for key in ("background=1c120d", "foreground=c6b395", "regular0=", "regular7=", "bright0=", "bright7="):
                    self.assertTrue(any(line.startswith(key) for line in section), key)

    def test_each_terminal_checks_the_config_it_wrote_before_any_run(self):
        for name in ("foot", "tmux", "ghostty", "kitty"):
            with self.subTest(name):
                commands = Commands()
                receipt = self.validate(self.adapter(name), commands)
                self.assertEqual(receipt["status"], "PASS")
                self.assertEqual([Path(record["argv"][0]).name for record in commands.records], ["kitty" if name == "kitty"
                                 else "ghostty" if name == "ghostty" else "foot"])
                self.assertEqual(commands.records[0]["argv"][-1].removeprefix("--config-file="), str(self.root / name / "terminal.conf"))
        commands = Commands()
        receipt = self.validate(self.adapter("alacritty"), commands)
        self.assertEqual((receipt["status"], commands.records), ("UNCHECKED", []))

    def test_a_config_its_terminal_rejects_is_blocked_with_the_checkers_first_error_line(self):
        cases = {"foot": "[colors]", "tmux": "[colors]", "ghostty": "bogus = 1", "kitty": "bogus 1"}
        for name, line in cases.items():
            with self.subTest(name):
                adapter = self.adapter(name)
                text = adapter.config.read_text()
                adapter.config.write_text(text.replace("[colors-dark]", line) if name in ("foot", "tmux") else text + line + "\n")
                with self.assertRaises(Blocked) as raised:
                    self.validate(adapter)
                expected = {"foot": f"err: config.c:3283: {adapter.config}:5: [colors]: invalid section name: colors",
                            "ghostty": f"{adapter.config}:9:bogus: unknown field",
                            "kitty": "[0.012] Ignoring unknown config key: bogus"}[name if name != "tmux" else "foot"]
                executable = "foot" if name == "tmux" else name
                self.assertEqual(str(raised.exception), f"{executable} rejects the private terminal config: {expected}")

    def test_a_rejected_config_blocks_the_state_before_its_terminal_launches(self):
        def reject(*_):
            raise Blocked("foot rejects the private terminal config: err: invalid section name: colors")
        root = self.root / "pair"
        root.mkdir()
        launches = []
        verdict, _ = pair(root, validate=reject, launches=launches)
        self.assertEqual(verdict["status"], "BLOCKED")
        self.assertEqual(verdict["reason"], "foot rejects the private terminal config: err: invalid section name: colors")
        self.assertEqual(launches, [], "the terminal launched although its config was rejected")
        self.assertFalse((root / "foot/tui-list-text/primary/launch-spec.json").exists())


class DisplayModeTests(unittest.TestCase):
    # Sample: Hyprland 0.56.2 reports a new headless output at 1920x1080 scale 1.6 until it applies the mode about a second later.
    def enter(self, reports, query_failure=None):
        default = {"name": "FLEA-id", "id": 7, "width": 1920, "height": 1080, "scale": 1.6}
        applied = dict(default, width=2560, height=1440, scale=1)
        # The first read is the in-use check; None never applies the mode, and a query_failure fails the first poll.
        reads = [[], [default]] if reports is None else [[]] + [[default]] * reports + [[applied]]
        if query_failure:
            reads = [[], query_failure]
        def answer(argv, **_):
            function = argv[5]
            code, stdout = {"hypr_query_help": (1, b"commands: monitors clients output version"),
                            "hypr_query_output_help": (1, b"output create remove"),
                            "hypr_query_option": (0, b'{"int": 0}')}.get(function, (0, b""))
            if function == "hypr_query_monitors":
                read = reads.pop(0) if len(reads) > 1 else reads[0]
                if isinstance(read, str):
                    return subprocess.CompletedProcess(argv, 1, b"", read.encode())
                stdout = json.dumps(read).encode()
            return subprocess.CompletedProcess(argv, code, stdout, b"")
        commands = Commands()
        with patch("tui_battery.evidence.subprocess.run", side_effect=answer), patch.object(native, "MODE_DEADLINE", 0.5, create=True):
            try:
                with native.Display(commands, "id") as display:
                    return display.monitor, None, commands
            except Blocked as error:
                return None, str(error), commands

    def test_the_owned_output_is_verified_once_hyprland_applies_its_mode(self):
        monitor, error, commands = self.enter(reports=3)
        self.assertIsNone(error)
        self.assertEqual((monitor["width"], monitor["height"], monitor["scale"]), (2560, 1440, 1))
        self.assertEqual([r["argv"][5] for r in commands.records][-1], "hypr_output_remove")

    def test_a_mode_hyprland_never_applies_blocks_with_the_mode_it_last_reported(self):
        monitor, error, commands = self.enter(reports=None)
        self.assertIsNone(monitor)
        self.assertEqual(error, "owned output mode/scale could not be verified: last seen 1920x1080 scale 1.6")
        self.assertEqual([r["argv"][5] for r in commands.records][-1], "hypr_output_remove")

    def test_a_failed_monitor_query_reaches_the_caller_with_its_own_reason(self):
        monitor, error, commands = self.enter(reports=None, query_failure="Hyprland socket is gone")
        self.assertIsNone(monitor)
        self.assertEqual(error, "command exited 1: bash: Hyprland socket is gone")
        self.assertEqual([r["argv"][5] for r in commands.records][-1], "hypr_output_remove")


if __name__ == "__main__":
    unittest.main()
