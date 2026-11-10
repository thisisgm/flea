"""Exercise the native copy gate against real staged descriptors and kernel events."""
import ast
import ctypes
import os
from pathlib import Path
import select
import stat
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class StagedCopyGate(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="flea-copy-gate.", dir="/tmp")
        self.root = Path(self.sandbox.name)
        (self.root / ".flea-test-sandbox").touch()
        self.destination = self.root / "destination"
        self.destination.mkdir()
        script = Path(__file__).resolve().parents[1] / "ui-operations-design.sh"
        body = script.read_text().split("3<&0 <<'PY'\n", 1)[1].split("\nPY\n", 1)[0]
        program = ast.parse(body)
        functions = [node for node in program.body if isinstance(node, ast.FunctionDef)]
        scope = dict(os=os, Path=Path, stat=stat, sys=sys, process=Path(f"/proc/{os.getpid()}"),
                     destination=self.destination, partial=self.destination / "a-large.bin", total=4096)
        exec(compile(ast.Module(body=functions, type_ignores=[]), str(script), "exec"), scope)
        self.read_stage = scope["staged_file"]
        stopped = next(node for node in program.body if isinstance(node, ast.Try)).body
        start = next(index for index, node in enumerate(stopped) if isinstance(node, ast.Assign)
                     and any(isinstance(name, ast.Name) and name.id == "metadata" for name in node.targets))
        end = next(index for index in range(start, len(stopped)) if isinstance(stopped[index], ast.Expr)
                   and isinstance(stopped[index].value, ast.Call) and isinstance(stopped[index].value.func, ast.Name)
                   and stopped[index].value.func.id == "print")
        self.stopped_check = compile(ast.Module(body=stopped[start:end], type_ignores=[]), str(script), "exec")
        self.scope = scope
        self.source = self.root / "source"
        self.source.mkdir()
        (self.source / "a-large.bin").write_bytes(bytes(4096))
        self.addCleanup(self.sandbox.cleanup)

    def verify_stopped(self):
        descriptor = os.open(self.destination, os.O_TMPFILE | os.O_WRONLY, 0o600)
        self.addCleanup(os.close, descriptor)
        os.write(descriptor, b"partial")
        self.scope.update(stagefd=descriptor, source=self.source, mode="staged", later=self.destination / "b-after.txt")
        exec(self.stopped_check, self.scope)

    def test_stopped_backend_must_hold_the_expected_source_open_for_reading(self):
        with self.assertRaisesRegex(RuntimeError, "source"):
            self.verify_stopped()

    def test_other_or_write_only_source_descriptor_does_not_bind_the_copy(self):
        other = self.source / "other.bin"
        other.write_bytes(b"other")
        for path, flags in [(other, os.O_RDONLY), (self.source / "a-large.bin", os.O_WRONLY)]:
            with self.subTest(path=path, flags=flags):
                descriptor = os.open(path, flags)
                try:
                    with self.assertRaisesRegex(RuntimeError, "source"):
                        self.verify_stopped()
                finally:
                    os.close(descriptor)

    def test_exact_read_source_and_partial_staged_inode_are_accepted(self):
        descriptor = os.open(self.source / "a-large.bin", os.O_RDONLY)
        self.addCleanup(os.close, descriptor)
        self.verify_stopped()

    def test_candidate_gui_uses_the_resolved_window_record_without_environment_override(self):
        record = dict(pid=os.getpid(), frontend="gpui", gui="/candidate/release/flea-gui")
        window = SimpleNamespace(record=lambda: record)
        helper = Path(__file__).with_name("flea_transport_cli.py")
        with patch.dict(os.environ):
            os.environ.pop("FLEA_GUI_BIN", None)
            with patch("flea_transport_cli.select", return_value=(window, [])) as select_window:
                self.assertEqual(self.scope["candidate_gui"](str(helper), str(os.getpid())), record["gui"])
                select_window.assert_called_once_with(["--pid", str(os.getpid())])

    def test_candidate_gui_refuses_a_foreign_window_record(self):
        record = dict(pid=os.getpid() + 1, frontend="gpui", gui="/candidate/release/flea-gui")
        window = SimpleNamespace(record=lambda: record)
        helper = Path(__file__).with_name("flea_transport_cli.py")
        with patch("flea_transport_cli.select", return_value=(window, [])):
            with self.assertRaisesRegex(RuntimeError, "exact native window"):
                self.scope["candidate_gui"](str(helper), str(os.getpid()))

    def test_unnamed_partial_is_visible_to_inotify_and_bound_to_its_inode(self):
        libc = ctypes.CDLL(None, use_errno=True)
        watcher = libc.inotify_init1(os.O_NONBLOCK | os.O_CLOEXEC)
        self.assertGreaterEqual(watcher, 0)
        self.addCleanup(os.close, watcher)
        libc.inotify_add_watch.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_uint32]
        self.assertGreaterEqual(libc.inotify_add_watch(watcher, os.fsencode(self.destination), 0x122), 0)
        descriptor = os.open(self.destination, os.O_TMPFILE | os.O_WRONLY, 0o600)
        self.addCleanup(os.close, descriptor)
        os.write(descriptor, b"partial")
        self.assertTrue(select.select([watcher], [], [], 1)[0])
        self.assertTrue(os.read(watcher, 4096))
        held = self.read_stage()
        self.assertIsNotNone(held)
        self.addCleanup(os.close, held)
        self.assertEqual(os.fstat(held).st_ino, os.fstat(descriptor).st_ino)
        self.assertEqual(os.fstat(held).st_size, len(b"partial"))
        self.assertFalse((self.destination / "a-large.bin").exists())

    def test_completed_inode_is_refused(self):
        descriptor = os.open(self.destination, os.O_TMPFILE | os.O_WRONLY, 0o600)
        self.addCleanup(os.close, descriptor)
        os.write(descriptor, bytes(4096))
        self.assertIsNone(self.read_stage())

    def test_read_only_destination_and_foreign_writable_file_are_refused(self):
        target = self.destination / "a-large.bin"
        target.write_bytes(b"source")
        reader = os.open(target, os.O_RDONLY)
        writer = os.open(self.root / ".flea-part-foreign", os.O_WRONLY | os.O_CREAT, 0o600)
        self.addCleanup(os.close, reader)
        self.addCleanup(os.close, writer)
        self.assertIsNone(self.read_stage())


if __name__ == "__main__":
    unittest.main()
