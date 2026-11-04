"""Width-changing disk facts and source-derived renderer acceptance controls."""
import copy
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

from .cells import width
from .dynamic import disk_field, masks, size
from .evidence import Blocked, Commands, MARKER, digest
from .fallback import bounds, representation_status, verify_sources
from .media import inspect
from .references import Board
from .case_identity import seal
from .reconcile import retained
from . import test_review_r2
from .test_review_r3 import NAMES, boards_directory
from . import png

CHILD = '''import sys
sys.path.insert(0, {tests!r})
from pathlib import Path
from tui_battery import png
if '--version' in sys.argv:
    print('fixture chafa boundary double, no native proof')
else:
    polite = next((a.split('=',1)[1] for a in reversed(sys.argv) if a.startswith('--polite=')), 'off') == 'on'
    if not polite:
        sys.stdout.write('\\x1b[?25l')
    dims = next(a.split('=',1)[1] for a in sys.argv if a.startswith('--size='))
    w,h = map(int,dims.split('x'))
    iw,ih,p = png.read(Path(sys.argv[-1]))
    for y in range(h):
        for x in range(w):
            a = p[((min(ih-1,y*ih//h)*iw)+min(iw-1,x*iw//w))*4:][:3]
            b = p[((min(ih-1,(2*y+1)*ih//(2*h))*iw)+min(iw-1,x*iw//w))*4:][:3]
            sys.stdout.write('\\x1b[38;2;%d;%d;%dm\\x1b[48;2;%d;%d;%dm▀' % (*a,*b))
        sys.stdout.write('\\x1b[0m\\n')
    if not polite:
        sys.stdout.write('\\x1b[?25h')
'''


def snapshot(board, columns=130, rows=31):
    cells = board.cells(columns, rows)
    return {"columns": columns, "rows": rows, "grid": [
        [dict(cells[x, y], reverse=False) for x in range(columns)] for y in range(rows)]}


def disk_snapshot(board, fs, free):
    result = snapshot(board)
    row = result["grid"][-1]
    field = disk_field(row)
    number = size(free).replace(".", ",") if field["comma"] else size(free)
    text = fs + field["labels"][0] + number + field["labels"][1]
    left = field["right"] - sum(width(c) for c in text)
    style = {key: value for key, value in row[field["left"]].items() if key not in ("ch", "width")}
    for x in range(min(left, field["left"]), field["right"]):
        row[x] = dict(style, ch=" ", width=1)
    x = left
    for ch in text:
        w = width(ch)
        row[x] = dict(style, ch=ch, width=w)
        if w == 2:
            row[x + 1] = dict(style, ch="", width=0)
        x += w
    return result, field


# Each change alters only the hint suffix right of the disk field, so the disk facts stay valid.
SUFFIX_CHANGES = (
    lambda cell: cell.update(ch="!"),
    lambda cell: cell.update(width=2),
    lambda cell: cell.update(fg="#ff0000"),
)


class Review4Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root = Path(tempfile.mkdtemp(prefix="review-r4.", dir=Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", Path(__file__).resolve().parents[2] / ".superpowers/t12a"))))
        (cls.root / MARKER).write_text("round four regression controls\n")

    def test_backend_derived_width_variations_on_five_approved_boards(self):
        for name in NAMES:
            board = Board(boards_directory() / (name + ".html"))
            for fs, free in (("tmpfs", 64200000), ("tmpfs", 164100000), ("tmpfs", 4100000),
                             ("tmpfs", 1), ("tmpfs", 1_000_000_000), ("btrfs", 2_000_000_000_000),
                             ("fuse.sshfs", 64100000), ("xfs", 64100000), ("xfs", 999)):
                with self.subTest(board=name, fs=fs, free=free):
                    actual, original = disk_snapshot(board, fs, free)
                    with patch("tui_battery.dynamic.fsinfo", return_value={"fs": fs, "free": free}):
                        expectations, facts = masks(board, actual, [], "/fx/v/work", None)
                    comparison = board.compare(actual, masks=expectations)
                    self.assertEqual(comparison["geometry"]["status"], "PASS")
                    self.assertEqual(comparison["color"]["status"], "PASS")
                    self.assertEqual(facts["anchor"][1], original["right"])
                    self.assertEqual(actual["grid"][-1][original["right"]:], snapshot(board)["grid"][-1][original["right"]:])
                    for change in SUFFIX_CHANGES:
                        corrupt = copy.deepcopy(actual)
                        row = corrupt["grid"][-1]
                        x = next(x for x in range(original["right"], corrupt["columns"]) if row[x]["ch"] != " ")
                        change(row[x])
                        with patch("tui_battery.dynamic.fsinfo", return_value={"fs": fs, "free": free}):
                            corrupt_expectations, _ = masks(board, corrupt, [], "/fx/v/work", None)
                        result = board.compare(corrupt, masks=corrupt_expectations)
                        self.assertIn("FAIL", {result["geometry"]["status"], result["color"]["status"]})

    def test_wider_tail_keeps_spacing_widths_and_all_rendition_checked(self):
        board = Board(boards_directory() / "tui-lang-ja.html")
        actual, _ = disk_snapshot(board, "fuse.sshfs", 164100000)
        with patch("tui_battery.dynamic.fsinfo", return_value={"fs": "fuse.sshfs", "free": 164100000}):
            expectations, facts = masks(board, actual, [], "/fx/v/work", None)
        start, end = facts["anchor"]
        for x in range(start, end):
            for key, value, gate in (("ch", "!", "geometry"), ("width", 0, "geometry"),
                                     ("fg", "#ff0000", "color"), ("bg", "#00ff00", "color"),
                                     ("bold", True, "color"), ("dim", True, "color"),
                                     ("underline", True, "color"), ("reverse", True, "color")):
                with self.subTest(cell=x, attribute=key):
                    corrupt = copy.deepcopy(actual)
                    corrupt["grid"][-1][x][key] = (1 if actual["grid"][-1][x]["width"] == 0 else 0) if key == "width" else value
                    self.assertEqual(board.compare(corrupt, masks=expectations)[gate]["status"], "FAIL")

    def fixture(self, name="tui-office", approved_board=False):
        case = Path(tempfile.mkdtemp(prefix="source.", dir=self.root))
        (case / MARKER).write_text("source-cell fallback fixture\n")
        fx = case / "fx"
        for directory in ("v/work", "data", "tools", "home", "config", "cache", "tmp"):
            (fx / directory).mkdir(parents=True, exist_ok=True)
        if approved_board:
            state = next(s for s in json.loads((Path(__file__).with_name("states.json")).read_text())["states"] if s["id"] == name)
            board = Board(boards_directory() / (name + ".html"))
        else:
            page = case / "board.html"
            page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d;--muted:#8b7b74;--muted-frame:#715e56;--accent-frame:#d28a41}</style>'
                            '<b style="left:4px;top:9px">Source fixture</b>')
            board = Board(page)
            board.pictures = [{"origin": [20.8, 53], "resolved_style": {"width": "33.6px", "height": "66px"}}]
            if name == "tui-combine":
                board.pictures += [{"origin": [71.2, 53], "resolved_style": {"width": "33.6px", "height": "66px"}}]
            state = {"id": name, "cwd": "/fx/v/work", "cursor": "source.pdf", "steps": []}
            if name == "tui-combine":
                state.update(cursor="photo.png", steps=[{"select": "sunset.jpg"}])
        declarations, copied, auxiliary = [], [], []
        names = [state["cursor"]] + ([s["select"] for s in state["steps"] if "select" in s] if name == "tui-combine" else [])
        for index, (filename, box) in enumerate(zip(names, board.media_boxes())):
            original = fx / state["cwd"].removeprefix("/fx/") / filename
            original.parent.mkdir(parents=True, exist_ok=True)
            original.write_bytes(("independent source fixture " + filename).encode())
            copied.append({"path": str(original.relative_to(fx / "v/work")), "sha256": digest(original)})
            raster = fx / "data" / f"render-{index}.png"
            w, h = round(box[2]), round(box[3])
            pixels = bytearray()
            for y in range(h):
                for x in range(w):
                    pixels.extend((220 if x < w // 2 else 40, 50, 20 + y * 160 // h, 255))
            png.write(raster, w, h, pixels)
            auxiliary.append({"path": str(raster.relative_to(fx)), "sha256": digest(raster)})
            declarations.append({"input": str(original.relative_to(fx)), "raster": str(raster.relative_to(fx)),
                                 "derivation": "Independent fixture raster; test boundary only"})
        executable = fx / "tools/chafa"
        executable.write_text("#!" + sys.executable + "\n" + CHILD.format(tests=str(Path(__file__).resolve().parents[1])))
        executable.chmod(0o700)
        auxiliary.append({"path": "tools/chafa", "sha256": digest(executable)})
        (case / "fixtures.json").write_text(json.dumps({"copied": copied, "auxiliary_copied": auxiliary,
            "representations": {name: {"sources": declarations}}}))
        actual = snapshot(board)
        caps = {"candidate_received": {"kitty_positive": False, "sixel_positive": False}, "forced_fallback": False}
        return case, fx, board, state, actual, caps

    def inspect(self, fixture, review=None, adapter="alacritty"):
        case, fx, board, state, actual, caps = fixture
        commands = Commands()
        result = inspect(state, board, actual, None, [4, 9, 1092, 682], caps, case, fx, boards_directory().parent,
                         review or {}, adapter=adapter, expected_transport="half-blocks", commands=commands)
        return (*result, commands)

    def prepared(self, name="tui-office", approved_board=False):
        fixture = self.fixture(name, approved_board)
        pending, _, expectations, commands = self.inspect(fixture)
        self.assertEqual(pending["status"], "BLOCKED")
        self.assertTrue(pending["representation_bound"])
        # Capture double paints the known independent oracle, without bypassing the comparator.
        for area in expectations:
            for entry in area["cells"]:
                x, y = entry["cell"]
                fixture[4]["grid"][y][x] = dict(entry["expected"], reverse=False)
        decision = dict(pending["review_requirements"], approved=True)
        return fixture, {"representation": decision}, expectations, commands

    def test_valid_fallback_uses_source_chafa_and_explicit_representation_review(self):
        fixture, review, _, _ = self.prepared()
        media, alternate, expectations, commands = self.inspect(fixture, review)
        self.assertEqual(media["status"], "PASS")
        self.assertIsNone(alternate)
        self.assertTrue(any("--format=symbols" in r["argv"] for r in commands.records))
        comparison = fixture[2].compare(fixture[4], masks=expectations)
        self.assertEqual(comparison["geometry"]["status"], "PASS")
        self.assertEqual(comparison["color"]["status"], "PASS")
        x, y = expectations[0]["field"][:2]
        self.assertEqual(fixture[4]["grid"][y][x]["ch"], "▀")
        self.assertEqual(fixture[4]["grid"][y][x]["fg"], "#dc3214")

    def test_all_eight_approved_states_accept_valid_mode_specific_fallback(self):
        for name in ("tui-list-pdf", "tui-office", "tui-list-video", "tui-quicklook-image",
                     "tui-3d-quicklook", "tui-print-sent", "tui-combine", "tui-lang-ar"):
            with self.subTest(state=name):
                fixture, review, _, _ = self.prepared(name, approved_board=True)
                for terminal in ("alacritty", "tmux"):
                    result, _, expected, _ = self.inspect(fixture, review, terminal)
                    self.assertEqual(result["status"], "PASS")
                    comparison = fixture[2].compare(fixture[4], masks=expected)
                    self.assertEqual(comparison["geometry"]["status"], "PASS")
                    self.assertEqual(comparison["color"]["status"], "PASS")

    def test_corrupt_source_content_geometry_and_color_are_rejected(self):
        fixture, review, _, _ = self.prepared()
        _, _, expected, _ = self.inspect(fixture, review)
        left, top, w, h = expected[0]["field"]
        for x, y in ((left, top), (left + w - 1, top + h - 1), (0, 0), (left - 1, top)):
            for key, value, gate in (("ch", "X", "geometry"), ("width", 2, "geometry"),
                                     ("fg", "#00ff00", "color"), ("bg", "#ff0000", "color"),
                                     ("dim", True, "color"), ("bold", True, "color"),
                                     ("underline", True, "color"), ("reverse", True, "color")):
                if x == left - 1 and key == "fg":
                    continue  # Outside blank foreground has no visible ink; background and attributes still compare.
                with self.subTest(cell=[x, y], attribute=key):
                    actual = copy.deepcopy(fixture[4])
                    actual["grid"][y][x][key] = value
                    self.assertEqual(fixture[2].compare(actual, masks=expected)[gate]["status"], "FAIL")
        for key in ("mode", "bounds", "sources", "renderer", "approved"):
            corrupt = copy.deepcopy(review)
            corrupt["representation"][key] = None
            self.assertEqual(self.inspect(fixture, corrupt)[0]["status"], "BLOCKED")
        source = fixture[1] / "data/render-0.png"
        source.write_bytes(b"corrupt source")
        with self.assertRaisesRegex(Blocked, "independently copied fixture"):
            self.inspect(fixture, review)

    def test_source_aspect_and_wrong_fixture_order_are_rejected(self):
        fixture = self.fixture()
        path = fixture[0] / "fixtures.json"
        receipt = json.loads(path.read_text())
        raster = fixture[1] / "data/wide.png"
        png.write(raster, 60, 1, bytes([240, 30, 10, 255]) * 60)
        receipt["auxiliary_copied"].append({"path": "data/wide.png", "sha256": digest(raster)})
        declaration = receipt["representations"]["tui-office"]["sources"][0]
        declaration["raster"] = "data/wide.png"
        path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(Blocked, "source aspect"):
            self.inspect(fixture)
        declaration["input"] = "v/work/other.pdf"
        path.write_text(json.dumps(receipt))
        with self.assertRaisesRegex(Blocked, "listing order"):
            self.inspect(fixture)

    def test_combine_numbered_boxes_keep_order_focus_ground_and_frame_ink(self):
        fixture, review, _, commands = self.prepared("tui-combine")
        self.assertFalse(commands.records)
        result, _, expected, _ = self.inspect(fixture, review)
        self.assertEqual(result["status"], "PASS")
        self.assertEqual(review["representation"]["mode"], "numbered-boxes")
        for number, area in enumerate(expected, 1):
            left, top, _, _ = area["field"]
            actual = fixture[4]["grid"]
            self.assertEqual(actual[top][left]["ch"], "┌")
            self.assertEqual(actual[top][left]["fg"], "#d28a41" if number == 1 else "#715e56")
            self.assertEqual(actual[top + 1][left + 1]["ch"], str(number))
            for key, value, gate in (("ch", "9", "geometry"), ("fg", "#ff0000", "color"), ("bg", "#000000", "color")):
                corrupt = copy.deepcopy(fixture[4])
                corrupt["grid"][top + 1][left + 1][key] = value
                self.assertEqual(fixture[2].compare(corrupt, masks=expected)[gate]["status"], "FAIL")

    def test_retained_approval_keeps_source_cells_and_configuration_gates(self):
        root = Path(tempfile.mkdtemp(prefix="retained.", dir=self.root))
        (root / MARKER).write_text("retained source-cell proof\n")
        helper = test_review_r2.Review2Tests()
        helper.root = root
        source, case, identity, row, config = helper.retained_case()
        fixture, decision, _, _ = self.prepared()
        pending, _, _, _ = self.inspect(fixture)
        # Move the independently verified fixture bytes alongside the retained native-boundary capture.
        import shutil
        for name in pending["source_files"]:
            target = case / "fx" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(fixture[1] / name, target)
        (case / "media.json").write_text(json.dumps(pending))
        verdict = json.loads((case / "verdict.json").read_text())
        verdict["artifact_sha256"], missing = seal(case)
        from .candidate import seal_extra
        missing += seal_extra(case, verdict["artifact_sha256"])
        self.assertFalse(missing)
        (case / "verdict.json").write_text(json.dumps(verdict))
        review = {"approved": True, "capture_sha256": digest(case / "native.png"),
                  "board_sha256": digest(root / "boards/tui-list-text.png"), **decision}
        key = "foot/tui-list-text/primary"
        def check(value, requested=config):
            return retained(source, identity, [row], root, {key: value}, {key: requested})[0]
        self.assertEqual(check(review)["status"], "PASS")
        absent = dict(review)
        absent.pop("representation")
        self.assertEqual(check(absent)["status"], "BLOCKED")
        self.assertEqual(check(review, dict(config, preset="windows"))["status"], "BLOCKED")
        for gate in ("geometry", "color"):
            changed = copy.deepcopy(verdict)
            changed["cells"][gate]["status"] = "FAIL"
            (case / "verdict.json").write_text(json.dumps(changed))
            self.assertEqual(check(review)["status"], "FAIL")
        (case / "verdict.json").write_text(json.dumps(verdict))
        (case / "fx/data/render-0.png").write_bytes(b"different retained render")
        self.assertEqual(check(review)["status"], "BLOCKED")

    def test_source_geometry_shift_and_failed_chafa_cannot_pass(self):
        fixture, review, _, _ = self.prepared()
        _, _, expected, _ = self.inspect(fixture, review)
        left, top, count, height = expected[0]["field"]
        actual = copy.deepcopy(fixture[4])
        for y in range(top, top + height):
            row = actual["grid"][y]
            row[left + 1:left + count + 1] = copy.deepcopy(row[left:left + count])
            row[left] = dict(fixture[2].cells()[left - 1, y], reverse=False)
        self.assertEqual(fixture[2].compare(actual, masks=expected)["geometry"]["status"], "FAIL")
        executable = fixture[1] / "tools/chafa"
        executable.write_text("#!" + sys.executable + "\nraise SystemExit(2)\n")
        with self.assertRaisesRegex(Blocked, "private chafa differs"):
            self.inspect(fixture, review)
        manifest_path = fixture[0] / "fixtures.json"
        manifest = json.loads(manifest_path.read_text())
        next(row for row in manifest["auxiliary_copied"] if row["path"] == "tools/chafa")["sha256"] = digest(executable)
        manifest_path.write_text(json.dumps(manifest))
        with self.assertRaisesRegex(Blocked, "command exited 2"):
            self.inspect(fixture, review)

    def test_pending_review_only_upgrades_exact_verified_representation(self):
        fixture, review, _, _ = self.prepared()
        pending, _, _, _ = self.inspect(fixture)
        self.assertEqual(representation_status(pending, review), "PASS")
        for key in pending["review_requirements"]:
            corrupt = copy.deepcopy(review)
            corrupt["representation"].pop(key)
            self.assertEqual(representation_status(pending, corrupt), "BLOCKED")
        self.assertEqual(representation_status(dict(pending, status="FAIL"), review), "FAIL")
        self.assertEqual(representation_status({"status": "BLOCKED"}, review), "BLOCKED")
        verify_sources(pending, fixture[1])
        (fixture[1] / "v/work/source.pdf").write_bytes(b"different retained source")
        with self.assertRaisesRegex(Blocked, "retained representation source"):
            verify_sources(pending, fixture[1])


if __name__ == "__main__":
    unittest.main()
