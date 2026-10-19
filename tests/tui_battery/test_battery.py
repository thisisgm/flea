"""Behavioral and adversarial checks for the native battery driver."""
import base64
import json
import os
from pathlib import Path
import stat
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from . import png
from .adapters import TERMINALS
from .cells import Screen, predicate, text
from .driver import contract, selection
from .evidence import Blocked, MARKER, Owned, digest, output_root
from .fixtures import NOTES, import_family, regular_bytes
from .keys import Keymap, bytes_for
from .protocol import GraphicsNegative, capabilities
from .references import Board, compare_pixels, separator
from .scenarios import legacy_shift_delete, negatives, visual_review
from .dynamic import masks
from .receipts import supplement
from .media import inspect as inspect_media
from .reconcile import pixel_status
from .review_support import media_transcript

HERE = Path(__file__).resolve().parent
FIFO_DEADLINE_SECONDS = 5


class BatteryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        if not parent.is_absolute():
            parent = parent.absolute()
        cls.scratch = Path(tempfile.mkdtemp(prefix="tests.", dir=parent))
        (cls.scratch / MARKER).write_text("battery tests\n")

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="case.", dir=self.scratch))
        (self.root / MARKER).write_text("owned test case\n")

    def test_root_requires_marker_and_preserves_prior_output(self):
        plain = self.root / "plain"
        plain.mkdir()
        with self.assertRaises(Blocked):
            Owned(plain)
        made = output_root(self.root / "run")
        self.assertTrue((made.path / MARKER).is_file())
        with self.assertRaises(Blocked):
            output_root(made.path)

    def test_paths_reject_parent_and_symlink_escapes(self):
        owner = Owned(self.root)
        for path in ("", "..", "../escaped", "/", "a/../../b"):
            with self.subTest(path=path), self.assertRaises(Blocked):
                owner.child(path)
        (self.root / "alias").symlink_to(self.root, target_is_directory=True)
        with self.assertRaises(Blocked):
            owner.child("alias/new")
        self.assertFalse((self.root / "new").exists())

    def test_regular_source_refuses_fifo_without_blocking(self):
        fifo = self.root / "source-fifo"
        os.mkfifo(fifo)
        # Sample input: argv [FIFO_PATH, TESTS_DIR] where FIFO_PATH is a named pipe, not a regular file.
        source = ("import sys\n"
                  "sys.path.insert(0, sys.argv[2])\n"
                  "from pathlib import Path\n"
                  "from tui_battery.evidence import Blocked\n"
                  "from tui_battery.fixtures import regular_bytes\n"
                  "try:\n"
                  "    regular_bytes(Path(sys.argv[1]))\n"
                  "except Blocked as error:\n"
                  "    print(error)\n"
                  "else:\n"
                  "    sys.exit(1)\n")
        try:
            child = subprocess.run([sys.executable, "-B", "-c", source, str(fifo), str(HERE.parent)], stdin=subprocess.DEVNULL,
                                   capture_output=True, text=True, timeout=FIFO_DEADLINE_SECONDS, check=False)
        except subprocess.TimeoutExpired:
            self.fail("FIFO source blocked past its deadline")
        self.assertEqual(child.returncode, 0, child.stderr)
        self.assertIn("fixture is not a bounded regular file", child.stdout)

    def test_regular_source_refuses_symlink(self):
        target = self.root / "data"
        target.write_bytes(b"approved")
        link = self.root / "link"
        link.symlink_to(target)
        with self.assertRaises(OSError):
            regular_bytes(link)

    def linked_family(self, name, *links):
        source = self.fixture_source(name)
        manifest = source / "manifest.json"
        document = json.loads(manifest.read_text())
        # The "links" family keeps the nine original work entries; only "work" is pinned to exactly those.
        entries = document["families"]["work"]["entries"] + [
            {"path": path, "kind": "symlink", "target": link, "mode": 0o777, "mtime_ns": 123456789000}
            for path, link in links]
        document["families"]["links"] = {"root": "work", "entries": entries}
        manifest.write_text(json.dumps(document))
        return source, Path(tempfile.mkdtemp(dir=self.root))

    def test_symlink_target_must_stay_inside_snapshot(self):
        source, target = self.linked_family("valid", ("contained", "Docs"))
        import_family(source, "links", target)
        self.assertEqual((target / "contained").resolve(), (target / "Docs").resolve())
        escapes = ((("escape", "../outside"),), (("escape", "/etc"),),
                   (("contained", "Docs"), ("hop", "contained/../../outside")))
        for index, links in enumerate(escapes):
            with self.subTest(links=links):
                source, target = self.linked_family(f"escape-{index}", *links)
                with self.assertRaisesRegex(Blocked, "escapes its snapshot"):
                    import_family(source, "links", target)

    def fixture_source(self, name="source"):
        source = self.root / name
        work = source / "work"
        work.mkdir(parents=True)
        entries = []
        for name in ("A", "Code", "Docs", "Empty", "Locked", "Many", "Media"):
            (work / name).mkdir()
            entries.append({"path": name, "kind": "directory", "mode": 0 if name == "Locked" else 0o755, "mtime_ns": 123456789000})
        for name, body in (("notes.txt", NOTES), ("todo.md", b"x" * 48), ("Docs/notes.txt", NOTES),
                           ("Docs/readme.md", b"x" * 48), ("Docs/report.pdf", b"test fixture")):
            path = work / name
            path.write_bytes(body)
            entries.append({"path": name, "kind": "file", "mode": 0o644, "mtime_ns": 123456789000,
                            "size": len(body), "sha256": digest(path)})
        (source / "manifest.json").write_text(json.dumps({"families": {"work": {"root": "work", "entries": entries}}}))
        return source

    def test_fixture_snapshot_retains_bytes_mtime_and_locked_mode(self):
        source = self.fixture_source()
        target = self.root / "snapshot"
        target.mkdir()
        receipt = import_family(source, "work", target)
        self.assertEqual((target / "notes.txt").read_bytes(), NOTES)
        self.assertEqual((target / "notes.txt").stat().st_mtime_ns, 123456789000)
        self.assertEqual(stat.S_IMODE((target / "Locked").stat().st_mode), 0)
        self.assertEqual((source / "work/notes.txt").read_bytes(), NOTES)
        self.assertTrue(receipt["manifest_sha256"])

    def test_fixture_identity_mismatch_is_blocked(self):
        source = self.fixture_source()
        (source / "work/notes.txt").write_bytes(b"changed source")
        target = self.root / "snapshot"
        target.mkdir()
        with self.assertRaisesRegex(Blocked, "identity mismatch"):
            import_family(source, "work", target)

    def test_grid_fragmented_utf8_sgr_and_cjk(self):
        screen = Screen(12, 4)
        raw = "\x1b[H\x1b[38;2;1;2;3m議A\x1b[2;3Hok".encode()
        for byte in raw:
            screen.feed(bytes([byte]))
        snapshot = screen.snapshot()
        self.assertEqual(snapshot["grid"][0][0]["ch"], "議")
        self.assertEqual(snapshot["grid"][0][0]["width"], 2)
        self.assertEqual(snapshot["grid"][0][1]["width"], 0)
        self.assertEqual(snapshot["grid"][0][2]["ch"], "A")
        self.assertEqual(snapshot["grid"][1][2]["fg"], "#010203")
        self.assertEqual(snapshot["unsupported"], [])

    def test_grid_combining_marks_do_not_shift_cells(self):
        screen = Screen(10, 4)
        screen.feed("\x1b[He\u0301X".encode())
        self.assertEqual(screen.grid[0][0]["ch"], "e\u0301")
        self.assertEqual(screen.grid[0][1]["ch"], "X")

    def test_graphics_strings_never_become_text(self):
        screen = Screen(20, 4)
        screen.feed(b"\x1b[Hbefore\x1b_Gi=1;image")
        self.assertTrue(screen.snapshot()["pending"])
        screen.feed(b"\x1b\\after")
        self.assertIn("beforeafter", text(screen.snapshot()))
        self.assertNotIn("image", text(screen.snapshot()))
        self.assertEqual(len(screen.graphics), 1)

    def test_predicate_does_not_accept_an_old_frame(self):
        screen = Screen(20, 4)
        screen.feed(b"\x1b[HREADY")
        self.assertTrue(predicate(screen.snapshot(), {"contains": ["READY"]}))
        screen.feed(b"\x1b[HOTHER")
        self.assertFalse(predicate(screen.snapshot(), {"contains": ["READY"]}))

    def test_every_surface_predicate_names_text_the_pre_action_frame_lacks(self):
        states = json.loads((HERE / "surfaces.json").read_text())["states"]
        before = Screen(60, 4)
        before.feed(b"\x1b[HREADY")  # the frame before the state's setup key
        for state in states:
            with self.subTest(state=state["id"]):
                rule = state["predicate"]
                # An empty rule is true on every frame, including the one before the key.
                self.assertTrue(rule.get("contains") or rule.get("regex"))
                self.assertFalse(predicate(before.snapshot(), rule))

    def test_open_refused_predicate_is_the_refusal_sentence(self):
        states = {s["id"]: s for s in json.loads((HERE / "surfaces.json").read_text())["states"]}
        rule = states["tui-open-refused"]["predicate"]
        self.assertEqual(rule, {"contains": ["A folder can\u2019t be opened together with files."]})
        screen = Screen(60, 4)
        screen.feed("\x1b[H".encode() + rule["contains"][0].encode())
        self.assertTrue(predicate(screen.snapshot(), rule))

    def test_missing_pending_frame_is_not_settled(self):
        screen = Screen(20, 4)
        screen.feed(b"\x1b[HREADY\x1b[")
        self.assertFalse(predicate(screen.snapshot(), {"contains": ["READY"]}))

    def test_cell_erase_and_resize_remove_old_content(self):
        screen = Screen(20, 4)
        screen.feed(b"\x1b[Hstale\x1b[2J")
        self.assertNotIn("stale", text(screen.snapshot()))
        screen.resize(11, 6)
        self.assertEqual(len(screen.grid), 6)
        self.assertEqual(len(screen.grid[0]), 11)

    def test_graphics_negative_preserves_keyboard_and_stale_id(self):
        negative = GraphicsNegative()
        negative.ids.add(7)
        data = b"a\x1b_Gi=6;OK\x1b\\\x1b_Gi=7;OK\x1b\\\x1b[?1;4c\x1b[?3uZ"
        result = b"".join(negative.feed(bytes([b])) for b in data) + negative.flush()
        self.assertEqual(result, b"a\x1b_Gi=6;OK\x1b\\\x1b[?3uZ")
        self.assertEqual(len(negative.dropped), 2)

    def test_graphics_negative_preserves_errors_and_incomplete_input(self):
        negative = GraphicsNegative()
        negative.ids.add(7)
        raw = b"\x1b_Gi=7;ENOENT\x1b\\\x1b["
        self.assertEqual(negative.feed(raw) + negative.flush(), raw)
        self.assertEqual(negative.dropped, [])

    def test_capability_never_inferred_from_TERM_or_setup_replay(self):
        log = self.root / "pty.log"
        rows = [{"direction": "setup-input", "base64": base64.b64encode(b"\x1b_Gi=7;OK\x1b\\").decode()}]
        log.write_text("\n".join(json.dumps(r) for r in rows))
        result = capabilities(log)
        self.assertFalse(result["kitty_positive"])
        self.assertFalse(result["sixel_positive"])
        self.assertEqual(result["keyboard_flags"], [])

    def test_unqueried_reply_does_not_enable_graphics(self):
        log = self.root / "pty.log"
        def row(direction, data):
            return json.dumps({"direction": direction, "base64": base64.b64encode(data).decode()})
        log.write_text(row("output", b"\x1b_Gi=7,a=q;AAAA\x1b\\") + "\n" + row("terminal-input", b"\x1b_Gi=8;OK\x1b\\"))
        self.assertFalse(capabilities(log)["kitty_positive"])

    def test_reply_before_query_is_stale_even_with_matching_id(self):
        log = self.root / "pty.log"
        def row(direction, data):
            return json.dumps({"direction": direction, "base64": base64.b64encode(data).decode()})
        log.write_text(row("terminal-input", b"\x1b_Gi=7;OK\x1b\\") + "\n" + row("output", b"\x1b_Gi=7,a=q;AAAA\x1b\\"))
        self.assertFalse(capabilities(log)["kitty_positive"])

    def test_keymap_honors_preset_override_and_gui_exclusion(self):
        table = self.root / "keys.toml"
        table.write_text('[[text]]\nchar = "j"\naction = "down"\n'
                         '[[preset]]\nname = "vim"\nmods = "text"\nkey = "j"\naction = "other"\n'
                         '[[preset]]\nname = "vim"\nmods = "text"\nkey = "k"\naction = "down"\nfrontend = "gui"\n')
        self.assertEqual(Keymap(table).resolve("down")["key"], "j")
        with self.assertRaises(Blocked):
            Keymap(table, "vim").resolve("down")

    def test_keymap_accepts_explicit_all_context(self):
        table = self.root / "keys.toml"
        table.write_text('[[preset]]\nname = "default"\ncontext = "all"\nmods = "text"\nkey = "?"\naction = "sheet"\n')
        self.assertEqual(Keymap(table).resolve("sheet")["key"], "?")

    def test_plain_key_encoding_and_enhanced_refusal(self):
        self.assertEqual(bytes_for("C", "ctrl"), b"\x03")
        self.assertEqual(bytes_for("Return"), b"\r")
        self.assertEqual(bytes_for("Delete", "shift"), b"\x1b[3;2~")
        with self.assertRaises(Blocked):
            bytes_for("N", "ctrlshift")

    def test_delete_cases_drive_shift_delete_and_never_X(self):
        states = json.loads((HERE / "states.json").read_text())["states"]
        deletes = [s for s in states if s["id"] in ("tui-delete", "tui-delete-focus")]
        self.assertEqual(len(deletes), 2)
        self.assertTrue(all(s["steps"][0] == {"key": "Delete", "mods": "shift"} for s in deletes))
        self.assertFalse(any(step.get("key") == "X" or step.get("text") == "X" for s in deletes for step in s["steps"]))

    def test_legacy_shift_delete_proof_requires_native_bytes(self):
        log = self.root / "pty.log"
        def row(direction, data):
            return json.dumps({"direction": direction, "base64": base64.b64encode(data).decode()})
        log.write_text(row("setup-input", b"\x1b[3;2~"))
        with self.assertRaises(Blocked):
            legacy_shift_delete(log)
        log.write_text(row("terminal-input", b"\x1b[3;") + "\n" + row("terminal-input", b"2~"))
        self.assertEqual(legacy_shift_delete(log)["expected_hex"], "1b5b333b327e")

    def test_added_surfaces_and_current_alternatives_remain_required(self):
        surfaces = json.loads((HERE / "surfaces.json").read_text())["states"]
        self.assertEqual({s["id"] for s in surfaces}, {"tui-close-matches", "tui-search-contents", "tui-jobs", "tui-open-refused"})
        controls = json.loads((HERE / "controls.json").read_text())["controls"]
        keys = next(s for s in controls if s["id"] == "keys-presets")
        self.assertTrue(set(("ctrl-enter/O", "ctrl-shift-z/ctrl-y", "ctrl-shift-n/N", "ctrl-tab-keeps-ctrl-pagedown")).issubset(keys["checks"]))
        self.assertTrue(any(s["id"] == "shift-delete-legacy" for s in controls))

    def test_png_roundtrip_and_crop(self):
        path = self.root / "capture.png"
        pixels = bytearray([1, 2, 3, 255, 9, 8, 7, 255]) * 2
        png.write(path, 2, 2, pixels)
        self.assertEqual(png.read(path), (2, 2, pixels))
        self.assertEqual(png.crop(png.read(path), (1, 0, 1, 2)), (1, 2, bytearray([9, 8, 7, 255]) * 2))
        with self.assertRaises(ValueError):
            png.crop(png.read(path), (-1, 0, 1, 2))

    def test_png_bad_checksum_is_refused(self):
        path = self.root / "corrupt.png"
        png.write(path, 1, 1, bytes([1, 2, 3, 255]))
        data = bytearray(path.read_bytes())
        data[20] ^= 1
        path.write_bytes(data)
        with self.assertRaises(ValueError):
            png.read(path)

    def test_native_pixel_mismatch_is_not_silently_stretched(self):
        candidate, board = self.root / "candidate.png", self.root / "board.png"
        png.write(candidate, 2, 2, bytes([1, 2, 3, 255]) * 4)
        png.write(board, 3, 2, bytes([1, 2, 3, 255]) * 6)
        result = compare_pixels(candidate, board, (0, 0, 2, 2), self.root, (0, 0, 3, 2))
        self.assertEqual(result["status"], "BLOCKED")
        self.assertFalse(result["resampled"])
        self.assertTrue((self.root / "diff.png").is_file())
        self.assertTrue((self.root / "overlay.png").is_file())

    def test_geometry_constraint_requires_an_exact_explicit_review(self):
        pixels = {"status": "BLOCKED", "native_size": [1040, 682], "reference_size": [1092, 682]}
        visual = {"status": "PASS", "review": {}}
        self.assertEqual(pixel_status(pixels, visual), "BLOCKED")
        visual["review"]["geometry_constraint"] = {"native_size": [1040, 682], "reference_size": [1092, 682], "reason": "measured cell width"}
        self.assertEqual(pixel_status(pixels, visual), "PASS")
        visual["review"]["geometry_constraint"]["native_size"] = [1300, 682]
        self.assertEqual(pixel_status(pixels, visual), "BLOCKED")

    def test_separator_stays_inside_client_and_requires_observed_rule(self):
        pixels = bytearray([28, 18, 13, 255]) * 200 * 140
        # A one-pixel rule where terminals draw it in the second 22-pixel cell, (22 - 1) // 2 rows down, inside a padded client.
        for x in range(20, 180):
            pixels[(42 * 200 + x) * 4:(42 * 200 + x) * 4 + 4] = bytes([90, 90, 90, 255])
        rect = separator((200, 140, pixels), (10, 10, 180, 100), (4, 20, 160, 88))
        self.assertEqual(rect, (20, 10, 160, 88))
        with self.assertRaises(Blocked):
            separator((200, 140, pixels), (0, 0, 10, 10), (4, 20, 160, 88))
        with self.assertRaises(Blocked):
            separator((200, 140, bytearray([28, 18, 13, 255]) * 200 * 140), (10, 10, 180, 100), (4, 20, 160, 88))

    def test_board_cells_colors_corners_and_nested_offset(self):
        page = self.root / "reference.html"
        page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d;--accent:#d28a41}</style>'
                        '<div style="position:absolute;left:0px;top:166px"><div class="tgrid">'
                        '<i style="left:4px;top:9px;width:16.8px;height:22px;background:var(--accent)"></i>'
                        '<b style="left:4px;top:9px;color:var(--fg)">議</b></div></div>')
        cells = Board(page).cells(11, 6, (4, 175))
        self.assertEqual(cells[(0, 0)]["ch"], "議")
        self.assertEqual(cells[(1, 0)]["width"], 0)
        self.assertEqual(cells[(0, 0)]["bg"], "#d28a41")

    def test_board_comparison_refuses_added_text_and_wrong_cell_fill(self):
        page = self.root / "board.html"
        page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d}</style><b style="left:4px;top:9px;color:var(--fg)">A</b>')
        board = Board(page)
        screen = Screen(10, 4)
        screen.feed(b"\x1b[HA")
        self.assertEqual(board.compare(screen.snapshot())["geometry"]["status"], "PASS")
        screen.feed(b"X")
        self.assertEqual(board.compare(screen.snapshot())["geometry"]["status"], "FAIL")
        screen.feed(b"\x1b[H\x1b[41mA\x1b[0m \x1b[0K")
        self.assertEqual(board.compare(screen.snapshot())["color"]["status"], "FAIL")

    def test_nested_board_color_mix_resolves_without_losing_focus_ink(self):
        page = self.root / "board.html"
        page.write_text('<style>.th{--fg:#ffffff;--bg:#000000;--weight:50%}</style><b style="left:4px;top:9px;color:var(--fg)">A</b>')
        board = Board(page)
        self.assertEqual(board.ink('color-mix(in srgb,color-mix(in srgb,var(--fg) 50%,var(--bg)) var(--weight),var(--bg))'), '#404040')

    def test_dynamic_disk_mask_requires_exact_backend_agreement(self):
        page = self.root / "board.html"
        page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d}</style><b style="left:4px;top:669px;color:var(--fg)">9 items    vfat · 9.9 kB free   ? keys</b>')
        screen = Screen()
        screen.feed(b"\x1b[H\x1b[31;1H9 items    ext4 \xc2\xb7 5.0 kB free   ? keys")
        with patch('tui_battery.dynamic.fsinfo', return_value={"fs": "ext4", "free": 5000}):
            rectangles, facts = masks(Board(page), screen.snapshot(), [], "/fx/v/work", None)
        self.assertEqual(facts["raw_backend"]["free"], 5000)
        self.assertGreater(rectangles[0]["field"][0], len("9 items"))
        self.assertEqual(Board(page).compare(screen.snapshot(), masks=rectangles)["geometry"]["status"], "PASS")
        with patch('tui_battery.dynamic.fsinfo', return_value={"fs": "ext4", "free": 9000}), self.assertRaises(Blocked):
            masks(Board(page), screen.snapshot(), [], "/fx/v/work", None)

    def test_supplement_never_treats_missing_controls_as_skips(self):
        identity = {"commit": "c", "dirty_patch_sha256": "p", "elf_sha256": "e", "references": {}}
        rows = supplement(None, identity, ["foot"], [{"id": "native-emission"}], {"states": ["original-calendar"]})
        self.assertEqual(len(rows), 2)
        self.assertTrue(all(r["status"] == "BLOCKED" for r in rows))
        source = self.root / "receipts"
        source.mkdir()
        (source / "manifest.json").write_text(json.dumps({"identity": dict(identity, elf_sha256="stale")}))
        with self.assertRaises(Blocked):
            supplement(source, identity, ["foot"], [{"id": "native-emission"}], {"states": []})

    def test_native_media_requires_pixels_and_placement_after_query(self):
        page = self.root / "image-board.html"
        page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d}</style><b style="left:4px;top:9px">image</b>'
                        '<div style="position:absolute;left:4px;top:53px"><img style="width:16.8px;height:22px"></div>')
        caps = {"candidate_received": {"kitty_positive": True, "sixel_positive": False}, "forced_fallback": False}
        pixels = bytearray([28, 18, 13, 255]) * 100 * 100
        state = {"id": "image", "cwd": "/fx/v/work", "cursor": None}
        with self.assertRaisesRegex(Blocked, "nonempty native picture"):
            inspect_media(state, Board(page), Screen(10, 4).snapshot(), (100, 100, pixels), [4, 9, 84, 88], caps, self.root, self.root, self.root, {}, adapter="kitty", expected_transport="kitty")
        for y in range(53, 75):
            for x in range(4, 21):
                pixels[(y * 100 + x) * 4:(y * 100 + x) * 4 + 4] = bytes([x, y, 40, 255])
        caps["candidate_received"] = media_transcript(self.root, [{"ns": 30, "data": "\x1b_Gi=7,a=q;AAAA\x1b\\"}])
        with self.assertRaisesRegex(Blocked, "placement or sixel"):
            inspect_media(state, Board(page), Screen(10, 4).snapshot(), (100, 100, pixels), [4, 9, 84, 88], caps, self.root, self.root, self.root, {}, adapter="kitty", expected_transport="kitty")

    def test_stale_visual_approval_refused(self):
        directory = self.root / "foot/tui-list-text/primary"
        directory.mkdir(parents=True)
        capture = directory / "native.png"
        capture.write_bytes(b"native-capture")
        key = "foot/tui-list-text/primary"
        bad = {key: {"capture_sha256": "stale", "board_sha256": "board", "approved": True}}
        self.assertEqual(visual_review(capture, bad, "board")["status"], "FAIL")
        bad[key]["capture_sha256"] = digest(capture)
        self.assertEqual(visual_review(capture, bad, "board")["status"], "PASS")

    def test_required_inventory_is_61_by_five_with_two_small_captures(self):
        spec = json.loads((HERE / "states.json").read_text())
        self.assertEqual(len(spec["states"]), 61)
        self.assertEqual(len({s["id"] for s in spec["states"]}), 61)
        variants = sum(len(s["variants"]) for s in spec["states"])
        self.assertEqual(variants, 63)
        self.assertEqual(variants * len(TERMINALS), 315)
        states = {s["id"]: s for s in spec["states"]}
        self.assertEqual([(v["id"], v["columns"], v["rows"], v["origin"]) for v in states["tui-small"]["variants"]],
                         [("wide-short", 130, 4, [4, 9]), ("narrow", 11, 6, [4, 175])])
        self.assertEqual([v["id"] for v in states["tui-long"]["variants"]], ["primary", "rest"])
        self.assertEqual(spec["excluded"][0]["id"], "tui-send-with")
        self.assertEqual(len(json.loads((HERE / "calendar.json").read_text())["states"]), 17)

    def test_unknown_or_duplicate_filter_refused(self):
        for value in ("unknown", "foot,foot"):
            with self.assertRaises(Blocked):
                selection(value, ["foot", "kitty"])

    def test_after_proof_negative_controls_still_fail(self):
        self.assertEqual(len(negatives()), 4)


if __name__ == "__main__":
    unittest.main()
