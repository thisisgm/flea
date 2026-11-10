"""Picker runtimes stay inside an owned marked boundary and fit native sockets."""
import ast
import os
from pathlib import Path
import stat
import tempfile
import unittest


class PickerRuntime(unittest.TestCase):
    def setUp(self):
        self.scratch = tempfile.TemporaryDirectory(prefix="flea-picker-runtime.", dir="/tmp")
        self.root = Path(self.scratch.name)
        (self.root / ".flea-test-sandbox").write_text("picker runtime controls\n")
        script = Path(__file__).resolve().parents[1] / "ui-captures-cohesion-picker.py"
        tree = ast.parse(script.read_text())
        helpers = [node for node in tree.body if isinstance(node, ast.FunctionDef)
                   and node.name in ("socket_fits", "private_runtime")]
        self.assertEqual(len(helpers), 2)
        self.scope = dict(Path=Path, os=os, stat=stat, tempfile=tempfile,
                          QS_INSTANCE_ID_CHARS=11, SOCKET_PATH_MAX_BYTES=107)
        exec(compile(ast.Module(body=helpers, type_ignores=[]), str(script), "exec"), self.scope)

    def tearDown(self):
        self.root.chmod(0o700)
        self.scratch.cleanup()

    def test_deep_evidence_uses_its_marked_ancestor(self):
        deep = self.root / ("evidence-" * 12)
        deep.mkdir(mode=0o700)
        (deep / ".flea-test-sandbox").write_text("marked but too deep\n")
        self.assertFalse(self.scope["socket_fits"](deep / "pkr.xxxxxxxx"))
        runtime = self.scope["private_runtime"](deep)
        self.assertEqual(runtime.parent, self.root)
        self.assertEqual(stat.S_IMODE(runtime.stat().st_mode), 0o700)
        self.assertTrue((runtime / ".flea-test-sandbox").is_file())
        self.assertTrue(self.scope["socket_fits"](runtime))

    def test_permissive_boundary_is_refused(self):
        self.root.chmod(0o755)
        with self.assertRaisesRegex(AssertionError, "no owned marked runtime ancestor"):
            self.scope["private_runtime"](self.root)


if __name__ == "__main__":
    unittest.main()
