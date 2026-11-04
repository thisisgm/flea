"""The row-2 rule finder on real native calibration captures, one per terminal, taken on minipc by the smoke run."""
import json
from pathlib import Path
import unittest

from . import png
from .evidence import Blocked
from .references import separator

FIXTURE = Path(__file__).resolve().parent / "fixtures/separator"
TERMINALS = ("alacritty", "foot", "ghostty", "kitty", "tmux")


def native(terminal):
    """The cropped capture with the client, the PTY, the cell grid and the expected grid, all in the crop's coordinates."""
    row = json.loads((FIXTURE / "separator.json").read_text())["terminals"][terminal]
    ox, oy = row["crop_origin"]
    image = png.read(FIXTURE / f"{terminal}.png")
    cx, cy, cw, ch = row["client"]
    gx, gy, gw, gh = row["grid"]
    reverse = {tuple(cell) for cell in row["reverse"]}
    cells = []
    for r, line in enumerate(row["cells"]):
        bgs = [bg for bg, count in line["bg"] for _ in range(count)]
        cells.append([{"ch": glyph, "bg": bg, "reverse": (r, c) in reverse} for c, (glyph, bg) in enumerate(zip(line["ch"], bgs))])
    return image, (cx - ox, cy - oy, cw, ch), row["pty"], cells, (gx - ox, gy - oy, gw, gh)


class NativeSeparatorTests(unittest.TestCase):
    def test_each_terminal_rule_with_junctions_places_the_grid_its_selection_fill_confirms(self):
        for terminal in TERMINALS:
            with self.subTest(terminal):
                image, client, pty, cells, expected = native(terminal)
                self.assertIn("┬", "".join(cell["ch"] for cell in cells[1]))
                rect = separator(image, client, pty)
                self.assertEqual(rect, expected)
                self.assertEqual(separator(image, client, pty, cells), expected)
                # The parent pane's highlight fills row 2 from its cell's top, so it fixes the grid's top on its own.
                width, _, pixels = image
                left, top, grid_w, grid_h = rect
                cell_w, cell_h = grid_w // pty[1], grid_h // pty[0]
                fill = bytes.fromhex(cells[2][0]["bg"][1:])
                column = left + cell_w // 2
                first = next(y for y in range(top, top + 4 * cell_h) if pixels[(y * width + column) * 4:(y * width + column) * 4 + 3] == fill)
                self.assertEqual(first, top + 2 * cell_h)

    def test_a_missing_rule_is_blocked_with_the_numbers_it_compared(self):
        image, client, pty, cells, (left, top, grid_w, _) = native("kitty")
        width, height, pixels = image
        pixels = bytearray(pixels)
        cell_h = pty[3] // pty[0]
        # The middle of the header row is a blank cell, so it shows the ground.
        middle = ((top + cell_h // 2) * width + left + grid_w // 2) * 4
        ground = pixels[middle:middle + 4]
        for y in range(top + cell_h, top + 2 * cell_h):
            pixels[(y * width + left) * 4:(y * width + left + grid_w) * 4] = ground * grid_w
        with self.assertRaises(Blocked) as raised:
            separator((width, height, pixels), client, pty)
        message = str(raised.exception)
        self.assertIn("0 anchors", message)
        self.assertIn(f"client {client[0]},{client[1]} {client[2]}x{client[3]}", message)
        self.assertIn("PTY 130x31 cells in 1430x775 px", message)
        self.assertIn("for a 1430 px rule; longest change", message)

    def test_a_rule_that_would_place_the_grid_outside_its_client_names_the_rejected_top(self):
        image, client, pty, cells, (left, top, _, _) = native("kitty")
        cx, cy, cw, ch = client
        with self.assertRaises(Blocked) as raised:
            separator(image, (cx, cy + 1, cw, ch - 1), pty)
        self.assertIn(f"x={left} thickness=2 top={top} outside the client", str(raised.exception))

    def test_a_grid_its_fill_contradicts_is_blocked_with_the_pixels_read(self):
        image, client, pty, cells, (left, top, _, _) = native("alacritty")
        # The same screen one row lower: the highlight it expects in row 3 is drawn in row 2.
        shifted = [cells[0]] + cells[:-1]
        with self.assertRaises(Blocked) as raised:
            separator(image, client, pty, shifted)
        self.assertIn(f"grid placed at {left},{top} disagrees with cell 0,3 filled", str(raised.exception))


if __name__ == "__main__":
    unittest.main()
