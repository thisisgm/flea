#!/usr/bin/env python3
"""Exercise the menu clipboard oracle against real copied and moved files."""
import importlib.util
import json
import os
from pathlib import Path
import shutil
import tempfile
import unittest

HELPER_PATH = Path(__file__).with_name("menu-clipboard-checks.py")
SPEC = importlib.util.spec_from_file_location("clipboard_checks", HELPER_PATH)
CHECKS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECKS)


class ClipboardFilesTest(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory()
        self.addCleanup(self.scratch.cleanup)
        self.source = Path(self.scratch.name) / "source"
        self.destination = Path(self.scratch.name) / "destination"
        self.source.mkdir()
        self.destination.mkdir()
        for name in CHECKS.SELECTED_NAMES:
            (self.source / name).write_text(name)
            shutil.copyfile(self.source / name, self.destination / name)
        self.source_bytes = {name: (self.source / name).read_bytes() for name in CHECKS.SELECTED_NAMES}

    def test_copy_rejects_each_wrong_bytes_destination(self):
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                (self.destination / name).write_bytes(b"wrong contents")
                with self.assertRaisesRegex(ValueError, name + ".*bytes"):
                    CHECKS.files("copy", self.source, self.destination, self.source_bytes)
                (self.destination / name).write_bytes(self.source_bytes[name])

    def test_recorded_bytes_survive_cut_and_decode_for_the_file_check(self):
        snapshot = Path(self.scratch.name) / "source-bytes.json"
        CHECKS.record_bytes(self.source, snapshot)
        for name in CHECKS.SELECTED_NAMES:
            (self.source / name).unlink()
        recorded = json.loads(snapshot.read_text())
        contents = {name: bytes.fromhex(recorded[name]) for name in CHECKS.SELECTED_NAMES}
        self.assertEqual(contents, self.source_bytes)
        CHECKS.files("cut", self.source, self.destination, contents)

    def test_cut_rejects_each_wrong_bytes_destination_after_sources_removed(self):
        for name in CHECKS.SELECTED_NAMES:
            (self.source / name).unlink()
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                (self.destination / name).write_bytes(b"wrong contents")
                with self.assertRaisesRegex(ValueError, name + ".*bytes"):
                    CHECKS.files("cut", self.source, self.destination, self.source_bytes)
                (self.destination / name).write_bytes(self.source_bytes[name])

    def test_terminal_log_records_both_expected_directories(self):
        log = Path(self.scratch.name) / "terminals"
        log.write_text(str(self.source) + "\n" + str(self.destination) + "\n")
        CHECKS.terminals(log, self.source, self.destination)

    def test_terminal_log_rejects_missing_wrong_duplicate_and_reordered_opens(self):
        log = Path(self.scratch.name) / "terminals"
        invalid = [[], [self.source], [self.source, self.source],
                   [self.destination, self.source], [self.source, self.destination, self.destination]]
        for directories in invalid:
            with self.subTest(directories=directories):
                log.write_text("".join(str(path) + "\n" for path in directories))
                with self.assertRaisesRegex(ValueError, "terminal argv"):
                    CHECKS.terminals(log, self.source, self.destination)

    def test_hunt_reads_terminal_argv_log(self):
        hunt = HELPER_PATH.with_name("menu-clipboard-hunt.sh").read_text()
        self.assertIn('tests/menu-clipboard-checks.py terminals "$action"', hunt)

    def test_hunt_expects_relative_path_between_path_and_name(self):
        hunt = HELPER_PATH.with_name("menu-clipboard-hunt.sh").read_text()
        qml = HELPER_PATH.with_name("menu-clipboard-hunt.qml").read_text()
        self.assertIn('["copyPath", "copyRelative", "copyName"', qml)
        self.assertIn('root / "sub" / "alpha.txt"', hunt)
        self.assertIn('"sub/" + pathlib.Path(p).name', hunt)
        self.assertIn("Relative path matched Name", hunt)
        self.assertIn("seven Copy as leaves", hunt)

    def test_publication_accepts_no_wl_copy_calls_or_text_only_calls(self):
        calls = Path(self.scratch.name) / "clipboard.calls"
        for action in ("copy", "cut"):
            for text in ("", json.dumps({"args": [], "text": "ordinary text"}) + "\n"):
                with self.subTest(action=action, text=text):
                    calls.write_text(text)
                    CHECKS.publication(action, calls, self.source)

    def test_publication_rejects_file_uris_or_file_mime_after_a_text_call(self):
        calls = Path(self.scratch.name) / "clipboard.calls"
        good = {"args": [], "text": "ordinary text"}
        paths = [(self.source / name).as_uri() for name in CHECKS.SELECTED_NAMES]
        bad_calls = [
            {"args": [], "text": "\n".join(paths)},
            {"args": ["--type", CHECKS.FILE_CLIPBOARD_MIME], "text": "copy\n" + paths[0]},
            {"args": ["--type", "text/uri-list"], "text": ""},
            {"args": [], "text": "cut\nfile:///another/window/file"},
        ]
        for action in ("copy", "cut"):
            for bad in bad_calls:
                with self.subTest(action=action, bad=bad):
                    calls.write_text(json.dumps(good) + "\n" + json.dumps(bad) + "\n")
                    with self.assertRaisesRegex(ValueError, "never wl-copy"):
                        CHECKS.publication(action, calls, self.source)

    def make_links(self, action):
        for name in CHECKS.SELECTED_NAMES:
            dest = self.destination / name
            dest.unlink()
            if action == "pasteas-hard":
                os.link(self.source / name, dest)
            else:
                target = self.source / name
                if action == "pasteas":
                    target = Path(os.path.relpath(target, self.destination))
                dest.symlink_to(target)

    def test_pasteas_accepts_both_relative_absolute_and_hard_links(self):
        for action in ("pasteas", "pasteas-absolute", "pasteas-hard"):
            with self.subTest(action=action):
                self.make_links(action)
                CHECKS.files(action, self.source, self.destination, self.source_bytes)

    def test_pasteas_rejects_each_missing_link_and_plain_copy(self):
        for action in ("pasteas", "pasteas-absolute", "pasteas-hard"):
            for name in CHECKS.SELECTED_NAMES:
                with self.subTest(action=action, name=name):
                    self.make_links(action)
                    dest = self.destination / name
                    dest.unlink()
                    with self.assertRaises(ValueError):
                        CHECKS.files(action, self.source, self.destination, self.source_bytes)
                    shutil.copyfile(self.source / name, dest)
                    with self.assertRaises(ValueError):
                        CHECKS.files(action, self.source, self.destination, self.source_bytes)

    def test_pasteas_rejects_each_wrong_target_and_target_form(self):
        for action in ("pasteas", "pasteas-absolute"):
            for name in CHECKS.SELECTED_NAMES:
                with self.subTest(action=action, name=name):
                    self.make_links(action)
                    dest = self.destination / name
                    dest.unlink()
                    other = next(other for other in CHECKS.SELECTED_NAMES if other != name)
                    target = self.source / other
                    if action == "pasteas":
                        target = Path(os.path.relpath(target, self.destination))
                    dest.symlink_to(target)
                    with self.assertRaisesRegex(ValueError, "selected source"):
                        CHECKS.files(action, self.source, self.destination, self.source_bytes)
                    dest.unlink()
                    target = self.source / name
                    if action == "pasteas-absolute":
                        target = Path(os.path.relpath(target, self.destination))
                    dest.symlink_to(target)
                    with self.assertRaisesRegex(ValueError, "target form"):
                        CHECKS.files(action, self.source, self.destination, self.source_bytes)

    def test_pasteas_hard_rejects_each_symlink_and_wrong_source_inode(self):
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                self.make_links("pasteas-hard")
                dest = self.destination / name
                dest.unlink()
                dest.symlink_to(self.source / name)
                with self.assertRaisesRegex(ValueError, "regular hard link"):
                    CHECKS.files("pasteas-hard", self.source, self.destination, self.source_bytes)
                dest.unlink()
                other = next(other for other in CHECKS.SELECTED_NAMES if other != name)
                os.link(self.source / other, dest)
                with self.assertRaisesRegex(ValueError, "source inode"):
                    CHECKS.files("pasteas-hard", self.source, self.destination, self.source_bytes)

    def test_copy_retains_both_sources(self):
        CHECKS.files("copy", self.source, self.destination, self.source_bytes)

    def test_copy_rejects_both_sources_moved(self):
        for name in CHECKS.SELECTED_NAMES:
            (self.source / name).replace(self.destination / name)
        with self.assertRaisesRegex(ValueError, "alpha.txt.*Copy source"):
            CHECKS.files("copy", self.source, self.destination, self.source_bytes)

    def test_copy_requires_each_source(self):
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                (self.source / name).replace(self.destination / name)
                with self.assertRaisesRegex(ValueError, name + ".*Copy source"):
                    CHECKS.files("copy", self.source, self.destination, self.source_bytes)
                shutil.copyfile(self.destination / name, self.source / name)

    def test_cut_removes_both_sources(self):
        for name in CHECKS.SELECTED_NAMES:
            (self.source / name).replace(self.destination / name)
        CHECKS.files("cut", self.source, self.destination, self.source_bytes)

    def test_cut_rejects_each_retained_source(self):
        for name in CHECKS.SELECTED_NAMES:
            (self.source / name).unlink()
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                shutil.copyfile(self.destination / name, self.source / name)
                with self.assertRaisesRegex(ValueError, name + ".*Cut source"):
                    CHECKS.files("cut", self.source, self.destination, self.source_bytes)
                (self.source / name).unlink()

    def test_copy_requires_each_destination(self):
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                (self.destination / name).unlink()
                with self.assertRaisesRegex(ValueError, name + ".*destination file"):
                    CHECKS.files("copy", self.source, self.destination, self.source_bytes)
                shutil.copyfile(self.source / name, self.destination / name)

    def test_copy_rejects_symlink_destinations(self):
        for name in CHECKS.SELECTED_NAMES:
            with self.subTest(name=name):
                (self.destination / name).unlink()
                (self.destination / name).symlink_to(self.source / name)
                with self.assertRaisesRegex(ValueError, name + ".*destination file"):
                    CHECKS.files("copy", self.source, self.destination, self.source_bytes)
                (self.destination / name).unlink()
                shutil.copyfile(self.source / name, self.destination / name)


if __name__ == "__main__":
    unittest.main()
