"""Permanent five-board regression adapted from the round-three review probe."""
import copy
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from .dynamic import disk_field, masks
from .evidence import Blocked, MARKER
from .references import Board
from .cells import Screen
from .media import inspect as inspect_media
from .review_support import board_file

def boards_directory():
    value = os.environ.get("FLEA_BATTERY_BOARDS")
    if not value:
        raise Blocked("FLEA_BATTERY_BOARDS is unset: set it to the design-hig directory that holds boards/")
    return Path(value) / "boards"
NAMES = ("tui-list-text", "tui-lang-ja", "tui-lang-ja-menu", "tui-lang-ar", "tui-lang-ru-keys")


def comparison(board, variant):
    cells = board.cells()
    snapshot = {"columns": 130, "rows": 31, "grid": [
        [dict(cells[x, y], reverse=False) for x in range(130)] for y in range(31)]}
    field = disk_field(snapshot["grid"][30])
    backend = {"path": "/fx/v/work", "fs": field["fs"], "free": 64100000}
    row = snapshot["grid"][30]
    if variant == "wrong-position":
        start, end = field["left"], field["right"]
        contents = copy.deepcopy(row[start:end])
        assert not row[start - 1]["ch"].strip()
        for x in range(start - 1, end):
            row[x] = dict(ch=" ", width=1, fg=board.ink("var(--fg)"), bg=board.ink("var(--bg)"),
                          bold=False, dim=False, underline=False, reverse=False)
        row[start - 1:end - 1] = contents
    if variant == "wrong-rendition":
        for cell in row[field["left"]:field["right"]]:
            cell.update(fg="#ff0000", bg="#00ff00", reverse=True, dim=True, underline=True, bold=True)
    if variant == "wrong-count":
        row[0]["ch"] = "!"
    if variant == "wrong-backend":
        backend["free"] = 5000
    if variant == "verified-change":
        for x in range(field["left"], field["right"] - 6):
            token = "".join(c["ch"] for c in row[x:x + 7])
            if token in ("64.1 MB", "64,1 MB"):
                row[x + 3]["ch"] = "2"
                break
        else:
            raise AssertionError("missing reference value")
        backend["free"] = 64200000
    unmasked = board.compare(snapshot)
    try:
        with patch("tui_battery.dynamic.fsinfo", return_value=backend):
            exemptions, facts = masks(board, snapshot, [], "/fx/v/work", None)
        result = board.compare(snapshot, masks=exemptions)
        return dict(result, status="COMPARED", without_mask=unmasked, exemptions=exemptions, facts=facts)
    except Blocked as error:
        return {"status": "BLOCKED", "reason": str(error), "without_mask": unmasked}


class Review3Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.boards = boards_directory()
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", Path(__file__).resolve().parents[2] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        cls.scratch = Path(tempfile.mkdtemp(prefix="review-r3.", dir=parent))
        (cls.scratch / MARKER).write_text("third-round regression controls\n")

    def test_reviewer_shifted_field_probe(self):
        for name in NAMES:
            with self.subTest(board=name):
                result = comparison(Board(self.boards / (name + ".html")), "wrong-position")
                self.assertEqual(result["without_mask"]["geometry"]["status"], "FAIL")
                self.assertEqual(result["status"], "COMPARED")
                self.assertEqual(result["geometry"]["status"], "FAIL")

    def test_reviewer_wrong_rendition_probe(self):
        for name in NAMES:
            with self.subTest(board=name):
                result = comparison(Board(self.boards / (name + ".html")), "wrong-rendition")
                self.assertEqual(result["without_mask"]["color"]["status"], "FAIL")
                self.assertEqual(result["color"]["status"], "FAIL")

    def test_reviewer_matching_count_and_backend_controls(self):
        for name in NAMES:
            with self.subTest(board=name):
                board = Board(self.boards / (name + ".html"))
                for variant in ("matching", "verified-change"):
                    result = comparison(board, variant)
                    self.assertEqual(result["geometry"]["status"], "PASS")
                    self.assertEqual(result["color"]["status"], "PASS")
                self.assertEqual(comparison(board, "wrong-count")["geometry"]["status"], "FAIL")
                backend = comparison(board, "wrong-backend")
                self.assertEqual(backend["status"], "BLOCKED")
                self.assertEqual(backend["reason"], "native disk facts differ from the real backend reply")

    def test_individual_rendition_and_width_remain_checked(self):
        for name in NAMES:
            board = Board(self.boards / (name + ".html"))
            cells = board.cells()
            snapshot = {"columns": 130, "rows": 31, "grid": [
                [dict(cells[x, y], reverse=False) for x in range(130)] for y in range(31)]}
            field = disk_field(snapshot["grid"][30])
            with patch("tui_battery.dynamic.fsinfo", return_value={"fs": field["fs"], "free": 64100000}):
                exemptions, _ = masks(board, snapshot, [], "/fx/v/work", None)
            space = next(x for x in range(field["left"], field["right"]) if snapshot["grid"][30][x]["ch"] == " ")
            for x in (field["left"], space):
                for key, value in (("fg", "#ff0000"), ("bg", "#00ff00"), ("reverse", True),
                                   ("bold", True), ("dim", True), ("underline", True)):
                    with self.subTest(board=name, cell=x, attribute=key):
                        actual = copy.deepcopy(snapshot)
                        actual["grid"][30][x][key] = value
                        self.assertEqual(board.compare(actual, masks=exemptions)["color"]["status"], "FAIL")
                actual = copy.deepcopy(snapshot)
                actual["grid"][30][x]["width"] = 2
                self.assertEqual(board.compare(actual, masks=exemptions)["geometry"]["status"], "FAIL")

    def test_legacy_rectangle_cannot_bypass_comparison(self):
        root = Path(tempfile.mkdtemp(prefix="rectangle.", dir=self.scratch))
        (root / MARKER).write_text("rectangle rejection\n")
        board = board_file(root)
        screen = Screen(10, 4)
        screen.feed(b"\x1b[H\x1b[31mX")
        with self.assertRaises(Blocked):
            board.compare(screen.snapshot(), masks=[[0, 0, 1, 1]])

    def test_unverified_media_cannot_exempt_a_rectangle(self):
        root = Path(tempfile.mkdtemp(prefix="media.", dir=self.scratch))
        (root / MARKER).write_text("media mask rejection\n")
        board = board_file(root)
        board.pictures = [{"origin": [20.8, 53], "resolved_style": {"width": "16.8px", "height": "22px"}}]
        snapshot = Screen(10, 4).snapshot()
        snapshot["grid"][2][2].update(ch="▀", fg="#ff0000", bg="#00ff00", bold=True)
        caps = {"candidate_received": {"kitty_positive": False, "sixel_positive": False}, "forced_fallback": False}
        review = {"representation": {"approved": True, "mode": "half-blocks", "fixture_sha256": None, "bounds": [[2, 2, 2, 1]]}}
        with self.assertRaises(Blocked):
            inspect_media({"id": "tui-office", "cwd": "/fx/v/work"}, board, snapshot,
                          (100, 100, bytes([0, 0, 0, 255]) * 10000), [4, 9, 84, 88], caps,
                          root, root, root, review, adapter="alacritty", expected_transport="half-blocks")


class BoardsVariableTests(unittest.TestCase):
    def test_unset_boards_variable_refuses_by_name(self):
        environment = {key: value for key, value in os.environ.items() if key != "FLEA_BATTERY_BOARDS"}
        with patch.dict(os.environ, environment, clear=True):
            with self.assertRaisesRegex(Blocked, "FLEA_BATTERY_BOARDS"):
                Review3Tests.setUpClass()


if __name__ == "__main__":
    unittest.main()
