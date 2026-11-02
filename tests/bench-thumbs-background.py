"""Offline contracts for the listed wallpaper preflight module and CLI."""

import importlib.util
import contextlib
import io
import json
import os
from pathlib import Path
import struct
import tempfile
import unittest

REPO = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("background_preflight", REPO / "tools/flea-bench-thumbs-lib/background_preflight.py")
guard = importlib.util.module_from_spec(spec)
spec.loader.exec_module(guard)
ROOT = Path(os.environ["FLEA_B7_TEST_ROOT"])

class Background(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="background-offline-", dir=ROOT))
        (self.root / ".flea-test-sandbox").touch()
        self.home = self.root / "home"
        self.current = self.home / ".local/state/omarchy/current"
        self.current.mkdir(parents=True)
        self.theme = self.root / "first-theme"
        (self.theme / "backgrounds").mkdir(parents=True)
        (self.root / "second-theme/backgrounds").mkdir(parents=True)
        (self.current / "theme").symlink_to(self.theme, target_is_directory=True)
        self.proc = self.root / "proc"
        self.proc.mkdir()
        self.picture("Still 01.PNG")

    def picture(self, name, content=b"offline still image"):
        path = self.theme / "backgrounds" / name
        path.write_bytes(content)
        (self.root / "second-theme/backgrounds" / name).write_bytes(content)
        link = self.current / "background"
        if link.is_symlink():
            link.unlink()
        link.symlink_to(path)
        return path

    def ticks(self, user, system, start=100):
        directory = self.proc / "123"
        directory.mkdir(exist_ok=True)
        fields = ["S"] + ["0"] * 49
        fields[11], fields[12], fields[19] = str(user), str(system), str(start)
        (directory / "stat").write_text("123 (owe-render) " + " ".join(fields))

    def probe(self, pause=lambda seconds: None):
        bg = (self.current / "background").resolve().name
        return guard.probe(self.home, bg, self.proc, pause)

    def test_video_suffixes_are_refused_and_both_readings_recorded(self):
        for suffix in ("mp4", "webm", "mkv", "mov", "gif", "MP4"):
            self.picture("default." + suffix)
            result = self.probe()
            self.assertFalse(result["passed"], result)
            self.assertIn("B7_BACKGROUND_ANIMATED", " ".join(result["failures"]))
            self.assertIn("first", result)
            self.assertIn("second", result)

    def test_animated_webp_chunks_and_flags_are_refused(self):
        def webp(flags=0, animated=False):
            chunks = b"VP8X" + struct.pack("<I", 10) + bytes([flags]) + b"\0" * 9
            if animated:
                chunks += b"ANIM" + struct.pack("<I", 6) + b"\0" * 6
            return b"RIFF" + struct.pack("<I", len(chunks) + 4) + b"WEBP" + chunks
        self.picture("still.webp", webp())
        self.assertTrue(self.probe()["passed"])
        for flags, animated in ((0, True), (2, False)):
            self.picture("renamed.jpg", webp(flags, animated))
            self.assertFalse(self.probe()["passed"])

    def test_proc_fields_and_strict_greater_than_two_tick_threshold(self):
        self.ticks(10, 5)
        result = self.probe(lambda seconds: self.ticks(11, 6))
        self.assertTrue(result["passed"], result)
        self.assertEqual(result["first"]["processes"]["123"]["utime"], 10)
        self.assertEqual(result["first"]["processes"]["123"]["stime"], 5)
        self.assertEqual(result["second"]["processes"]["123"]["total_ticks"], 17)
        self.ticks(10, 5)
        result = self.probe(lambda seconds: self.ticks(12, 6))
        self.assertFalse(result["passed"])
        self.assertEqual(result["deltas"]["123"], 3)
        self.assertIn("B7_BACKGROUND_OWE_BUSY", " ".join(result["failures"]))

    def test_replaced_process_and_background_race_are_refused(self):
        self.ticks(10, 5)
        self.assertFalse(self.probe(lambda seconds: self.ticks(10, 5, start=200))["passed"])
        result = self.probe(lambda seconds: self.picture("unexpected.mp4"))
        self.assertIn("B7_BACKGROUND_CHANGED", " ".join(result["failures"]))

    def cli(self):
        from unittest import mock
        output, error = io.StringIO(), io.StringIO()
        with mock.patch.dict(os.environ, HOME=str(self.home)), contextlib.redirect_stdout(output), contextlib.redirect_stderr(error):
            result = guard.main([str(self.root), (self.current / "background").resolve().name],
                                proc_root=self.proc, pause=lambda seconds: None)
        self.cli_output = output.getvalue(), error.getvalue()
        return result

    def test_cli_refuses_video_and_records_both_readings(self):
        self.picture("default.mp4")
        self.assertEqual(self.cli(), 2)
        self.assertIn("FAIL B7_BACKGROUND_ANIMATED", self.cli_output[1])
        proof = json.loads((self.root / "background.json").read_text())
        self.assertFalse(proof["passed"])
        self.assertEqual((proof["first"]["processes"], proof["second"]["processes"]), ({}, {}))

    def test_cli_accepts_still_and_never_overwrites_its_evidence(self):
        self.assertEqual(self.cli(), 0)
        proof = json.loads((self.root / "background.json").read_text())
        self.assertTrue(proof["passed"])
        with self.assertRaises(FileExistsError):
            self.cli()

    def test_cli_rejects_unmarked_evidence_root(self):
        unmarked = self.root / "unmarked"
        unmarked.mkdir()
        error = io.StringIO()
        with contextlib.redirect_stderr(error):
            result = guard.main([str(unmarked), "Still 01.PNG"], proc_root=self.proc, pause=lambda seconds: None)
        self.assertEqual(result, 2)
        self.assertIn("FAIL B7_BACKGROUND_ROOT", error.getvalue())
        self.assertFalse((unmarked / "background.json").exists())


if __name__ == "__main__":
    result = unittest.main(exit=False)
    print(f"bench-thumbs-background: {result.result.testsRun} offline contracts, {len(result.result.failures) + len(result.result.errors)} failed")
    raise SystemExit(not result.result.wasSuccessful())
