"""Second-round rejection controls for facts, displays and retained identities."""
import base64
from contextlib import ExitStack, redirect_stdout
import io
import inspect
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from .cells import Screen
from .dynamic import masks
from .evidence import Blocked, MARKER, digest
from .media import inspect as inspect_media
from .protocol import capabilities
from .reconcile import retained
from .references import Board
from .review_support import board_file, pair
from . import driver

HERE = Path(__file__).resolve().parent
LOCALES = {"ja": "tmpfs · 空き 64.1 MB", "ar": "tmpfs · 64.1 MB ةحاتم", "ru": "tmpfs · свободно 64,1 MB"}


class Review2Tests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        parent.mkdir(parents=True, exist_ok=True)
        cls.scratch = Path(tempfile.mkdtemp(prefix="review-r2.", dir=parent))
        (cls.scratch / MARKER).write_text("second-round controls\n")

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="case.", dir=self.scratch))
        (self.root / MARKER).write_text("owned case\n")

    def localized(self, language, actual=None):
        status = LOCALES[language]
        page = self.root / "localized.html"
        page.write_text('<style>.th{--fg:#c6b395;--bg:#1c120d}</style>'
                        '<b style="left:4px;top:669px">12 項目</b>'
                        f'<b style="left:676px;top:669px">{status}</b>'
                        '<b style="left:1012px;top:669px">? keys</b>')
        board = Board(page)
        screen = Screen()
        screen.feed(("\x1b[31;1H12 項目\x1b[31;81H" + (actual or status) + "\x1b[31;121H? keys").encode())
        return board, screen.snapshot()

    def test_1_localized_false_facts_are_refused(self):
        for language in LOCALES:
            with self.subTest(language=language):
                board, snapshot = self.localized(language)
                with patch("tui_battery.dynamic.fsinfo", return_value={"fs": "ext4", "free": 5000}) as backend:
                    with self.assertRaisesRegex(Blocked, "real backend"):
                        masks(board, snapshot, [], "/fx/v/work", None)
                    backend.assert_called_once()

    def test_1_verified_localized_variation_masks_only_disk_cells(self):
        for language in LOCALES:
            with self.subTest(language=language):
                actual = LOCALES[language].replace("64.1", "64.2").replace("64,1", "64,2")
                board, snapshot = self.localized(language, actual)
                with patch("tui_battery.dynamic.fsinfo", return_value={"fs": "tmpfs", "free": 64200000}) as backend:
                    rectangles, facts = masks(board, snapshot, [], "/fx/v/work", None)
                    backend.assert_called_once()
                self.assertIsNotNone(facts)
                self.assertEqual(rectangles[0]["field"][0], 80)
                self.assertLess(rectangles[0]["field"][0] + rectangles[0]["field"][2], 120)
                self.assertEqual(board.compare(snapshot, masks=rectangles)["geometry"]["status"], "PASS")
                snapshot["grid"][30][0]["ch"] = "9"
                self.assertEqual(board.compare(snapshot, masks=rectangles)["geometry"]["status"], "FAIL")

    def test_1_mask_cannot_hide_wrong_language_or_decimal_recipe(self):
        for language, actual in (("ja", "tmpfs · 64.2 MB free"), ("ar", "tmpfs · 64.2 MB متاحة"),
                                 ("ru", "tmpfs · свободно 64.2 MB")):
            with self.subTest(language=language):
                board, snapshot = self.localized(language, actual)
                with patch("tui_battery.dynamic.fsinfo", return_value={"fs": "tmpfs", "free": 64200000}):
                    with self.assertRaises(Blocked):
                        masks(board, snapshot, [], "/fx/v/work", None)

    def test_1_shorter_verified_field_cannot_hide_adjacent_text(self):
        board, snapshot = self.localized("ja", "e4 · 空き 5 B BAD")
        with patch("tui_battery.dynamic.fsinfo", return_value={"fs": "e4", "free": 5}):
            expectations, _ = masks(board, snapshot, [], "/fx/v/work", None)
        self.assertEqual(board.compare(snapshot, masks=expectations)["geometry"]["status"], "FAIL")

    def image(self, graphics, protocol="kitty", output_fragments=None):
        page = board_file(self.root, "tui-list-image")
        page.pictures = [{"origin": [4, 9], "resolved_style": {"width": "16.8px", "height": "22px"}}]
        query, reply = ((b"\x1b_Gi=7,a=q;AAAA\x1b\\", b"\x1b_Gi=7;OK\x1b\\") if protocol == "kitty"
                        else (b"\x1b[c", b"\x1b[?1;4c"))
        rows = [(10, "output", query), (21, "candidate-input", reply)]
        rows += [(ns, "output", data.encode()) for ns, data in (graphics if output_fragments is None else output_fragments)]
        rows.sort(key=lambda row: row[0])
        (self.root / "pty.log").write_text("\n".join(json.dumps({"ns": ns, "direction": direction,
            "base64": base64.b64encode(data).decode()}) for ns, direction, data in rows))
        caps = {"candidate_received": capabilities(self.root / "pty.log", "candidate-input"), "forced_fallback": False}
        (self.root / "graphics.log").write_text("\n".join(json.dumps({"ns": ns, "data": data}) for ns, data in graphics))
        pixels = bytearray(v for y in range(100) for x in range(100) for v in (x, y, 40, 255))
        state = {"id": "tui-list-image", "cwd": "/fx/v/work", "cursor": "photo.png"}
        (self.root / "v/work").mkdir(parents=True, exist_ok=True)
        (self.root / "v/work/photo.png").write_bytes(b"fixture identity")
        arguments = (state, page, Screen(10, 4).snapshot(), (100, 100, pixels), [4, 9, 84, 88], caps, self.root, self.root, self.root)
        keywords = dict(adapter="kitty" if protocol == "kitty" else "foot", expected_transport=protocol)
        # The controller approves the picture for the identities this run produced; placements are still judged.
        first = inspect_media(*arguments, {}, **keywords)[0]
        approval = {"representation": dict(approved=True, **first["review_requirements"])} if "review_requirements" in first else {}
        return inspect_media(*arguments, approval, **keywords)

    def test_2_transmit_only_is_not_placement(self):
        with self.assertRaises(Blocked):
            self.image([(30, "\x1b_Ga=t,i=42;AAAA\x1b\\")])

    def test_2_display_before_positive_reply_is_refused(self):
        with self.assertRaises(Blocked):
            self.image([(15, "\x1b_Ga=T,i=42;AAAA\x1b\\")])

    def test_2_payload_free_placement_after_transmit_passes(self):
        result = self.image([(30, "\x1b_Ga=t,i=42;AAAA\x1b\\"), (31, "\x1b_Ga=p,i=42,q=2\x1b\\")])
        self.assertEqual(result[0]["status"], "PASS")

    def test_2_completed_chunked_display_and_wrong_id(self):
        graphics = [(30, "\x1b_Ga=t,i=42,m=1;AAAA\x1b\\"), (31, "\x1b_Gm=0;BBBB\x1b\\")]
        with self.assertRaises(Blocked):
            self.image(graphics)
        with self.assertRaisesRegex(Blocked, "image ID"):
            self.image(graphics + [(32, "\x1b_Ga=p,i=43\x1b\\")])
        self.assertEqual(self.image(graphics + [(32, "\x1b_Ga=p,i=42\x1b\\")])[0]["status"], "PASS")
        display = [(30, "\x1b_Ga=T,i=42,m=1;AAAA\x1b\\")]
        with self.assertRaises(Blocked):
            self.image(display)
        self.assertEqual(self.image(display + [(32, "\x1b_Gm=0;BBBB\x1b\\")])[0]["status"], "PASS")

    def test_2_fragmented_image_start_precedes_its_completion(self):
        token = "\x1b_Ga=T,i=42;AAAA\x1b\\"
        with self.assertRaisesRegex(Blocked, "before.*negotiation"):
            self.image([(30, token)], output_fragments=[(15, token[:8]), (30, token[8:])])
        result = self.image([(30, token)], output_fragments=[(23, token[:8]), (30, token[8:])])
        self.assertEqual(result[0]["placements"][0]["start_ns"], 23)

    def test_2_sixel_also_requires_earlier_positive_reply(self):
        token = "\x1bP0;1q#1;2;100;0;0~~~~\x1b\\"
        with self.assertRaisesRegex(Blocked, "before.*negotiation"):
            self.image([(15, token)], protocol="sixel")
        self.assertEqual(self.image([(30, token)], protocol="sixel")[0]["status"], "PASS")

    def test_2_raw_early_image_cannot_be_omitted_from_graphics_receipt(self):
        early = "\x1b_Ga=T,i=43;BBBB\x1b\\"
        current = "\x1b_Ga=T,i=42;AAAA\x1b\\"
        with self.assertRaises(Blocked):
            self.image([(30, current)], output_fragments=[(15, early), (30, current)])

    def retained_case(self):
        board_file(self.root)
        source = self.root / "capture"
        case = source / "foot/tui-list-text/primary"
        case.mkdir(parents=True)
        identity = dict(commit="c", dirty_patch_sha256="p", elf_sha256="e", references={}, untracked={})
        from .candidate import input_identity, LIFECYCLE
        identity["candidate_inputs"] = input_identity(HERE.parents[1])
        (source / "identity.json").write_text(json.dumps(identity))
        row = dict(terminal="foot", state="tui-list-text", variant="primary")
        state = next(s for s in json.loads((HERE / "states.json").read_text())["states"] if s["id"] == row["state"])
        config = {"terminal": "foot", "preset": "vim", "variant": {"id": "primary", "columns": 130, "rows": 31, "origin": [4, 9]},
                  "profile": {"font": "JetBrainsMono Nerd Font", "font_size": 14}, "state": state, "steps": state["steps"], "seam": {}}
        values = {"native.png": {}, "grid-crop.png": {}, "cells.json": {"columns": 130, "rows": 31}, "pty.log": {}, "graphics.log": {},
                  "capabilities.json": {}, "state.json": {"preset": "vim", "requested": state, "variant": config["variant"]}, "commands.json": [], "diff.png": {}, "overlay.png": {},
                  "media.json": {"status": "PASS"}, "profile.json": dict(config["profile"], terminal="foot", packages=["foot 1"], path=str(case / "terminal.conf")),
                  "fixtures.json": {"family": "work", "manifest_sha256": "original-fixture"}, "launch-spec.json": {"argv": ["foot"]},
                  "relay-spec.json": {"argv": ["flea"]}, "capture-config.json": config,
                  "candidate-settings.json": {"keys": "vim"}, "fx/state/flea/ui.json": {"keys": "vim"}, "terminal.conf": {}, "dynamic-facts.json": {}}
        for name, value in values.items():
            target = case / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(json.dumps(value))
        raw = {"launch-exit.json": {"exit_code": 0},
               "relay-exit.json": {"candidate_exit": 0, "termios_restored": True, "relay_termios_restored": True},
               "launch-start.json": {"pid": 2147483647, "start_ns": 1},
               "relay-start.json": {"stdin_tty": True, "stdout_tty": True}}
        for name in LIFECYCLE:
            (case / name).write_text(json.dumps(raw[name]))
            values[name] = raw[name]
        verdict = dict(row, behavior={"status": "PASS"}, cells={"geometry": {"status": "PASS"}, "color": {"status": "PASS"}},
                       pixels={"status": "REVIEW"}, edges={"status": "PASS"}, terminal_exit={"exit_code": 0},
                       shutdown={"candidate_exit": 0, "termios_restored": True, "relay_termios_restored": True},
                       artifact_sha256={name: digest(case / name) for name in values})
        (case / "verdict.json").write_text(json.dumps(verdict))
        return source, case, identity, row, config

    def reconcile(self, source, identity, row, config):
        args = (source, identity, [row], self.root, {})
        extra = {"configurations": {"foot/tui-list-text/primary": config}} if "configurations" in inspect.signature(retained).parameters else {}
        with patch("tui_battery.reconcile.visual_review", return_value={"status": "PASS"}):
            return retained(*args, **extra)[0]

    def test_3_changed_case_files_cannot_reuse_acceptance(self):
        source, case, identity, row, config = self.retained_case()
        self.assertEqual(self.reconcile(source, identity, row, config)["status"], "PASS")
        for name in ("profile.json", "fixtures.json", "launch-spec.json", "relay-spec.json", "fx/state/flea/ui.json",
                     "terminal.conf", "candidate-settings.json", "capture-config.json", "graphics.log"):
            with self.subTest(file=name):
                path = case / name
                original = path.read_text()
                self.assertEqual(self.reconcile(source, identity, row, config)["status"], "PASS")
                try:
                    path.write_text(json.dumps({"font_size": 24, "keys": "windows", "packages": ["foot 2"]}))
                    self.assertNotEqual(self.reconcile(source, identity, row, config)["status"], "PASS")
                finally:
                    path.write_text(original)

    def test_3_conflicting_requested_configuration_is_refused(self):
        source, case, identity, row, config = self.retained_case()
        for field, value in (("terminal", "ghostty"), ("preset", "windows"), ("profile", {"font": "other", "font_size": 24}),
                             ("variant", dict(config["variant"], columns=100))):
            with self.subTest(field=field):
                changed = dict(config, **{field: value})
                self.assertNotEqual(self.reconcile(source, identity, row, changed)["status"], "PASS")

    def test_3_live_capture_seals_configuration_for_reconciliation(self):
        verdict, _ = pair(self.root, preset="vim")
        self.assertEqual(verdict["status"], "PASS")
        case = self.root / "foot/tui-list-text/primary"
        config = json.loads((case / "capture-config.json").read_text())
        identity = dict(commit="c", dirty_patch_sha256="p", elf_sha256="elf", references={}, untracked={})
        (self.root / "identity.json").write_text(json.dumps(identity))
        row = dict(terminal="foot", state="tui-list-text", variant="primary")
        self.assertEqual(self.reconcile(self.root, identity, row, config)["status"], "PASS")
        profile = case / "profile.json"
        data = json.loads(profile.read_text())
        data["font_size"] = 24
        profile.write_text(json.dumps(data))
        self.assertEqual(self.reconcile(self.root, identity, row, config)["status"], "BLOCKED")

    def test_3_main_routes_preset_and_font_constraints(self):
        source, case, identity, row, config = self.retained_case()
        binary = self.root / "candidate"
        binary.write_bytes(b"\x7fELFprobe")
        binary.chmod(0o755)
        reviews = self.root / "reviews.json"
        reviews.write_text(json.dumps({"foot/tui-list-text/primary": {"capture_sha256": digest(case / "native.png"),
            "board_sha256": digest(self.root / "boards/tui-list-text.png"), "approved": True}}))
        spec = dict(states=[config["state"]], primary_ids=[row["state"]], additional_states=[], excluded=[], reserved=[])
        profile = self.root / "profiles.json"
        profile.write_text(json.dumps({"foot": {"font_size": 24}}))
        for name, extra, expected in (("same", ["--preset", "vim"], 0),
                                      ("preset", ["--preset", "windows"], 2),
                                      ("font", ["--preset", "vim", "--profiles", str(profile)], 2)):
            with self.subTest(request=name), ExitStack() as stack:
                for target, value in (("contract", lambda *a: spec), ("palette", lambda *a: {}),
                                      ("source_identity", lambda *a: identity), ("supplement", lambda *a: [])):
                    stack.enter_context(patch.object(driver, target, value))
                stack.enter_context(patch.object(driver.os, "geteuid", return_value=1000))
                stack.enter_context(redirect_stdout(io.StringIO()))
                output = self.root / ("out-" + name)
                code = driver.main(["--boards", str(self.root), "--binary", str(binary), "--terminals", "foot",
                                    "--states", row["state"], "--reconcile", str(source), "--reviews", str(reviews),
                                    "--output", str(output), *extra])
                self.assertEqual(code, expected)
                manifest = json.loads((output / "manifest.json").read_text())
                self.assertEqual(manifest["native_acceptance"], expected == 0)


if __name__ == "__main__":
    unittest.main()
