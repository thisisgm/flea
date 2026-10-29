"""Synthetic native-boundary fixtures drive real picture observation and scenario dispatch, with no display access."""
import base64
import json
import os
from pathlib import Path
from types import SimpleNamespace
import tempfile
import time
import unittest

from . import candidate, png, scenarios
from .evidence import Blocked, Commands, MARKER
from .review_support import media_transcript

COLORS = [(255, 0, 0, 255), (0, 255, 0, 255), (0, 0, 255, 255), (255, 255, 255, 255)]


def pixels(phase=0, width=4, height=6):
    return bytes(value for y in range(height) for x in range(width) for value in COLORS[(x+y+phase) % 4])


def native_terminal(root):
    terminal = SimpleNamespace(case=root, snapshot=lambda: {"grid": []}, rgba=pixels(), input_serial=0)
    terminal.fixture, terminal.commands = root / "fx", Commands()
    for name in ("tmp", "home", "config", "cache", "state", "runtime", "data", "tools"):
        (terminal.fixture / name).mkdir(parents=True, exist_ok=True)
    terminal.picture_geometry = {"native_rect": [0, 0, 4, 6], "pixels": [4, 6], "cells": [0, 0, 4, 6],
                                 "column_cells": [4, 0, 2, 6]}
    def capture(path):
        png.write(path, 4, 6, terminal.rgba)
        return 4, 6, terminal.rgba
    terminal.display = SimpleNamespace(capture=capture)
    return terminal


def kitty(terminal, ident, rgba, width=4):
    terminal.rgba = rgba if width == 4 else pixels()
    path = terminal.case / ("input-" + str(terminal.input_serial) + ".png")
    terminal.input_serial += 1
    png.write(path, width, 6, rgba)
    token = "\x1b_Ga=T,f=100,i=" + str(ident) + ";" + base64.b64encode(path.read_bytes()).decode() + "\x1b\\"
    media_transcript(terminal.case, [{"ns": 30, "data": token}])


class BoundaryTerminal:
    def __init__(self, root, stuck=False):
        self.case, self.phase, self.requested, self.pending, self.partial = root, None, None, 0, False
        self.keys, self.captured, self.stuck = [], [], stuck
        self.picture_geometry = {"native_rect": [0, 0, 4, 6], "pixels": [4, 6], "cells": [0, 0, 4, 6],
                                 "column_cells": [4, 0, 2, 6]}
        (root / "picture-config.json").write_text(json.dumps(self.picture_geometry))
        self.display = SimpleNamespace(capture=self.capture)
        (root / "pty.log").write_text("")
        self.event("output", b"column picture")

    def event(self, direction, data):
        with (self.case / "pty.log").open("a") as stream:
            stream.write(json.dumps({"ns": time.time_ns(), "direction": direction,
                                     "base64": base64.b64encode(data).decode()}) + "\n")

    def key(self, key):
        name = key["key"]
        self.keys.append(name)
        if name == "Space":
            self.requested, self.pending = 0, 2
        elif name == "Right":
            if self.phase is None or not self.captured or self.captured[-1] != self.phase:
                raise AssertionError("Right preceded the actual initial Quick Look picture")
            self.requested, self.pending = self.phase + 15, 1
        self.event("terminal-input", b"\x1b[C" if name == "Right" else b" ")

    def snapshot(self):
        if self.pending:
            self.pending -= 1
        elif self.requested != self.phase:
            self.phase = self.requested
            self.partial = True
            self.event("output", b"\x1b[H" + "▀▄".encode())
        elif self.partial:
            self.partial = False
            self.event("output", b"\x1b[29;10H" + ("└" + "─"*110 + "┘").encode() + b"\x1b[0m")
        grid = [[{"ch": "▀" if (x < 4 if self.phase is not None else 4 <= x < 6) and y < (1 if self.partial else 6) else " ",
                  "fg": "#" + bytes(COLORS[(x+y+(self.phase or 0)//15) % 4][:3]).hex(),
                  "bg": "#" + bytes(COLORS[(x+y+(self.phase or 0)//15) % 4][:3]).hex()}
                 for x in range(130)] for y in range(31)]
        if not self.partial:
            for x, char in enumerate("└" + "─"*110 + "┘", 9):
                grid[28][x]["ch"] = char
        return {"grid": grid, "columns": 130, "rows": 31, "pending": False,
                "cursor": [4, 0] if self.partial else [121, 28]}

    def capture(self, path):
        self.captured.append(self.phase)
        image = pixels(0 if self.stuck else self.phase // 15)
        png.write(path, 4, 6, image)
        return 4, 6, image


class PictureDriverTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-picture.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned synthetic picture boundary\n")

    def state(self):
        return {"id": "tui-3d-quicklook", "steps": [{"action": "preview"}, {"key": "Right"}, {"key": "Right"}],
                "picture_steps": {"orbit_degrees": 30, "presses": 2, "key": "Right", "plate": 1}}

    def test_configuration_uses_the_actual_board_and_native_cell_calibration(self):
        from .bundle.pictures import configure
        from .references import Board
        boards = Path(os.environ["FLEA_BATTERY_BOARDS"]) / "boards"
        terminal = SimpleNamespace(case=self.root)
        configure(terminal, Board(boards / "tui-3d-quicklook.html"), Board(boards / "tui-3d-column.html"),
                  [4, 9, 1040, 682], [8, 22])
        self.assertEqual(terminal.picture_geometry["native_rect"], [196, 141, 656, 462])
        self.assertEqual(terminal.picture_geometry["cells"], [24, 6, 82, 21])
        self.assertEqual(json.loads((self.root / "picture-config.json").read_text()), terminal.picture_geometry)

    def test_dispatch_waits_for_initial_picture_then_observes_each_real_changed_capture(self):
        terminal, journal = BoundaryTerminal(self.root), []
        scenarios.steps(terminal, self.state(), self.root, SimpleNamespace(resolve=lambda action: {"key": "Space"}), journal, 1)
        self.assertEqual(terminal.keys, ["Space", "Right", "Right"])
        self.assertEqual(terminal.captured[0], 0)
        self.assertEqual(terminal.captured[-1], 30)
        self.assertTrue(any(phase == 15 for phase in terminal.captured))
        self.assertEqual(candidate.picture_steps(self.state(), journal, self.root / "pty.log")["status"], "PASS")

    def test_dispatch_refuses_unchanged_presented_pixels(self):
        terminal, journal = BoundaryTerminal(self.root, stuck=True), []
        with self.assertRaises(Blocked):
            scenarios.steps(terminal, self.state(), self.root, SimpleNamespace(resolve=lambda action: {"key": "Space"}), journal, .2)
        self.assertEqual(terminal.keys, ["Space", "Right"])

    def test_partial_ansi_redraw_cannot_become_initial_picture(self):
        from .cells import Screen
        screen = Screen(130, 31)
        partial = b"\x1b[H"
        for r,g,b,_ in COLORS:
            partial += f"\x1b[38;2;{r};{g};{b};48;2;{r};{g};{b}m".encode() + "▀".encode()
        screen.feed(partial)
        (self.root / "pty.log").write_text(json.dumps({"ns": time.time_ns(), "direction": "output",
            "base64": base64.b64encode(partial).decode()}) + "\n")
        rgba = bytes(value for color in COLORS for value in color) + bytes([0,0,0,255])*20
        def capture(path):
            png.write(path, 4, 6, rgba)
            return 4,6,rgba
        terminal = SimpleNamespace(case=self.root, snapshot=screen.snapshot, display=SimpleNamespace(capture=capture),
            picture_geometry={"native_rect":[0,0,4,6], "pixels":[4,6], "cells":[0,0,4,6], "column_cells":[4,0,2,6]})
        with self.assertRaisesRegex(Blocked, "final card edge"):
            candidate.picture_frame(terminal)

    def test_empty_wide_cell_continuations_are_not_picture_pixels(self):
        terminal = BoundaryTerminal(self.root)
        terminal.phase = terminal.requested = 0
        snapshot = terminal.snapshot()
        for row in snapshot["grid"][:6]:
            for cell in row[:4]:
                cell["ch"] = ""
        terminal.snapshot = lambda: snapshot
        with self.assertRaisesRegex(Blocked, "historical column"):
            candidate.picture_frame(terminal)

    def test_equivalent_sixel_encodings_do_not_become_changed_pixels(self):
        terminal = native_terminal(self.root)
        terminal.rgba = bytes(value for _ in range(6) for color in COLORS for value in color)
        def observe(repeat):
            token = '\x1bPq"1;1;4;6' + ''.join(f"#{i};2;{r};{g};{b}#" + str(i) + repeat
                  for i, (r,g,b) in enumerate([(100,0,0),(0,100,0),(0,0,100),(100,100,100)])) + '\x1b\\'
            media_transcript(self.root, [{"ns": 30, "data": token}], kitty=False, sixel=True)
            return candidate.picture_frame(terminal)
        first, second = observe("~"), observe("!1~")
        self.assertEqual(first["sha256"], second["sha256"])
        self.assertNotEqual(first["capture"], second["capture"])

    def test_sixel_receipt_names_the_magick_run_and_rechecks_its_raster(self):
        terminal = native_terminal(self.root)
        terminal.rgba = bytes(value for _ in range(6) for color in COLORS for value in color)
        token = '\x1bPq"1;1;4;6' + ''.join(f"#{i};2;{r};{g};{b}#" + str(i) + "~"
              for i, (r,g,b) in enumerate([(100,0,0),(0,100,0),(0,0,100),(100,100,100)])) + '\x1b\\'
        media_transcript(self.root, [{"ns": 30, "data": token}], kitty=False, sixel=True)
        receipt = candidate.picture_frame(terminal)["native_source"]
        records = terminal.commands.records
        self.assertEqual([row["argv"][row["argv"].index("/opt/flea") - 1] for row in records],
                         ["/usr/bin/sixel2png", "/usr/bin/magick"])
        self.assertEqual(receipt["command"], 1)
        self.assertEqual(records[0]["argv"][-2:], ["-o", "/" + receipt["raster_path"]])
        self.assertIn("PNG:/" + receipt["raster_path"], records[1]["argv"])
        raster = self.root / receipt["raster_path"]
        self.assertEqual(candidate.digest(raster), receipt["raster_sha256"])
        self.assertEqual(candidate.picture_frame(terminal)["native_source"], receipt)
        self.assertEqual(len(records), 2)
        before, data = raster.stat(), raster.read_bytes()
        # Same length and mtime, so only a re-hash of the raster can see the change.
        raster.write_bytes(bytes([data[0] ^ 1]) + data[1:])
        os.utime(raster, ns=(before.st_atime_ns, before.st_mtime_ns))
        with self.assertRaisesRegex(Blocked, "retained native image decoding changed"):
            candidate.picture_frame(terminal)

    def test_native_column_size_cannot_satisfy_the_initial_quicklook_barrier(self):
        terminal = native_terminal(self.root)
        kitty(terminal, 1, pixels(width=2), width=2)
        with self.assertRaisesRegex(Blocked, "full calibrated size"):
            candidate.picture_frame(terminal)
        kitty(terminal, 2, pixels())
        self.assertTrue(candidate.picture_frame(terminal)["capture_sha256"])


if __name__ == "__main__":
    unittest.main()
