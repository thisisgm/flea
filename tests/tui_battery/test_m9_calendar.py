"""Calendar supplement: original identity, candidate view predicates, native geometry and the standard artifact bundle."""
from contextlib import redirect_stderr, redirect_stdout
from copy import deepcopy
import datetime
import errno
import io
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from zoneinfo import ZoneInfo

from contextlib import ExitStack
from types import SimpleNamespace
from unittest.mock import patch

from . import calendar_native, driver, fake, native as native_module, png
from .adapters import TERMINALS
from .evidence import Blocked, Commands, MARKER, digest
from .geometry import COLUMNS, ROWS
from .receipts import REQUIRED, supplement
from .references import Board
from .test_native_separator import native

HERE = Path(__file__).resolve().parent
ORIGINAL = "e3f18db34c5423ff8450b59d649d0395388bf8c85a5a3f45c5854ecf680b24b5"
MANIFEST = "10f86963175d3fe0d5a16d4af0e282e532a2e42c3d193e9b5ad8e9cc1a0add9b"
WORK = "/fx/v/work"
DATE = "2025-06-15 10:00"


def boards():
    value = os.environ.get("FLEA_BATTERY_BOARDS")
    if not value:
        raise Blocked("FLEA_BATTERY_BOARDS is unset: set it to the design-hig directory that holds boards/")
    return Path(value)


def board_snapshot(name):
    """The approved board as the cell grid a terminal snapshot carries."""
    cells = Board(boards() / "boards" / (name + ".html")).cells()
    grid = [[dict(cells[(x, y)], reverse=False) for x in range(COLUMNS)] for y in range(ROWS)]
    return {"columns": COLUMNS, "rows": ROWS, "grid": grid, "frames": 1, "pending": False, "unsupported": [],
            "pty": [ROWS, COLUMNS, COLUMNS * 8, ROWS * 16]}


def board_fixtures(modified=DATE):
    """The boards' work folder: seven folders, notes.txt and todo.md, every one modified at the board's date."""
    sizes = {"A": "462 B", "Code": "216 B", "Docs": "22.1 kB", "Empty": "40 B", "Locked": ">60 B", "Many": "2.2 kB", "Media": "2.6 MB"}
    rows = {name: {"directory": True, "modified": modified, "detail": size, "children": None} for name, size in sizes.items()}
    rows["notes.txt"] = {"directory": False, "modified": modified, "detail": "99 B", "children": None}
    rows["todo.md"] = {"directory": False, "modified": modified, "detail": "48 B", "children": None}
    return rows


def overwrite(snapshot, y, x, text):
    """Writes text into row y from cell x, keeping every cell's inks."""
    for offset, ch in enumerate(text):
        snapshot["grid"][y][x + offset]["ch"] = ch
    return snapshot


def column(snapshot, y, text):
    return "".join(cell["ch"] for cell in snapshot["grid"][y]).index(text)


def blank(snapshot, y, text):
    """Replaces one occurrence of text on row y with spaces, keeping every cell's inks."""
    row = snapshot["grid"][y]
    line = "".join(cell["ch"] for cell in row)
    start = line.index(text)
    for cell in row[start:start + len(text)]:
        cell["ch"] = " "
    return snapshot


class CalendarTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/m9-tui-02"))
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="calendar.", dir=parent)).absolute()
        (self.root / MARKER).write_text("owned calendar regression evidence\n")
        self.strings = calendar_native.strings()

    def test_m9_calendar_preserves_all_original_ids_and_driver_hash(self):
        retained = json.loads((HERE / "calendar.json").read_text())
        source = calendar_native.original_source()
        self.assertEqual(calendar_native.hashlib.sha256(source).hexdigest(), ORIGINAL)
        plan = calendar_native.original_plan()
        self.assertEqual(plan["labels"], sorted(retained["states"]))
        steps = [step for step in calendar_native.schedule(plan) if step["op"] == "capture"]
        self.assertEqual(sorted(step["label"] for step in steps), retained["states"])
        self.assertEqual(len({step["label"] for step in steps}), 17)
        self.assertEqual([name for name, _ in plan["definitions"]], ["amber", "bronze", "charlie.txt",
                         "delta-needleproof.txt", "echo.txt", "screenshot-2026-08-30.png", ".hidden-proof"])
        self.assertEqual(dict(plan["definitions"])["charlie.txt"], ("year", 6, 28, 0, 27))
        self.assertEqual(dict(plan["cases"])["echo.txt"], "file-midnight")
        self.assertEqual(plan["hidden"], ".hidden-proof")
        records = calendar_native.plan()
        self.assertEqual(len(records), 85)
        self.assertEqual(len({(row["terminal"], row["id"]) for row in records}), 85)
        for terminal in TERMINALS:
            selected = [row for row in records if row["terminal"] == terminal]
            self.assertEqual([row["id"] for row in selected], retained["states"])
            for row in selected:
                self.assertEqual(row["kind"], "calendar")
                self.assertEqual(row["original_driver_sha256"], ORIGINAL)
                self.assertEqual(row["original_manifest_sha256"], MANIFEST)
                self.assertEqual(row["adapted_driver_sha256"], digest(Path(calendar_native.__file__)))
                self.assertEqual(row["checks"], ["exact-date", "selected-identity", "view-predicate", "clean-quit"])
        self.assertEqual([row["id"] for row in calendar_native.plan(["kitty"])], retained["states"])
        for terminals in ([], ["foot", "foot"], ["unknown"]):
            with self.subTest(terminals=terminals), self.assertRaises(Blocked):
                calendar_native.plan(terminals)
        replacement = self.root / "date_case.py"
        replacement.write_bytes(source.replace(b"folder-a5", b"folder-a6"))
        with self.assertRaisesRegex(Blocked, "SHA-256 mismatch"):
            calendar_native.original_source(replacement)
        original = self.root / "original.py"
        original.write_bytes(source)
        self.assertEqual(calendar_native.original_source(original), source)

    def test_m9_calendar_missing_or_replacement_driver_blocks_before_inventory(self):
        original = self.root / "original.py"
        for present in (False, True):
            if present:
                original.write_text("raise SystemExit('replacement driver')\n")
            stdout, stderr = io.StringIO(), io.StringIO()
            with redirect_stdout(stdout), redirect_stderr(stderr):
                code = calendar_native.main(["--plan", "--original-driver", str(original)])
            self.assertEqual(code, 2)
            self.assertEqual(stdout.getvalue(), "")
            self.assertIn("SHA-256 mismatch" if present else "is absent", stderr.getvalue())

    def test_m9_calendar_fixture_tree_keeps_exact_original_dates(self):
        listing = self.root / "listing"
        zone = ZoneInfo("UTC")
        fixtures = calendar_native.build_fixtures(listing, "UTC")
        year = datetime.datetime.now(zone).year
        expected = {"amber": "2026-09-22 08:15", "bronze": "2025-12-31 23:55", "charlie.txt": f"{year}-06-28 00:27",
                    "delta-needleproof.txt": "2025-01-02 22:00", "echo.txt": "2026-09-23 00:05",
                    "screenshot-2026-08-30.png": "2026-09-22 08:15", ".hidden-proof": None}
        self.assertEqual({name: row["modified"] for name, row in fixtures.items()}, expected)
        for name, row in fixtures.items():
            path = listing / name
            self.assertEqual(path.stat().st_mtime_ns, row["mtime"] * 1000000000)
            if row["modified"]:
                stamp = datetime.datetime.fromtimestamp(row["mtime"], zone).strftime("%Y-%m-%d %H:%M")
                self.assertEqual(stamp, row["modified"])
        self.assertEqual(fixtures[".hidden-proof"]["mtime"], 0)
        self.assertTrue(fixtures["amber"]["directory"] and fixtures["bronze"]["directory"])
        self.assertEqual(fixtures["amber"]["children"], ["nested-proof.txt"])
        self.assertEqual((listing / "screenshot-2026-08-30.png").stat().st_size, 2100000)
        self.assertEqual(fixtures["screenshot-2026-08-30.png"]["detail"], "2.1 MB")
        self.assertEqual(fixtures["charlie.txt"]["detail"], "12 B")
        elsewhere = calendar_native.build_fixtures(self.root / "listing-pacific", "Pacific/Kiritimati")
        self.assertEqual(elsewhere["echo.txt"]["modified"], "2026-09-23 00:05")
        self.assertNotEqual(elsewhere["echo.txt"]["mtime"], fixtures["echo.txt"]["mtime"])
        with self.assertRaisesRegex(Blocked, "must be new"):
            calendar_native.build_fixtures(listing, "UTC")

    def test_m9_calendar_view_predicates_on_approved_boards(self):
        fixtures = board_fixtures()
        snapshot = board_snapshot("tui-list-text")
        receipt = calendar_native.view(snapshot, fixtures, "notes.txt", "preview", WORK, self.strings)
        self.assertEqual(receipt["observed"], [DATE])
        self.assertEqual(receipt["expected"], [DATE])
        self.assertTrue(receipt["cells"])
        properties = calendar_native.view(board_snapshot("tui-properties"), fixtures, "notes.txt", "properties", WORK, self.strings)
        self.assertEqual(properties["observed"], [DATE])
        self.assertEqual(properties["path"], WORK + "/notes.txt")
        look = dict(fixtures, **{"photo.png": {"directory": False, "modified": DATE, "detail": None, "children": None}})
        quick = calendar_native.view(board_snapshot("tui-quicklook-image"), look, "photo.png", "quicklook", WORK + "/Media", self.strings)
        self.assertEqual((quick["observed"], quick["expected"]), ([], []))
        self.assertEqual(calendar_native.view(snapshot, fixtures, "notes.txt", "loaded", WORK, self.strings)["observed"], [DATE])
        # Negative controls run after each positive path, on the same frames.
        refusals = {
            "wrong date": lambda: calendar_native.view(snapshot, board_fixtures("2025-06-15 10:01"), "notes.txt", "preview", WORK, self.strings),
            "wrong selected file": lambda: calendar_native.view(snapshot, fixtures, "todo.md", "preview", WORK, self.strings),
            "date where none is due": lambda: calendar_native.view(snapshot, dict(fixtures, **{"notes.txt": dict(fixtures["notes.txt"], modified=None)}), "notes.txt", "preview", WORK, self.strings),
            "wrong size": lambda: calendar_native.view(snapshot, dict(fixtures, **{"notes.txt": dict(fixtures["notes.txt"], detail="98 B")}), "notes.txt", "preview", WORK, self.strings),
            "absent fixture row": lambda: calendar_native.view(snapshot, dict(fixtures, extra={"directory": False, "modified": DATE, "detail": "1 B", "children": None}), "notes.txt", "preview", WORK, self.strings),
            "properties for another file": lambda: calendar_native.view(board_snapshot("tui-properties"), fixtures, "todo.md", "properties", WORK, self.strings),
            "properties wrong date": lambda: calendar_native.view(board_snapshot("tui-properties"), board_fixtures("2024-06-15 10:00"), "notes.txt", "properties", WORK, self.strings),
            "listing behind properties is no card": lambda: calendar_native.view(snapshot, fixtures, "notes.txt", "properties", WORK, self.strings),
            "quick look names another file": lambda: calendar_native.view(board_snapshot("tui-quicklook-image"), look, "notes.txt", "quicklook", WORK + "/Media", self.strings),
            "preview is no quick look": lambda: calendar_native.view(snapshot, fixtures, "notes.txt", "quicklook", WORK, self.strings),
            "unknown view": lambda: calendar_native.view(snapshot, fixtures, "notes.txt", "sideways", WORK, self.strings),
        }
        for name, check in refusals.items():
            with self.subTest(control=name), self.assertRaises(AssertionError):
                check()
        absent = blank(board_snapshot("tui-list-text"), 3, " · " + DATE)
        missing = dict(fixtures, **{"notes.txt": dict(fixtures["notes.txt"], modified=None)})
        self.assertEqual(calendar_native.view(absent, missing, "notes.txt", "preview", WORK, self.strings)["observed"], [])
        with self.assertRaises(AssertionError):
            calendar_native.view(absent, fixtures, "notes.txt", "preview", WORK, self.strings)
        no_row = blank(board_snapshot("tui-properties"), 15, "Modified     " + DATE)
        self.assertEqual(calendar_native.view(no_row, missing, "notes.txt", "properties", WORK, self.strings)["observed"], [])
        loading = board_snapshot("tui-properties")
        line = "".join(cell["ch"] for cell in loading["grid"][13])
        for offset, ch in enumerate(self.strings["properties.loading"]):
            loading["grid"][13][line.index("Kind") + offset]["ch"] = ch
        with self.assertRaisesRegex(AssertionError, "Properties has not answered"):
            calendar_native.view(loading, fixtures, "notes.txt", "properties", WORK, self.strings)
        dated = board_snapshot("tui-quicklook-image")
        bar = "".join(cell["ch"] for cell in dated["grid"][3])
        for offset, ch in enumerate(DATE):
            dated["grid"][3][bar.index("800") + offset]["ch"] = ch
        with self.assertRaisesRegex(AssertionError, "Quick Look"):
            calendar_native.view(dated, look, "photo.png", "quicklook", WORK + "/Media", self.strings)
        inks = calendar_native.role_inks(boards(), self.strings)
        self.assertEqual(calendar_native.color_status(snapshot, receipt["cells"]["date"], inks["preview"]), "PASS")
        recoloured = deepcopy(snapshot)
        x, y = receipt["cells"]["date"][0]
        recoloured["grid"][y][x]["fg"] = "#ff00ff"
        self.assertEqual(calendar_native.color_status(recoloured, receipt["cells"]["date"], inks["preview"]), "FAIL")
        self.assertEqual(calendar_native.color_status(snapshot, [], inks["preview"]), "PASS")

    def test_m9_calendar_detail_column_notice_lasts_four_seconds(self):
        fixtures = board_fixtures()
        detail = board_snapshot("tui-detail")
        receipt = calendar_native.detail_rows(detail, fixtures, "Modified")
        self.assertEqual(receipt["notice"], "Detail column · Modified")
        self.assertEqual(set(receipt["rows"].values()), {DATE})
        for name, check in {
                "wrong column date": lambda: calendar_native.detail_rows(detail, board_fixtures("2025-06-15 10:02"), "Modified"),
                "size column is not modified": lambda: calendar_native.detail_rows(board_snapshot("tui-list-text"), fixtures, "Modified"),
                "notice names another column": lambda: calendar_native.detail_rows(detail, fixtures, "Mode")}.items():
            with self.subTest(control=name), self.assertRaises(AssertionError):
                check()
        second = 1000000000
        good = [(int(0.2 * second), True), (int(3.9 * second), True), (int(4.05 * second), False)]
        window = calendar_native.notice_window(0, good, "Modified")
        self.assertGreaterEqual(window["gone_after_seconds"], 4)
        for name, observations in {
                "never shown": [(second, False)],
                "gone before four seconds": [(int(0.2 * second), True), (int(3.0 * second), False)],
                "still shown after five and a half seconds": [(int(0.2 * second), True), (int(5.6 * second), True), (int(6 * second), False)],
                "never gone": [(int(0.2 * second), True), (int(3.9 * second), True)],
                "shown for one second only": [(int(0.2 * second), True), (int(1.2 * second), True), (int(4.2 * second), False)]}.items():
            with self.subTest(control=name), self.assertRaisesRegex(Blocked, "Detail column · Modified"):
                calendar_native.notice_window(0, observations, "Modified")

    def test_m9_calendar_rejects_out_of_grid_observation(self):
        for terminal in TERMINALS:
            image, client, pty, grid, expected = native(terminal)
            snapshot = {"pty": pty, "rows": pty[0], "columns": pty[1], "grid": grid, "unsupported": []}
            left, top, width, height = expected
            inside = [(left, top), (left + width - 1, top + height - 1)]
            with self.subTest(terminal=terminal):
                self.assertEqual(calendar_native.observed_canvas(image, client, snapshot, inside), expected)
                cells = [(0, 0), (pty[1] - 1, pty[0] - 1), (pty[1] // 2, 3)]
                points = calendar_native.grid_points(cells, (left, top), pty)
                self.assertEqual(calendar_native.observed_canvas(image, client, snapshot, points), expected)
                for point in ((left - 1, top), (left + width, top), (left, top - 1), (left, top + height)):
                    with self.subTest(point=point), self.assertRaisesRegex(Blocked, "outside measured grid"):
                        calendar_native.observed_canvas(image, client, snapshot, [point])
                moved = calendar_native.grid_points(cells, (left + width // 2, top), pty)
                with self.assertRaisesRegex(Blocked, "outside measured grid"):
                    calendar_native.observed_canvas(image, client, snapshot, moved)
                with self.assertRaisesRegex(Blocked, "outside the PTY grid"):
                    calendar_native.grid_points([(pty[1], 0)], (left, top), pty)
                shifted = dict(snapshot, grid=[grid[0]] + grid[:-1])
                with self.assertRaisesRegex(Blocked, "disagrees with cell"):
                    calendar_native.observed_canvas(image, client, shifted, inside)

    def test_m9_calendar_rejects_incomplete_cells_and_pixel_metrics(self):
        image, client, pty, grid, expected = native("kitty")
        snapshot = {"pty": pty, "rows": pty[0], "columns": pty[1], "grid": grid, "unsupported": []}
        self.assertEqual(calendar_native.observed_canvas(image, client, snapshot, []), expected)
        for mutation in ({"grid": []}, {"grid": grid[:-1]}, {"grid": [grid[0][:-1]] + grid[1:]},
                         {"pty": pty[:2] + [0, 0]}, {"pty": pty[:2] + [pty[2] + 1, pty[3]]},
                         {"rows": pty[0] - 1}, {"pending": "\x1b["}, {"unsupported": ["unknown CSI"]}):
            with self.subTest(mutation=list(mutation)), self.assertRaisesRegex(Blocked, "incomplete"):
                calendar_native.observed_canvas(image, client, dict(snapshot, **mutation), [])

    def test_m9_calendar_keeps_setup_refusals_apart(self):
        stage = calendar_native.Stages()
        with self.assertRaisesRegex(Blocked, "^fontconfig: no font file"):
            with stage("fontconfig"):
                raise Blocked("no font file")
        self.assertEqual(stage.failed, "fontconfig")
        with self.assertRaisesRegex(Blocked, "^terminal-config: include"):
            with calendar_native.Stages()("terminal-config"):
                raise Blocked("include")
        with self.assertRaisesRegex(Blocked, "^grid: calendar observation"):
            with calendar_native.Stages()("grid"):
                raise Blocked("calendar observation (0, 0) outside measured grid")

    def test_m9_calendar_rejects_wrong_identity_and_dirty_quit(self):
        case = self.root / "relay"
        case.mkdir()
        receipt = fake.adapter_diagnostic("foot", case, Commands(), {"bg": "#1c120d", "fg": "#c6b395"})
        self.assertIs(receipt["native_acceptance"], False)
        self.assertIs(receipt["fake"], True)
        shutdown = json.loads((case / "relay-exit.json").read_text())
        identity = {"commit": "candidate-commit", "dirty_patch_sha256": "candidate-patch",
                    "elf_sha256": "candidate-elf", "references": {"board": "reference"}, "untracked": {}}
        terminal_exit = {"exit_code": receipt["exit_code"]}
        positive = calendar_native.candidate_receipt(identity, deepcopy(identity), terminal_exit, shutdown)
        self.assertIs(positive["clean-quit"], True)
        for field in identity:
            changed = dict(identity, **{field: "old-binary-or-source"})
            with self.subTest(field=field), self.assertRaisesRegex(Blocked, "identity mismatch: " + field):
                calendar_native.candidate_receipt(identity, changed, terminal_exit, shutdown)
        for field, value in (("candidate_exit", -15), ("candidate_exit", True),
                             ("termios_restored", False), ("relay_termios_restored", False)):
            with self.subTest(field=field), self.assertRaisesRegex(Blocked, "TTY restoration failed"):
                calendar_native.candidate_receipt(identity, identity, terminal_exit, dict(shutdown, **{field: value}))
        for value in (1, -15, True, None):
            with self.subTest(terminal_exit=value), self.assertRaisesRegex(Blocked, "terminal process"):
                calendar_native.candidate_receipt(identity, identity, {"exit_code": value}, shutdown)
        controls = calendar_native.session_negatives(identity, terminal_exit, shutdown)
        self.assertEqual(set(controls), {"wrong-identity", "dirty-quit", "early-notice", "late-notice"})
        self.assertTrue(all(controls.values()))

    def test_m9_calendar_folder_sizes_are_checked(self):
        fixtures = board_fixtures()
        folder = board_snapshot("tui-list-folder")
        self.assertEqual(calendar_native.view(folder, fixtures, "A", "preview", WORK, self.strings)["observed"], [DATE])
        for name, rows in {"listing total": dict(fixtures, Code=dict(fixtures["Code"], detail="217 B")),
                           "preview total": dict(fixtures, A=dict(fixtures["A"], detail="463 B")),
                           "unchecked folder": dict(fixtures, Docs=dict(fixtures["Docs"], detail=None)),
                           "unchecked file": dict(fixtures, **{"todo.md": dict(fixtures["todo.md"], detail=None)})}.items():
            with self.subTest(control=name), self.assertRaises(AssertionError):
                calendar_native.view(folder, rows, "A", "preview", WORK, self.strings)
        listing = self.root / "listing"
        built = calendar_native.build_fixtures(listing, "UTC")
        for name in ("amber", "bronze"):
            total = sum(os.lstat(path).st_size for path in [listing / name, *(listing / name).rglob("*")])
            self.assertEqual(built[name]["detail"], f"{total} B" if total < 1000 else built[name]["detail"])
            self.assertEqual(calendar_native.size_text(total), built[name]["detail"])
        self.assertEqual([calendar_native.size_text(n) for n in (0, 999, 1000, 22100, 2100000, 2649999)],
                         ["0 B", "999 B", "1.0 kB", "22.1 kB", "2.1 MB", "2.6 MB"])
        sizes = overwrite(board_snapshot("tui-list-text"), ROWS - 1, 10, "Detail column · Size")
        self.assertEqual(calendar_native.detail_rows(sizes, fixtures, "Size", False)["rows"]["Docs"], "22.1 kB")
        with self.assertRaisesRegex(AssertionError, "Size column for Code"):
            calendar_native.detail_rows(sizes, dict(fixtures, Code=dict(fixtures["Code"], detail="1 B")), "Size", False)

    def test_m9_calendar_quick_look_refuses_any_modified_fact(self):
        look = dict(board_fixtures(), **{"photo.png": {"directory": False, "modified": DATE, "detail": "1 B", "children": None}})
        positive = board_snapshot("tui-quicklook-image")
        self.assertEqual(calendar_native.view(positive, look, "photo.png", "quicklook", WORK + "/Media", self.strings)["observed"], [])
        x = column(positive, 3, "800")
        for text in ("Modified · Jun 15", "Modified · 2025-06-15", "2025-06-15", "Jun 15 10:00", "10:00", "Modified"):
            with self.subTest(text=text), self.assertRaisesRegex(AssertionError, "Quick Look"):
                calendar_native.view(overwrite(board_snapshot("tui-quicklook-image"), 3, x, text), look, "photo.png",
                                     "quicklook", WORK + "/Media", self.strings)
        dated = board_snapshot("tui-quicklook-image")
        name = column(dated, 3, "photo.png")
        overwrite(dated, 3, name, "s-2026-08-30.png" + " " * 11)
        named = dict(look, **{"s-2026-08-30.png": look["photo.png"]})
        self.assertEqual(calendar_native.view(dated, named, "s-2026-08-30.png", "quicklook", WORK + "/Media", self.strings)["observed"], [])

    def test_m9_calendar_manual_preview_loads_on_the_key(self):
        self.assertEqual(calendar_native.candidate_settings("default"),
                         {"keys": "default", "hidden": False, "sort": {"key": "name", "reverse": False},
                          "preview": {"loadOn": "manual", "column": True}})
        steps = [step for step in calendar_native.schedule(calendar_native.original_plan()) if step["op"] == "capture"]
        unloaded = [step["label"] for step in steps if step.get("unloaded")]
        self.assertEqual(unloaded, ["image-a5-preview"])
        fixtures = board_fixtures()
        bare = board_snapshot("tui-list-text")
        hint = self.strings["preview.loadHint"].replace("%1", "ctrl-space")
        rule = "".join(cell["ch"] for cell in bare["grid"][12]).rindex("│")
        waiting = overwrite(board_snapshot("tui-list-text"), 12, rule + 1, " " + hint)
        args = (fixtures, "notes.txt")
        self.assertEqual(calendar_native.view(waiting, *args, "preview", WORK, self.strings, unloaded=True)["observed"], [DATE])
        with self.assertRaisesRegex(AssertionError, "load key"):
            calendar_native.view(bare, *args, "preview", WORK, self.strings, unloaded=True)
        with self.assertRaisesRegex(AssertionError, "load key"):
            calendar_native.view(waiting, *args, "loaded", WORK, self.strings)
        self.assertEqual(calendar_native.view(bare, *args, "loaded", WORK, self.strings)["observed"], [DATE])

    def test_m9_calendar_capture_path_on_retained_native_frame(self):
        """capture_state on a real candidate frame from minipc: settle, capture, grid, negatives and artifacts."""
        image, client, pty, grid, rect = native("kitty")

        class Display:
            monitor = {"x": 0, "y": 0}
            def __init__(self, at):
                self.at = at
            def capture(self, path):
                png.write(path, *image)
                return image
            def client(self, identity):
                return {"at": self.at, "size": [client[2], client[3]]}

        class Terminal:
            adapter = type("Adapter", (), {"identity": "flea.battery.tcalendar"})
            def snapshot(self):
                return {"pty": list(pty), "rows": pty[0], "columns": pty[1], "grid": grid, "frames": 1,
                        "pending": False, "unsupported": []}

        case = self.root / "session"
        case.mkdir()
        (case / "pty.log").write_text('{"direction":"output","base64":""}\n')
        fixtures = board_fixtures()
        for name, size in {"A": "274 B", "Code": "170 B", "Docs": "22.0 kB", "Empty": "0 B", "Locked": ">20 B", "Many": "2.3 kB"}.items():
            fixtures[name]["detail"] = size
        context = {"root": calendar_native.output_bundle(self.root / "run"), "name": "kitty", "display": Display(client[:2]),
                   "terminal": Terminal(), "case": case, "commands": Commands(), "journal": [], "strings": self.strings,
                   "inks": calendar_native.role_inks(boards(), self.strings), "timeout": .6, "preset": "default", "hidden": False,
                   "offset": 0, "fixtures": fixtures, "profile": {"terminal": "kitty"},
                   "calibration": {"origin_padding": [rect[0] - client[0], rect[1] - client[1]]}}
        step = {"label": "folder-a5-preview", "name": "A", "kind": "preview"}
        directory, capture = calendar_native.capture_state(dict(context, fixtures=dict(fixtures)), dict(step))
        self.assertEqual(capture["receipt"]["observed"], [DATE])
        self.assertEqual(list(capture["rect"]), list(rect))
        self.assertEqual(capture["negatives"], {"wrong-date": True, "wrong-selected": True, "shifted-grid": True})
        self.assertIs(capture["synthetic"], False)
        # The retained frame carries no inks, so its colour check fails closed.
        self.assertEqual(capture["color"], "FAIL")
        for name in REQUIRED:
            self.assertTrue((directory / name).is_file(), name)
        verdict = calendar_native.write_verdict(directory, capture, {"identity": dict.fromkeys(calendar_native.IDENTITY_FIELDS, "x"),
                                                "clean-quit": True, "detail-notice": True, "session-negatives": True}, {})
        self.assertEqual(verdict["behavior"], "PASS")
        self.assertEqual(verdict["native_visual"], "PENDING")
        self.assertEqual(verdict["status"], "BLOCKED")
        late = dict(step, label="folder-older-preview")
        wrong = dict(fixtures, A=dict(fixtures["A"], modified="2025-06-15 10:01"))
        with self.assertRaisesRegex(Blocked, "never held: preview Modified for A"):
            calendar_native.capture_state(dict(context, fixtures=wrong), late)
        self.assertTrue((context["root"].path / "kitty/folder-older-preview/failure.png").is_file())
        self.assertTrue((context["root"].path / "kitty/folder-older-preview/last-cells.json").is_file())
        moved = dict(context, display=Display([client[0] + 300, client[1]]))
        with self.assertRaises(Blocked):
            calendar_native.capture_state(moved, dict(step, label="file-older-preview"))

    def bundle(self, terminal="kitty", quit=True, labels=None, approve=True, **override):
        """Writes the producer's standard artifacts for the labels from a retained native capture and its grid."""
        if not hasattr(self, "capture"):
            CalendarTests.capture = native(terminal)
        image, client, pty, grid, rect = self.capture
        identity = {"commit": "c", "dirty_patch_sha256": "p", "elf_sha256": "e", "references": {"r": "s"}, "untracked": {}}
        run = calendar_native.output_bundle(self.root / ("run-" + terminal + str(len(list(self.root.iterdir())))))
        snapshot = {"pty": pty, "rows": pty[0], "columns": pty[1], "grid": grid, "unsupported": [], "pending": False}
        entries = []
        for label in labels or calendar_native.original_plan()["labels"]:
            directory = run.directory(f"{terminal}/{label}")
            png.write(directory / "native.png", *image)
            capture = {"terminal": terminal, "label": label, "selected": "notes.txt", "kind": "preview",
                       "snapshot": snapshot, "image": image, "rect": list(rect), "client": list(client), "pty_log": b'{"direction":"output","base64":""}\n',
                       "capabilities": {"kitty_graphics": False}, "state": {"label": label}, "commands": [{"argv": ["grim"], "exit_code": 0}],
                       "profile": {"terminal": terminal}, "receipt": {"observed": [DATE], "expected": [DATE],
                       "cells": {"date": [(1, 3)], "name": [(2, 2)]}}, "color": "PASS", "negatives": {"wrong-date": True},
                       "assertions": {"exact-date": True, "selected-identity": True, "view-predicate": True}, "cell_geometry": "PASS",
                       "synthetic": False}
            capture.update(override.get(label, {}))
            calendar_native.write_artifacts(directory, capture)
            entries.append((directory, capture))
        session = {"identity": identity, "clean-quit": quit, "detail-notice": True, "session-negatives": True}
        reviews = {}
        for directory, capture in entries if approve else ():
            reviews[f"{terminal}/{capture['label']}"] = {"capture_sha256": digest(directory / "native.png"), "approved": True}
        for directory, capture in entries:
            calendar_native.write_verdict(directory, capture, session, reviews)
        calendar_native.seal(run.path, identity, [directory for directory, _ in entries], [terminal])
        return run.path, identity

    def test_m9_calendar_producer_writes_standard_artifacts_supplement_accepts(self):
        path, identity = self.bundle()
        calendar = json.loads((HERE / "calendar.json").read_text())
        rows = supplement(path, identity, ["kitty"], [], calendar)
        self.assertEqual([row["status"] for row in rows], ["PASS"] * 17, [row.get("reason") for row in rows])
        manifest = json.loads((path / "manifest.json").read_text())
        self.assertEqual(manifest["original_driver_sha256"], ORIGINAL)
        self.assertEqual(manifest["original_manifest_sha256"], MANIFEST)
        self.assertEqual(manifest["adapted_driver_sha256"], digest(Path(calendar_native.__file__)))
        verdict = json.loads((path / "kitty/folder-a5-preview/verdict.json").read_text())
        self.assertEqual(verdict["original_driver_sha256"], ORIGINAL)
        self.assertIs(verdict["synthetic"], False)
        for name in ("grid-crop.png", "overlay.png", "diff.png"):
            png.read(path / "kitty/folder-a5-preview" / name)
        # After the positive bundle: a missing state, a changed capture and fake or failing records all block.
        (path / "kitty/missing-properties/verdict.json").unlink()
        rows = supplement(path, identity, ["kitty"], [], calendar)
        self.assertEqual(sum(row["status"] == "PASS" for row in rows), 16)
        manifest["records"] = [row for row in manifest["records"] if row["id"] != "missing-preview"]
        (path / "manifest.json").write_text(json.dumps(manifest))
        rows = {row["id"]: row for row in supplement(path, identity, ["kitty"], [], calendar)}
        self.assertEqual(rows["missing-preview"]["reason"], "missing controller native receipt")
        changed = path / "kitty/folder-a5-preview/native.png"
        changed.write_bytes(changed.read_bytes() + b"\0")
        rows = {row["id"]: row for row in supplement(path, identity, ["kitty"], [], calendar)}
        self.assertIn("stale supplemental artifact: native.png", rows["folder-a5-preview"]["reason"])
        with self.assertRaisesRegex(Blocked, "identity mismatch: elf_sha256"):
            supplement(path, dict(identity, elf_sha256="old"), ["kitty"], [], calendar)
        failing = {"image-a5-loaded": {"assertions": {"exact-date": False, "selected-identity": True, "view-predicate": True}},
                   "file-older-preview": {"assertions": {"exact-date": True, "selected-identity": False, "view-predicate": True}},
                   "folder-older-preview": {"color": "FAIL"}, "file-midnight-preview": {"negatives": {"wrong-date": False}},
                   "image-a5-preview": {"synthetic": True}}
        subset = dict(calendar, states=sorted(failing) + ["missing-preview"])
        path, identity = self.bundle(labels=subset["states"], **failing)
        rows = {row["id"]: row for row in supplement(path, identity, ["kitty"], [], subset)}
        for label in failing:
            self.assertEqual(rows[label]["status"], "BLOCKED", label)
        self.assertIn("synthetic", rows["image-a5-preview"]["reason"])
        self.assertEqual(rows["missing-preview"]["status"], "PASS")
        subset = dict(calendar, states=["folder-a5-quicklook"])
        path, identity = self.bundle(quit=False, labels=subset["states"])
        rows = supplement(path, identity, ["kitty"], [], subset)
        self.assertEqual({row["status"] for row in rows}, {"BLOCKED"})

    def test_m9_calendar_visual_review_is_bound_to_each_capture(self):
        calendar = dict(json.loads((HERE / "calendar.json").read_text()), states=["image-a5-loaded", "image-a5-quicklook"])
        path, identity = self.bundle(labels=calendar["states"], approve=False)
        calendar_native.apply_reviews(path, {})
        rows = {row["id"]: row for row in supplement(path, identity, ["kitty"], [], calendar)}
        self.assertEqual({row["status"] for row in rows.values()}, {"BLOCKED"})
        stale = {f"kitty/{label}": {"capture_sha256": "0" * 64, "approved": True} for label in rows}
        calendar_native.apply_reviews(path, stale)
        self.assertEqual({row["status"] for row in supplement(path, identity, ["kitty"], [], calendar)}, {"BLOCKED"})
        approved = {f"kitty/{label}": {"capture_sha256": digest(path / "kitty" / label / "native.png"), "approved": True} for label in rows}
        calendar_native.apply_reviews(path, approved)
        self.assertEqual({row["status"] for row in supplement(path, identity, ["kitty"], [], calendar)}, {"PASS"})
        (path / "kitty/image-a5-quicklook/cells.json").write_text("{}")
        with self.assertRaisesRegex(Blocked, "cells.json"):
            calendar_native.apply_reviews(path, approved)

    def test_m9_calendar_review_refuses_edited_verdicts_and_escaping_records(self):
        calendar = dict(json.loads((HERE / "calendar.json").read_text()), states=["image-a5-loaded", "image-a5-quicklook"])
        path, identity = self.bundle(labels=calendar["states"], approve=False)
        approved = {f"kitty/{label}": {"capture_sha256": digest(path / "kitty" / label / "native.png"), "approved": True}
                    for label in calendar["states"]}
        calendar_native.apply_reviews(path, approved)
        verdict_path = path / "kitty/image-a5-loaded/verdict.json"
        verdict = json.loads(verdict_path.read_text())
        verdict["assertions"]["clean-quit"] = True
        verdict["behavior"] = "PASS"
        verdict["receipt"]["observed"] = ["1999-01-01 00:00"]
        verdict_path.write_text(json.dumps(verdict))
        with self.assertRaisesRegex(Blocked, "verdict changed"):
            calendar_native.apply_reviews(path, approved)
        self.assertEqual(json.loads(verdict_path.read_text())["receipt"]["observed"], ["1999-01-01 00:00"])
        path, identity = self.bundle(labels=calendar["states"], approve=False)
        manifest = json.loads((path / "manifest.json").read_text())
        other, _ = self.bundle(labels=calendar["states"], approve=False)
        for directory, reason in ((str(other / "kitty/image-a5-loaded"), "escaped"), ("../x/kitty/image-a5-loaded", "escaped"),
                                  ("", "escaped"), ("kitty/image-a5-quicklook", "names another record")):
            edited = deepcopy(manifest)
            edited["records"][0]["directory"] = directory
            (path / "manifest.json").write_text(json.dumps(edited))
            before = (other / "kitty/image-a5-loaded/verdict.json").read_bytes()
            with self.subTest(directory=directory), self.assertRaisesRegex(Blocked, reason):
                calendar_native.apply_reviews(path, approved)
            self.assertEqual((other / "kitty/image-a5-loaded/verdict.json").read_bytes(), before)

    def test_m9_calendar_acceptance_needs_the_whole_roster(self):
        partial = dict(json.loads((HERE / "calendar.json").read_text()), states=["image-a5-loaded"])
        path, identity = self.bundle(labels=partial["states"])
        manifest = json.loads((path / "manifest.json").read_text())
        self.assertIs(manifest["native_acceptance"], False)
        self.assertEqual(manifest["missing"], [f"kitty/{label}" for label in json.loads((HERE / "calendar.json").read_text())["states"]
                                               if label != "image-a5-loaded"])
        reviews = self.root / "reviews.json"
        reviews.write_text(json.dumps({"kitty/image-a5-loaded": {"capture_sha256": digest(path / "kitty/image-a5-loaded/native.png"),
                                                                 "approved": True}}))
        stdout = io.StringIO()
        with redirect_stdout(stdout):
            self.assertEqual(calendar_native.main(["--review", str(path), "--reviews", str(reviews), "--terminals", "kitty"]), 2)
        self.assertIs(json.loads(stdout.getvalue())["native_acceptance"], False)
        empty = json.loads((path / "manifest.json").read_text())
        empty["records"] = []
        (path / "manifest.json").write_text(json.dumps(empty))
        with redirect_stdout(io.StringIO()):
            self.assertEqual(calendar_native.main(["--review", str(path), "--reviews", str(reviews), "--terminals", "kitty"]), 2)
        full, _ = self.bundle()
        approved = {f"kitty/{label}": {"capture_sha256": digest(full / "kitty" / label / "native.png"), "approved": True}
                    for label in json.loads((HERE / "calendar.json").read_text())["states"]}
        reviews.write_text(json.dumps(approved))
        with redirect_stdout(io.StringIO()):
            self.assertEqual(calendar_native.main(["--review", str(full), "--reviews", str(reviews), "--terminals", "kitty"]), 0)
            self.assertEqual(calendar_native.main(["--review", str(full), "--reviews", str(reviews)]), 2)

    def test_m9_calendar_signal_cancels_and_cleans_up(self):
        children, cleaned = [], []

        def start(name):
            child = subprocess.Popen(["sleep", "30"], start_new_session=True)
            children.append(child)
            try:
                os.kill(os.getpid(), signal.SIGTERM if name == "foot" else signal.SIGHUP)
                time.sleep(5)
                return {"terminal": name, "status": "CAPTURED", "directories": []}
            except calendar_native.Cancelled as error:
                return {"terminal": name, "status": "BLOCKED", "reason": str(error), "cancelled": True, "directories": []}
            finally:
                os.killpg(child.pid, signal.SIGTERM)
                child.wait(5)
                cleaned.append(name)
        previous = signal.getsignal(signal.SIGTERM), signal.getsignal(signal.SIGHUP)
        for first in ("foot", "kitty"):
            children.clear(), cleaned.clear()
            with calendar_native.cancellable():
                sessions = calendar_native.run_sessions([first, "ghostty", "tmux"], start)
            self.assertEqual(cleaned, [first])
            self.assertIsNotNone(children[0].poll())
            self.assertIn("signal", sessions[0]["reason"])
            self.assertEqual([s["reason"] for s in sessions[1:]], ["cancelled before this terminal"] * 2)
            self.assertEqual((signal.getsignal(signal.SIGTERM), signal.getsignal(signal.SIGHUP)), previous)
        lost = calendar_native.run_sessions(["foot", "kitty"], lambda name: {"terminal": name, "status": "BLOCKED", "display_lost": True,
                                                                             "directories": []})
        self.assertEqual(lost[1]["reason"], "display lost before this terminal")

    def test_m9_calendar_review_refuses_linked_leaf_files(self):
        """Ported r2 probe: a linked verdict, manifest or artifact is refused before any read or write."""
        for leaf in ("kitty/image-a5-loaded/verdict.json", "manifest.json", "kitty/image-a5-loaded/cells.json"):
            path, _ = self.bundle(labels=["image-a5-loaded"], approve=False)
            other, _ = self.bundle(labels=["image-a5-loaded"], approve=False)
            outside = other / leaf
            before = outside.read_bytes()
            (path / leaf).unlink()
            (path / leaf).symlink_to(outside)
            reviews = {"kitty/image-a5-loaded": {"capture_sha256": digest(path / "kitty/image-a5-loaded/native.png"), "approved": True}}
            with self.subTest(leaf=leaf), self.assertRaisesRegex(Blocked, "linked or non-regular"):
                calendar_native.apply_reviews(path, reviews)
            self.assertEqual(outside.read_bytes(), before)
        path, _ = self.bundle(labels=["image-a5-loaded"], approve=False)
        verdict = path / "kitty/image-a5-loaded/verdict.json"
        verdict.unlink()
        os.mkfifo(verdict)
        with self.assertRaisesRegex(Blocked, "linked or non-regular"):
            calendar_native.apply_reviews(path, {})

    def production_session(self, terminal_class=None, launch=None, names=("kitty",)):
        """Runs the production session and run_sessions with only the display-facing pieces replaced."""
        class Adapter:
            def __init__(self, *args): pass
            def preflight(self, commands): return {}
            def configure(self): return {"font": "test"}
            def validate(self, commands): return {}
            def environment(self): return {}
            def close(self, commands): pass

        class Keys:
            def resolve(self, action): return {"key": "q"}
        args = SimpleNamespace(binary=self.root / "fake-elf", boards=self.root, preset="default", timeout=.1)
        identity = dict.fromkeys(calendar_native.IDENTITY_FIELDS, "test")
        started = []
        with ExitStack() as patches:
            patches.enter_context(patch.object(calendar_native, "Adapter", Adapter))
            patches.enter_context(patch.object(driver, "font_identity", return_value={}))
            patches.enter_context(patch.object(driver, "private_theme", return_value="theme"))
            patches.enter_context(patch.object(calendar_native.fixture_tree, "TMPFS", self.root))
            patches.enter_context(patch.object(calendar_native.fixture_tree, "namespace", return_value=["fake", "--setenv", "TZ", "UTC"]))
            if terminal_class:
                patches.enter_context(patch.object(calendar_native, "Terminal", terminal_class))
            if launch:
                patches.enter_context(patch.object(native_module.Terminal, "launch", launch))
            owner = calendar_native.output_bundle(self.root / ("session-" + str(len(list(self.root.iterdir())))))

            def start(name):
                started.append(name)
                return calendar_native.session(args, owner, None, name, {}, identity, calendar_native.original_plan(),
                                               {}, {}, Keys(), {}, {})
            with calendar_native.cancellable():
                sessions = calendar_native.run_sessions(list(names), start)
                inherited = subprocess.run([sys.executable, "-B", "-c", "import signal; print(signal.getsignal(signal.SIGTERM) == signal.SIG_DFL)"],
                                           capture_output=True, text=True, timeout=10).stdout.strip()
        return started, sessions, inherited

    def test_m9_calendar_cancellation_survives_session_cleanup(self):
        """Ported r2 probe: a signal that lands inside Terminal.close still cancels, blocks later terminals and leaks no SIG_IGN."""
        class Terminal:
            def __init__(self, *args): self.launched = False
            def launch(self, *args):
                self.launched = True
                raise Blocked("deliberate body refusal")
            def close(self):
                os.kill(os.getpid(), signal.SIGTERM)
                return {"exit_code": 0}
        previous = {number: signal.getsignal(number) for number in (signal.SIGTERM, signal.SIGHUP, signal.SIGINT)}
        started, sessions, inherited = self.production_session(terminal_class=Terminal, names=("kitty", "foot"))
        self.assertEqual(started, ["kitty"])
        self.assertIs(sessions[0].get("cancelled"), True)
        self.assertIn("SIGTERM", sessions[0]["cancel_reason"])
        self.assertEqual(sessions[1]["reason"], "cancelled before this terminal")
        self.assertEqual(inherited, "True")
        self.assertEqual({number: signal.getsignal(number) for number in previous}, previous)

    def test_m9_calendar_close_fallback_kills_and_reaps_the_terminal_group(self):
        """Ported r2 probe: the production Terminal.close path leaves no live member of a SIGTERM-ignoring terminal group."""
        children = []

        def launch(terminal, *args, **kwargs):
            program = ('import signal,sys,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); '
                       'open(sys.argv[1], "w").write("ready"); time.sleep(30)')
            child = subprocess.Popen([sys.executable, "-B", "-c", program, str(terminal.case / "ready")], start_new_session=True)
            children.append(child)
            native_module.wait_for(lambda: (terminal.case / "ready").exists(), 5, "owned child readiness")
            (terminal.case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
            terminal.launched = True
            os.kill(os.getpid(), signal.SIGHUP)
        try:
            _, sessions, _ = self.production_session(launch=launch)
            result = sessions[0]
            self.assertIs(result.get("cancelled"), True)
            self.assertEqual(result["reap"]["signals"][-1], "SIGKILL")
            self.assertIs(result["reap"]["live"], False)
            self.assertIsNotNone(children[0].poll())
            self.assertIn("process group", result["quit_reason"])
        finally:
            for child in children:
                if child.poll() is None:
                    os.killpg(child.pid, signal.SIGKILL)
                child.wait(5)
        quiet = calendar_native.reap_group(self.root / "absent-case", None)
        self.assertEqual(quiet, {"signals": [], "live": False, "pid": None})

    def test_m9_calendar_reap_refuses_a_group_the_case_cannot_prove_is_its_own(self):
        """Ported r3 probe L2-F7: a stale launch record whose numeric group is live is never signalled."""
        def launch(terminal, *args, **kwargs):
            (terminal.case / "launch-start.json").write_text(json.dumps({"pid": 314159, "start_ns": 1}))
            (terminal.case / "launch-exit.json").write_text(json.dumps({"exit_code": 0, "end_ns": 2}))
            terminal.launched = True
            raise Blocked("deliberate body refusal before cleanup")
        with patch.object(calendar_native, "group_members", return_value=[314159]), \
                patch.object(calendar_native.os, "killpg") as kill, \
                patch.object(calendar_native, "TERM_GRACE", 0), patch.object(calendar_native, "KILL_GRACE", 0):
            _, sessions, _ = self.production_session(launch=launch)
        self.assertEqual(kill.call_args_list, [])
        self.assertEqual(sessions[0]["reap"]["signals"], [])
        self.assertIs(sessions[0]["reap"]["live"], True)
        self.assertIn("not proven", sessions[0]["quit_reason"])

    def test_m9_calendar_group_anchor_needs_the_launch_identity(self):
        """Ported r3 probe L2-F7 proof: a pidfd anchors the launched leader only while its start time matches the launch record."""
        child = subprocess.Popen([sys.executable, "-B", "-c", "import time; time.sleep(30)"], start_new_session=True)
        try:
            case = self.root / "anchor-case"
            case.mkdir()
            (case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": 1}))
            self.assertIsNone(calendar_native.hold_group(case))
            (case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
            anchor = calendar_native.hold_group(case)
            self.assertIsNotNone(anchor)
            os.close(anchor)
        finally:
            child.kill()
            child.wait(5)
        (case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
        self.assertIsNone(calendar_native.hold_group(case))

    def test_m9_calendar_cancel_during_reap_still_kills_and_reaps(self):
        """Ported r3 probe L2-F4: a cancellation in the grace check still ends a SIGTERM-ignoring terminal group."""
        children, scans = [], []
        original_members = calendar_native.group_members

        def launch(terminal, *args, **kwargs):
            program = ('import signal,sys,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); '
                       'open(sys.argv[1], "w").write("ready"); time.sleep(30)')
            child = subprocess.Popen([sys.executable, "-B", "-c", program, str(terminal.case / "ready")], start_new_session=True)
            children.append(child)
            native_module.wait_for(lambda: (terminal.case / "ready").exists(), 5, "owned child readiness")
            (terminal.case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
            terminal.launched = True
            raise Blocked("deliberate body refusal before cleanup")

        def members_signalled_in_grace(group):
            scans.append(group)
            if len(scans) == 2:
                os.kill(os.getpid(), signal.SIGTERM)
            return original_members(group)
        try:
            with patch.object(calendar_native, "group_members", members_signalled_in_grace):
                started, sessions, _ = self.production_session(launch=launch, names=("kitty", "foot"))
        finally:
            for child in children:
                if child.poll() is None:
                    os.killpg(child.pid, signal.SIGKILL)
                child.wait(5)
        self.assertEqual(started, ["kitty"])
        self.assertIs(sessions[0].get("cancelled"), True)
        self.assertIn("SIGTERM", sessions[0]["cancel_reason"])
        self.assertEqual(sessions[0]["reap"]["signals"], ["SIGTERM", "SIGKILL"])
        self.assertIs(sessions[0]["reap"]["live"], False)
        self.assertIsNotNone(children[0].poll())
        self.assertEqual(sessions[1]["reason"], "cancelled before this terminal")


REUSE_PROBE = r'''
import json, subprocess, sys, time
from pathlib import Path
tests, case = Path(sys.argv[1]), Path(sys.argv[2])
sys.path.insert(0, str(tests))
from tui_battery import calendar_native as cn
case.mkdir(parents=True, exist_ok=True)
leader = subprocess.Popen(["sleep", "30"], start_new_session=True)
(case / "launch-start.json").write_text(json.dumps({"pid": leader.pid, "start_ns": time.time_ns()}))
anchor = cn.hold_group(case)
assert anchor is not None, "anchor not held"
leader.kill(); leader.wait()
Path("/proc/sys/kernel/ns_last_pid").write_text(str(leader.pid - 1))
stranger = subprocess.Popen(["sleep", "30"], start_new_session=True)
try:
    assert stranger.pid == leader.pid, f"reuse not forced: leader {leader.pid} stranger {stranger.pid}"
    reap = cn.reap_group(case, anchor)
    time.sleep(0.2)
    assert stranger.poll() is None, f"stranger {stranger.pid} signalled: {reap}"
    print(json.dumps({"reused_pid": stranger.pid, "reap": reap}))
finally:
    stranger.kill(); stranger.wait()
    cn.os.close(anchor)
'''


class PidfdTests(unittest.TestCase):
    """Ported r4 probes: the group is signalled through its pidfd only, and cancellation inside the launch-record path still reaps."""
    setUp = CalendarTests.setUp
    production_session = CalendarTests.production_session

    def launching(self, children, reads=None):
        def launch(terminal, *args, **kwargs):
            program = ('import signal,sys,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); '
                       'open(sys.argv[1], "w").write("ready"); time.sleep(30)')
            child = subprocess.Popen([sys.executable, "-B", "-c", program, str(terminal.case / "ready")], start_new_session=True)
            children.append(child)
            native_module.wait_for(lambda: (terminal.case / "ready").exists(), 5, "owned child readiness")
            (terminal.case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
            terminal.launched = True
            raise Blocked("deliberate body refusal before cleanup")
        return launch

    def test_m9_calendar_reap_signals_the_group_only_through_its_pidfd(self):
        """Ported r4 L2-F7 fix probe: no numeric kill API is reached, and the owned group still ends through its pidfd."""
        children, numeric = [], []

        def refuse(*args, **kwargs):
            numeric.append(args[:2])
            raise AssertionError("numeric kill reached the terminal group")
        try:
            launch = self.launching(children)

            def launch_with_exit(terminal, *args, **kwargs):
                (terminal.case / "launch-exit.json").write_text(json.dumps({"exit_code": 0, "end_ns": time.time_ns()}))
                return launch(terminal, *args, **kwargs)
            with patch.object(calendar_native.os, "killpg", side_effect=refuse), \
                    patch.object(calendar_native.os, "kill", side_effect=refuse), \
                    patch.object(calendar_native, "TERM_GRACE", .5), patch.object(calendar_native, "KILL_GRACE", .5):
                _, sessions, _ = self.production_session(launch=launch_with_exit)
        finally:
            for child in children:
                if child.poll() is None:
                    child.kill()
                child.wait(5)
        self.assertEqual(numeric, [])
        self.assertEqual(sessions[0]["reap"]["signals"], ["SIGTERM", "SIGKILL"])
        self.assertIs(sessions[0]["reap"]["live"], False)
        self.assertIsNotNone(children[0].poll())

    def test_m9_calendar_pid_reuse_cannot_signal_a_stranger(self):
        """Ported r4 pid_reuse probe: a stranger that takes the reaped leader's number is never signalled."""
        unshare = shutil.which("unshare")
        if not unshare or subprocess.run([unshare, "-Urpfm", "true"], stdin=subprocess.DEVNULL, capture_output=True).returncode:
            self.skipTest("unshare -Urpfm is unavailable in this lane, so pid reuse cannot be forced")
        script = self.root / "pid_reuse_probe.py"
        script.write_text(REUSE_PROBE)
        run = subprocess.run([unshare, "-Urpfm", "--mount-proc", sys.executable, "-B", str(script),
                              str(HERE.parent), str(self.root / "reuse-case")],
                             stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=120)
        self.assertEqual(run.returncode, 0, run.stderr)
        self.assertIn("reused_pid", run.stdout)

    def test_m9_calendar_cancel_inside_the_launch_record_read_still_reaps(self):
        """Ported r4 preamble probe: a cancellation inside the hold or the reap read still ends the group and is recorded."""
        real = calendar_native.read_leaf
        for call in (1, 2):
            with self.subTest(read=call):
                children, reads = [], []

                def read_leaf(path, call=call):
                    reads.append(path)
                    if len(reads) == call:
                        os.kill(os.getpid(), signal.SIGTERM)
                    return real(path)
                try:
                    with patch.object(calendar_native, "read_leaf", read_leaf):
                        _, sessions, _ = self.production_session(launch=self.launching(children))
                finally:
                    for child in children:
                        if child.poll() is None:
                            os.killpg(child.pid, signal.SIGKILL)
                        child.wait(5)
                result = sessions[0]
                self.assertIs(result.get("cancelled"), True)
                self.assertIn("SIGTERM", result["cancel_reason"])
                self.assertEqual(result["reap"]["signals"], ["SIGTERM", "SIGKILL"])
                self.assertIs(result["reap"]["live"], False)
                self.assertIsNotNone(children[0].poll())

    def test_m9_calendar_reap_holds_its_own_cancellation_when_called_alone(self):
        """Ported r4 preamble probe: reap_group finishes the escalation when a cancellation lands in its own launch read."""
        case = self.root / "preamble-case"
        case.mkdir()
        program = 'import signal,time; signal.signal(signal.SIGTERM, signal.SIG_IGN); time.sleep(30)'
        child = subprocess.Popen([sys.executable, "-B", "-c", program], start_new_session=True)
        try:
            time.sleep(.3)
            (case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
            anchor = calendar_native.hold_group(case)
            self.assertIsNotNone(anchor)
            real = calendar_native.read_leaf

            def read_leaf(path):
                os.kill(os.getpid(), signal.SIGTERM)
                return real(path)
            try:
                with calendar_native.cancellable(), patch.object(calendar_native, "read_leaf", read_leaf), \
                        patch.object(calendar_native, "TERM_GRACE", .5), patch.object(calendar_native, "KILL_GRACE", .5):
                    reap = calendar_native.reap_group(case, anchor)
            finally:
                os.close(anchor)
            self.assertIn("SIGTERM", reap["cancel"])
            self.assertEqual(reap["signals"], ["SIGTERM", "SIGKILL"])
            self.assertIs(reap["live"], False)
            self.assertIsNotNone(child.poll())
        finally:
            if child.poll() is None:
                child.kill()
            child.wait(5)

    def test_m9_calendar_pidfd_group_signals_refuse_when_unsupported(self):
        """Ported r4 fallback rule: an EINVAL from the pidfd group signal refuses and sends no numeric signal."""
        child = subprocess.Popen([sys.executable, "-B", "-c", "import time; time.sleep(30)"], start_new_session=True)
        case = self.root / "einval-case"
        case.mkdir()
        anchor = None
        try:
            (case / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
            anchor = calendar_native.hold_group(case)
            self.assertIsNotNone(anchor)
            with patch.object(signal, "pidfd_send_signal", side_effect=OSError(errno.EINVAL, "Invalid argument")), \
                    patch.object(calendar_native.os, "killpg") as killpg, patch.object(calendar_native.os, "kill") as kill:
                reap = calendar_native.reap_group(case, anchor)
            self.assertEqual(killpg.call_args_list + kill.call_args_list, [])
            self.assertEqual(reap["signals"], [])
            self.assertIs(reap["live"], True)
            self.assertIn("BLOCKED", reap["refused"])
            self.assertIsNone(child.poll())
        finally:
            if anchor is not None:
                os.close(anchor)
            child.kill()
            child.wait(5)


if __name__ == "__main__":
    unittest.main()
