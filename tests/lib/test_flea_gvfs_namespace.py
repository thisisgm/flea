"""The phone namespace receives only its owned runtime through an inherited descriptor."""
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


class RuntimeControls(unittest.TestCase):
    def setUp(self):
        self.sandbox = tempfile.TemporaryDirectory(prefix="flea-gvfs-controls-")
        self.addCleanup(self.sandbox.cleanup)
        self.root = Path(self.sandbox.name)
        (self.root / ".flea-test-sandbox").touch()
        self.window = self.root / "owned window"
        self.runtime = self.window / "runtime"
        self.runtime.mkdir(parents=True, mode=0o700)
        (self.runtime / ".flea-test-sandbox").touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.probe = self.bin / "unshare"
        self.probe.write_text('''#!/usr/bin/python3
import json, os, pathlib, sys
runtime = pathlib.Path(os.environ["XDG_RUNTIME_DIR"])
assert str(runtime).startswith("/proc/self/fd/")
assert runtime.resolve() == pathlib.Path(os.environ["FLEA_TEST_WINDOW"]) / "runtime"
assert (runtime / ".flea-test-sandbox").is_file()
print(json.dumps(sys.argv[1:]))
''')
        self.probe.chmod(0o700)
        self.helper = Path(__file__).with_name("flea-gvfs-namespace.sh")

    def run_helper(self, runtime=None, window=None):
        environment = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}",
                           XDG_RUNTIME_DIR=str(runtime or self.runtime),
                           FLEA_TEST_WINDOW=str(window or self.window))
        return subprocess.run(["bash", str(self.helper), "--enter", "source with space", "target",
                               "user:[123]", "mnt:[456]", "/bin/true", "literal argument"],
                              env=environment, capture_output=True, text=True, timeout=5)

    def test_descriptor_survives_exec_and_preserves_arguments(self):
        result = self.run_helper()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), [
            "--user", "--map-current-user", "--keep-caps", "--mount", "--propagation", "private",
            "/bin/bash", str(self.helper), "source with space", "target", "user:[123]", "mnt:[456]",
            "/bin/true", "literal argument"])

    def test_foreign_window_runtime_is_refused(self):
        self.assertNotEqual(self.run_helper(window=self.root / "other").returncode, 0)

    def test_unmarked_runtime_is_refused(self):
        (self.runtime / ".flea-test-sandbox").unlink()
        self.assertNotEqual(self.run_helper().returncode, 0)

    def test_public_runtime_is_refused(self):
        self.runtime.chmod(0o755)
        self.assertNotEqual(self.run_helper().returncode, 0)

    def test_symlink_marker_is_refused(self):
        marker = self.runtime / ".flea-test-sandbox"
        marker.unlink()
        marker.symlink_to(self.root / ".flea-test-sandbox")
        self.assertNotEqual(self.run_helper().returncode, 0)

    def run_inner(self, source, target, namespaces=("user:[123]", "mnt:[456]")):
        # A mount stub stands in, so a guard that lets one through records it instead of mounting.
        (self.bin / "mount").write_text(f"#!/bin/sh\n: > {self.root / 'mounted'}\nexit 97\n")
        (self.bin / "mount").chmod(0o700)
        environment = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}")
        return subprocess.run(["bash", str(self.helper), str(source), str(target), *namespaces, "/bin/true"],
                              env=environment, capture_output=True, text=True, timeout=5)

    def assert_refused(self, result, reason):
        self.assertEqual((result.returncode, reason in result.stderr), (2, True), result.stderr)
        self.assertFalse((self.root / "mounted").exists())

    def test_inner_stage_in_the_callers_namespaces_is_refused(self):
        own = (os.readlink("/proc/self/ns/user"), os.readlink("/proc/self/ns/mnt"))
        self.assert_refused(self.run_inner(self.root, "/run/user/0/gvfs", own), "independent user and mount")

    def test_inner_stage_refuses_a_relative_unmarked_or_linked_source(self):
        unmarked = self.root / "unmarked"
        unmarked.mkdir()
        linked = self.root / "linked"
        linked.symlink_to(self.root)
        for source in ("relative", unmarked, linked):
            with self.subTest(source=source):
                self.assert_refused(self.run_inner(source, "/run/user/0/gvfs"), "absolute marked directory")

    def test_inner_stage_refuses_a_non_canonical_target(self):
        for target in (self.root, "/run/user/0/gvfs/../gvfs", "/run/user/x/gvfs"):
            with self.subTest(target=target):
                self.assert_refused(self.run_inner(self.root, target), "canonical GVFS directory")


if __name__ == "__main__":
    unittest.main()
