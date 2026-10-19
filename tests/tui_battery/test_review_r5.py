"""The real oracle CLI must request Chafa's polite, cursor-free output."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from .evidence import Blocked, Commands, MARKER
from .fallback import STATES
from . import test_review_r4 as r4


class Review5Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", Path(__file__).resolve().parents[2] / ".superpowers/t12a"))
        cls.root = Path(tempfile.mkdtemp(prefix="review-r5.", dir=parent))
        (cls.root / MARKER).write_text("Chafa polite-mode regression\n")

    def test_polite_flag_suppresses_default_cursor_controls_and_keeps_strict_parser(self):
        helper = r4.Review4Tests()
        helper.root = self.root
        real_run = Commands.run
        receipts = []
        for name in sorted(STATES - {"tui-combine"}):
            with self.subTest(state=name):
                fixture = helper.fixture(name, approved_board=True)
                default_outputs = []
                for explicit_off in (False, True):
                    def without_polite(commands, argv, *args, **kwargs):
                        actual = [a for a in argv if a != "--polite=on"]
                        if explicit_off and "--format=symbols" in actual:
                            actual.insert(1, "--polite=off")
                        result = real_run(commands, actual, *args, **kwargs)
                        if "--format=symbols" in actual:
                            default_outputs.append(result.stdout)
                        return result
                    with patch.object(Commands, "run", without_polite):
                        with self.assertRaisesRegex(Blocked, "chafa source output holds characters outside the half-block set"):
                            helper.inspect(fixture)
                self.assertEqual(len(default_outputs), 2)
                for output in default_outputs:
                    self.assertTrue(output.startswith(b"\x1b[?25l"))
                    self.assertTrue(output.endswith(b"\x1b[?25h"))
                pending, _, expected, commands = helper.inspect(fixture)
                renderer = next(row for row in commands.records if "--format=symbols" in row["argv"])
                self.assertIn("--polite=on", renderer["argv"])
                self.assertNotIn("\x1b[?25", renderer["stdout"])
                for output in default_outputs:
                    clean = output.removeprefix(b"\x1b[?25l").removesuffix(b"\x1b[?25h")
                    self.assertEqual(clean, renderer["stdout"].encode())
                for area in expected:
                    for entry in area["cells"]:
                        x, y = entry["cell"]
                        fixture[4]["grid"][y][x] = dict(entry["expected"], reverse=False)
                decision = {"representation": dict(pending["review_requirements"], approved=True)}
                media, _, expected, _ = helper.inspect(fixture, decision)
                compared = fixture[2].compare(fixture[4], masks=expected)
                self.assertEqual(media["status"], "PASS")
                self.assertEqual(compared["geometry"]["status"], "PASS")
                self.assertEqual(compared["color"]["status"], "PASS")
                receipts.append({"state": name, "default_cursor_controls": "REJECTED", "explicit_off": "REJECTED",
                                 "polite_cursor_controls": "ABSENT", "geometry": "PASS", "color": "PASS"})
        self.assertEqual(len(receipts), 7)
        _, _, _, commands = helper.inspect(fixture)
        renderer = next(row for row in commands.records if "--format=symbols" in row["argv"])
        size = next(arg for arg in renderer["argv"] if arg.startswith("--size="))
        picture_width, picture_height = (int(part) for part in size.removeprefix("--size=").split("x"))
        picture = ("▀" * picture_width + "\n").encode()

        def frame(prefix):
            # One picture of the approved height whose first row starts with the prefix.
            first = prefix + ("▀" * (picture_width - len(prefix))).encode() + b"\n"
            return first + picture * (picture_height - 1)

        def emitted(payload):
            def output(commands, argv, *args, **kwargs):
                result = real_run(commands, argv, *args, **kwargs)
                if "--format=symbols" in argv:
                    result.stdout = payload
                return result
            return output

        with self.subTest(control="unmodified picture"):
            # The control proves the frame shape is accepted, so each refusal below is its own branch.
            with patch.object(Commands, "run", emitted(picture * picture_height)):
                helper.inspect(fixture)
        charset = "holds characters outside the half-block set"
        graphics = "contains unparsed or native graphics controls"
        size = "exceeds the approved picture size"
        refusals = ((b"\x1b[?25l", charset), (b"\x1b[?25h", charset), (b"\x1b[2J", charset),
                    (b"\x1b]0;title\x07", charset), (b"\t", charset),
                    (b"\x1b[8m", graphics), (b"X" * (picture_width + 1), charset))
        for extra, branch in refusals:
            with self.subTest(unsupported=extra, branch=branch):
                with patch.object(Commands, "run", emitted(frame(extra))):
                    with self.assertRaisesRegex(Blocked, branch):
                        helper.inspect(fixture)
        overflows = {"width": ("▀" * (picture_width + 1) + "\n").encode() + picture * (picture_height - 1),
                     "height": picture * (picture_height + 1)}
        for axis, payload in overflows.items():
            with self.subTest(overflow=axis):
                with patch.object(Commands, "run", emitted(payload)):
                    with self.assertRaisesRegex(Blocked, size):
                        helper.inspect(fixture)
        (self.root / "chafa-controls.json").write_text(json.dumps(receipts, indent=2) + "\n")


if __name__ == "__main__":
    unittest.main()
