"""Clipboard cleanup must accept only the exact private candidate owner."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "cleanup", Path(__file__).with_name("flea_live_ops_cleanup.py"))
cleanup = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cleanup)


class OwnershipControls(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="flea-live-ops-controls-")
        self.addCleanup(self.sandbox.cleanup)
        self.root = Path(self.sandbox.name)
        (self.root / ".flea-test-sandbox").touch()
        self.process = self.root / "process"
        self.process.mkdir()
        self.binary = self.root / "release/flea"
        self.arguments = [os.fsencode(self.binary), b"--clip-own"]
        self.environment = {
            b"FLEA_UNDO_DIR": os.fsencode(self.root / "undo"),
            b"XDG_DATA_HOME": os.fsencode(self.root / "data"),
            b"FLEA_TEST_WINDOWS_ROOT": os.fsencode(self.root / "windows"),
        }
        self.write_process()
        (self.process / "exe").symlink_to(self.binary)

    def write_process(self):
        (self.process / "cmdline").write_bytes(b"\0".join(self.arguments) + b"\0")
        (self.process / "environ").write_bytes(b"\0".join(
            name + b"=" + value for name, value in self.environment.items()) + b"\0")

    def test_exact_private_owner_is_accepted(self):
        self.assertTrue(cleanup.owned(self.process, self.root, self.binary))

    def test_each_private_marker_is_required_and_exact(self):
        for name in self.environment:
            original = self.environment[name]
            for replacement in (b"", original + b"-other"):
                with self.subTest(name=name, replacement=replacement):
                    self.environment[name] = replacement
                    self.write_process()
                    self.assertFalse(cleanup.owned(self.process, self.root, self.binary))
            self.environment[name] = original

    def test_other_mode_binary_and_extra_arguments_are_refused(self):
        original = self.arguments[:]
        for arguments in ([original[0], b"--other"],
                          [original[0] + b"-other", original[1]],
                          original + [b"extra"]):
            with self.subTest(arguments=arguments):
                self.arguments = arguments
                self.write_process()
                self.assertFalse(cleanup.owned(self.process, self.root, self.binary))

    def test_executable_must_match_even_when_argv_matches(self):
        (self.process / "exe").unlink()
        (self.process / "exe").symlink_to(self.binary.with_name("other"))
        self.assertFalse(cleanup.owned(self.process, self.root, self.binary))

    def test_departed_process_is_ignored(self):
        self.assertFalse(cleanup.owned(self.root / "absent", self.root, self.binary))


if __name__ == "__main__":
    unittest.main()
