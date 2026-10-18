"""Behavioral reproductions for all seven first-round review findings."""
import base64
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from .cells import Screen
from .driver import contract
from .evidence import Blocked, MARKER, digest
from .media import inspect as inspect_media, transport_for
from .protocol import capabilities
from .reconcile import retained
from .review_support import board_file, pair, media_transcript

HERE = Path(__file__).resolve().parent


class ReviewTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scratch = Path(tempfile.mkdtemp(prefix="review-r1.", dir=Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))))
        (cls.scratch / MARKER).write_text("review regression checks\n")

    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="case.", dir=self.scratch))
        (self.root / MARKER).write_text("owned case\n")

    def test_1_convert_inventory_does_not_block_original_scope(self):
        spec = json.loads((HERE / "states.json").read_text())
        for state in spec["states"] + spec["excluded"]:
            board_file(self.root, state["id"])
        for name in ("tui-convert", "tui-convert-pdf"):
            board_file(self.root, name)
        self.assertEqual(len(contract(self.root)["states"]), 63)
        self.assertEqual([s["id"] for s in contract(self.root, "tui-list-text")["states"]], ["tui-list-text"])
        converts = contract(self.root, "tui-convert,tui-convert-pdf")["states"]
        self.assertEqual(len(converts), 2)
        self.assertTrue(all(s["requires"] for s in converts))

    def test_2_requested_presets_reach_candidate_settings(self):
        for preset in ("vim", "mac", "windows"):
            with self.subTest(preset=preset):
                case = self.root / preset
                case.mkdir()
                verdict, settings = pair(case, preset=preset)
                self.assertEqual(verdict["status"], "PASS")
                self.assertEqual(settings.get("keys"), preset)

    def test_3_mime_kind_passes_but_wrong_kind_fails(self):
        verdict, _ = pair(self.root, state_id="tui-properties")
        self.assertEqual(verdict["cells"]["geometry"]["status"], "PASS")
        self.assertEqual(verdict["status"], "PASS")
        wrong = self.root / "wrong-kind"
        wrong.mkdir()
        verdict, _ = pair(wrong, state_id="tui-properties", wrong_kind=True)
        self.assertEqual(verdict["status"], "FAIL")

    def test_4_unwanted_renditions_fail_color(self):
        board = board_file(self.root)
        for sgr in (7, 2, 4):
            with self.subTest(sgr=sgr):
                screen = Screen(10, 4)
                screen.feed(f"\x1b[H\x1b[{sgr}mA".encode())
                result = board.compare(screen.snapshot())
                self.assertEqual(result["geometry"]["status"], "PASS")
                self.assertEqual(result["color"]["status"], "FAIL")

    def test_4_equivalent_reverse_and_expected_underline_pass(self):
        board = board_file(self.root)
        screen = Screen(10, 4)
        screen.feed(b"\x1b[H\x1b[38;2;28;18;13m\x1b[48;2;198;179;149m\x1b[7mA")
        self.assertEqual(board.compare(screen.snapshot())["color"]["status"], "PASS")
        board = board_file(self.root, decoration="text-decoration:underline")
        screen = Screen(10, 4)
        screen.feed(b"\x1b[H\x1b[4mA")
        self.assertEqual(board.compare(screen.snapshot())["color"]["status"], "PASS")
        screen.feed(b"\x1b[H\x1b[24mA")
        self.assertEqual(board.compare(screen.snapshot())["color"]["status"], "FAIL")

    def test_5_primary_protocol_loss_cannot_select_fallback(self):
        board = board_file(self.root, "tui-list-image")
        board_file(self.root, "tui-image-chafa", "▀")
        board.pictures = [{"origin": [4, 9], "resolved_style": {"width": "8.4px", "height": "22px"}}]
        caps = {"candidate_received": {"kitty_positive": False, "sixel_positive": False}, "forced_fallback": False}
        (self.root / "graphics.log").write_text("")
        (self.root / "pty.log").write_text("")
        state = {"id": "tui-list-image", "cwd": "/fx/v/work"}
        image = (100, 100, bytearray(value for y in range(100) for x in range(100) for value in (x, y, 40, 255)))
        for adapter, transport in (("kitty", "kitty"), ("ghostty", "kitty"), ("foot", "sixel")):
            with self.subTest(adapter=adapter), self.assertRaisesRegex(Blocked, f"{adapter} requires actual {transport} negotiation"):
                inspect_media(state, board, Screen(10, 4).snapshot(), image,
                              [0, 0, 100, 100], caps, self.root, self.root, self.root, {}, adapter=adapter, expected_transport=transport)

    def media_case(self, adapter, forced=False, traffic="", kitty=False, sixel=False):
        board = board_file(self.root, "tui-list-image")
        board_file(self.root, "tui-image-chafa", "▀")
        board.pictures = [{"origin": [4, 9], "resolved_style": {"width": "16.8px", "height": "22px"}}]
        pixels = bytearray(value for y in range(100) for x in range(100) for value in (x, y, 40, 255))
        received = media_transcript(self.root, [{"ns": 30, "data": traffic}], kitty=kitty, sixel=sixel)
        caps = {"candidate_received": received, "forced_fallback": forced}
        state = {"id": "tui-list-image", "cwd": "/fx/v/work", "cursor": "photo.png"}
        (self.root / "v/work").mkdir(parents=True, exist_ok=True)
        (self.root / "v/work/photo.png").write_bytes(b"fixture identity")
        arguments = (state, board, Screen(10, 4).snapshot(), (100, 100, pixels), [4, 9, 84, 88], caps, self.root, self.root, self.root)
        keywords = dict(adapter=adapter, expected_transport=transport_for(adapter, forced))
        # The controller approves the picture for the identities this run produced; placements are still judged.
        first = inspect_media(*arguments, {}, **keywords)[0]
        approval = {"representation": dict(approved=True, **first["review_requirements"])} if "review_requirements" in first else {}
        return inspect_media(*arguments, approval, **keywords)

    def test_5_transport_specific_placement_and_primary_paths(self):
        kitty = "\x1b_Ga=T,i=7;AAAA\x1b\\"
        sixel = "\x1bP0;1q#1;2;100;0;0~~~~\x1b\\"
        for adapter, traffic, flags in (("kitty", kitty, {"kitty": True}), ("ghostty", kitty, {"kitty": True}),
                                        ("foot", sixel, {"sixel": True})):
            with self.subTest(adapter=adapter):
                self.assertEqual(self.media_case(adapter, traffic=traffic, **flags)[0]["status"], "PASS")
                with self.assertRaisesRegex(Blocked, "required.*traffic"):
                    self.media_case(adapter, traffic=sixel if adapter != "foot" else kitty, **flags)
                with self.assertRaisesRegex(Blocked, "required.*traffic"):
                    self.media_case(adapter, traffic="\x1b_Gi=7,a=q;AAAA\x1b\\", **flags)

    def test_5_explicit_fallbacks_remain_separate(self):
        for adapter in ("alacritty", "tmux"):
            with self.subTest(adapter=adapter):
                result, reference, _ = self.media_case(adapter)
                self.assertEqual((result["status"], result["mode"], result["forced"]), ("PASS", "half-blocks", False))
                self.assertIsNotNone(reference)
        for adapter in ("alacritty", "foot", "ghostty", "kitty", "tmux"):
            with self.subTest(adapter=adapter):
                result, _, _ = self.media_case(adapter, forced=True)
                self.assertEqual((result["mode"], result["forced"]), ("half-blocks", True))
                with self.assertRaisesRegex(Blocked, "unexpectedly enabled"):
                    self.media_case(adapter, forced=True, kitty=True)

    def test_6_failed_terminal_fails_pair(self):
        verdict, _ = pair(self.root, exit_code=73)
        self.assertEqual(verdict["terminal_exit"]["exit_code"], 73)
        self.assertEqual(verdict["status"], "FAIL")

    def test_6_terminal_signals_and_missing_exit_are_failures(self):
        for code in (-9, None, False):
            with self.subTest(code=code):
                root = self.root / str(code)
                root.mkdir()
                self.assertEqual(pair(root, exit_code=code)[0]["status"], "FAIL")

    def test_6_retained_failed_terminal_cannot_pass(self):
        board_file(self.root)
        source = self.root / "retained"
        directory = source / "foot/tui-list-text/primary"
        directory.mkdir(parents=True)
        identity = dict(commit="c", dirty_patch_sha256="p", elf_sha256="e", references={}, untracked={})
        (source / "identity.json").write_text(json.dumps(identity))
        names = ("native.png", "grid-crop.png", "cells.json", "pty.log", "capabilities.json", "state.json", "commands.json", "diff.png", "overlay.png", "media.json")
        for name in names:
            (directory / name).write_text(json.dumps({"status": "PASS"}))
        row = dict(terminal="foot", state="tui-list-text", variant="primary")
        verdict = dict(row, behavior={"status": "PASS"}, cells={"geometry": {"status": "PASS"}, "color": {"status": "PASS"}},
                       pixels={"status": "REVIEW"}, terminal_exit={"exit_code": 73},
                       shutdown={"candidate_exit": 0, "termios_restored": True, "relay_termios_restored": True},
                       artifact_sha256={name: digest(directory / name) for name in names})
        (directory / "verdict.json").write_text(json.dumps(verdict))
        with patch("tui_battery.reconcile.visual_review", return_value={"status": "PASS"}):
            self.assertEqual(retained(source, identity, [row], self.root, {})[0]["status"], "FAIL")

    def test_7_fragmented_stale_reply_is_not_current(self):
        log = self.root / "pty.log"
        def write(rows):
            log.write_text("\n".join(json.dumps({"direction": direction, "ns": i,
                "base64": base64.b64encode(data).decode()}) for i, (direction, data) in enumerate(rows)))
        query = ("output", b"\x1b_Gi=7,a=q;AAAA\x1b\\")
        beginning = ("terminal-input", b"\x1b_Gi=7;")
        ending = ("terminal-input", b"OK\x1b\\")
        write([beginning, query, ending])
        self.assertFalse(capabilities(log)["kitty_positive"])
        write([query, beginning, ending])
        self.assertTrue(capabilities(log)["kitty_positive"])

    def test_7_other_fragmented_capabilities_and_query_generations(self):
        log = self.root / "pty.log"
        for query, head, tail, key in ((b"\x1b[c", b"\x1b[?1;", b"4c", "sixel_positive"),
                                       (b"\x1b[?u", b"\x1b[?", b"3u", "keyboard_flags")):
            for stale in (True, False):
                with self.subTest(key=key, stale=stale):
                    pieces = [("terminal-input", head), ("output", query), ("terminal-input", tail)]
                    if not stale:
                        pieces[0], pieces[1] = pieces[1], pieces[0]
                    log.write_text("\n".join(json.dumps({"direction": d, "base64": base64.b64encode(b).decode()}) for d, b in pieces))
                    self.assertEqual(bool(capabilities(log)[key]), not stale)
        query = b"\x1b_Gi=7,a=q;AAAA\x1b\\"
        pieces = [("output", query), ("terminal-input", b"\x1b_Gi=7;"), ("output", query), ("terminal-input", b"OK\x1b\\")]
        log.write_text("\n".join(json.dumps({"direction": d, "base64": base64.b64encode(b).decode()}) for d, b in pieces))
        result = capabilities(log)
        self.assertFalse(result["kitty_positive"])
        self.assertLess(result["kitty_replies"][0]["reply_start_event"], result["kitty_replies"][0]["query_event"])


if __name__ == "__main__":
    unittest.main()
