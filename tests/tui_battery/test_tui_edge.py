"""Native window placement and the TUI ground against the terminal's own background."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

from .evidence import MARKER
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
        for y in (*range(8), *range(height - 8, height)):
            for x in (*range(8), *range(width - 8, width)):
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


if __name__ == "__main__":
    unittest.main()
