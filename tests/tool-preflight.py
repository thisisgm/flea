#!/usr/bin/env python3
"""Check contributor refusals without touching a compositor, cache, or real fixture."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


REPO = Path(__file__).resolve().parent.parent
# Exit statuses of the stubs: the guard stub stands in for fixture setup, the headless stubs for dispatch.
GUARD_REACHED = 73
HEADLESS_DISPATCHED = 74
STUB_EXIT = 89


class ToolPreflight(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="flea-tool-preflight-"))
        self.addCleanup(self.remove_root)
        (self.root / ".flea-test-sandbox").touch()
        self.repo = self.root / "repo"
        self.bin = self.root / "bin"
        self.home = self.root / "home"
        for directory in (self.repo / "tests/lib", self.repo / "tools", self.bin,
                          self.home / ".local/bin"):
            directory.mkdir(parents=True)
        for name in ("tests/ui.sh", "tests/drag.sh", "tools/flea-field-bench",
                     "tools/flea-ipc", "tests/lib/flea-transport.sh"):
            shutil.copyfile(REPO / name, self.repo / name)
        for name in ("dirname", "readlink", "python3"):
            (self.bin / name).symlink_to(shutil.which(name))
        self.trace = self.root / "setup.trace"
        self.env = {"HOME": str(self.home), "PATH": str(self.bin),
                    "FLEA_REPO": str(self.repo), "TRACE": str(self.trace),
                    "QT_QPA_PLATFORMTHEME": "generic"}
        self.script(self.repo / "tools/flea-sandbox-guard",
                    f'printf "fixture setup\\n" >> "$TRACE"; exit {GUARD_REACHED}\n')
        self.script(self.repo / "tests/raildrop.sh",
                    f'printf "headless drag\\n"; exit {HEADLESS_DISPATCHED}\n')
        self.script(self.repo / "tests/lib/flea_uish_inventory.py",
                    f'import sys\nprint("headless UI", *sys.argv[1:])\nsys.exit({HEADLESS_DISPATCHED})\n',
                    shebang="#!/usr/bin/env python3\n")

    def remove_root(self):
        assert self.root.parent == Path(tempfile.gettempdir())
        assert self.root.name.startswith("flea-tool-preflight-")
        assert (self.root / ".flea-test-sandbox").is_file()
        shutil.rmtree(self.root)

    def script(self, path, body, shebang="#!/bin/bash\n"):
        path.write_text(shebang + body)
        path.chmod(0o700)

    def stub(self, path):
        self.script(path, f"exit {STUB_EXIT}\n")

    def run_tool(self, name, *args, **environment):
        return subprocess.run(["/bin/bash", str(self.repo / name), *args],
                              cwd=self.repo, env=self.env | environment,
                              text=True, capture_output=True, timeout=5)

    def assert_refusal(self, result, *words):
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("FAIL:", result.stderr)
        for word in words:
            self.assertIn(word, result.stderr)
        self.assertFalse(self.trace.exists(), result.stdout + result.stderr)

    def assert_reached_setup(self, result):
        self.assertEqual(result.returncode, GUARD_REACHED, result.stderr)
        self.assertEqual(self.trace.read_text(), "fixture setup\n")

    def test_native_ui_missing_private_helper_fails_before_setup(self):
        for frontend in ("qml", "gpui"):
            with self.subTest(frontend=frontend):
                result = self.run_tool("tests/ui.sh", "cursor", FLEA_TEST_FRONTEND=frontend)
                self.assert_refusal(result, "omarchy-drive", "private", "FLEA_UI_DRIVER=headless")

    def test_native_drag_missing_private_helper_fails_before_setup(self):
        for case in ("all", "raildrop", "look235"):
            with self.subTest(case=case):
                self.assert_refusal(self.run_tool("tests/drag.sh", case),
                                    "omarchy-drive", "private", "headless")

    def test_native_ui_still_finds_controller_helper_in_local_bin(self):
        self.stub(self.home / ".local/bin/omarchy-drive")
        self.assert_reached_setup(self.run_tool("tests/ui.sh", "cursor"))

    def test_native_ui_still_accepts_controller_helper_on_path(self):
        self.stub(self.bin / "omarchy-drive")
        self.assert_reached_setup(self.run_tool("tests/ui.sh", "cursor"))

    def test_native_drag_still_finds_controller_helper_in_local_bin(self):
        self.stub(self.home / ".local/bin/omarchy-drive")
        self.assert_reached_setup(self.run_tool("tests/drag.sh", "all"))

    def test_native_drag_still_accepts_controller_helper_on_path(self):
        self.stub(self.bin / "omarchy-drive")
        self.assert_reached_setup(self.run_tool("tests/drag.sh", "all"))

    def test_ui_headless_dispatch_needs_no_private_helper(self):
        result = self.run_tool("tests/ui.sh", "cursor", FLEA_UI_DRIVER="headless")
        self.assertEqual(result.returncode, HEADLESS_DISPATCHED, result.stderr)
        self.assertIn("--headless -- cursor", result.stdout)
        self.assertFalse(self.trace.exists())

    def test_drag_headless_dispatch_needs_no_private_helper(self):
        result = self.run_tool("tests/drag.sh", "headless")
        self.assertEqual(result.returncode, HEADLESS_DISPATCHED, result.stderr)
        self.assertIn("headless drag", result.stdout)
        self.assertFalse(self.trace.exists())

    def test_bench_missing_kitty_fails_before_setup(self):
        self.assert_refusal(self.run_tool("tools/flea-field-bench"), "kitty", "install")

    def test_bench_rejects_unsupported_terminal_without_silent_substitution(self):
        self.stub(self.bin / "kitty")
        self.stub(self.bin / "foot")
        self.assert_refusal(self.run_tool("tools/flea-field-bench", TUI_TERM="foot"),
                            "kitty", "TUI_TERM=foot", "--dump-bytes")

    def test_bench_default_and_explicit_kitty_continue_to_existing_setup(self):
        self.stub(self.bin / "kitty")
        for environment in ({}, {"TUI_TERM": "kitty"}, {"TUI_TERM": ""}):
            with self.subTest(environment=environment):
                self.assert_reached_setup(self.run_tool("tools/flea-field-bench", **environment))
                self.trace.unlink()

    def test_bench_default_finds_kitty_in_local_bin(self):
        self.stub(self.home / ".local/bin/kitty")
        self.assert_reached_setup(self.run_tool("tools/flea-field-bench"))

    def test_bench_preflight_uses_controller_path_override(self):
        extra = self.root / "bench-bin"
        extra.mkdir()
        self.stub(extra / "kitty")
        self.assert_reached_setup(self.run_tool("tools/flea-field-bench", B5_BENCH_PATH=str(extra)))

    def test_bench_path_override_cannot_hide_missing_terminal(self):
        self.stub(self.bin / "kitty")
        extra = self.root / "empty-bin"
        extra.mkdir()
        result = self.run_tool("tools/flea-field-bench", B5_BENCH_PATH=str(extra))
        self.assert_refusal(result, "kitty", "install")

    def test_bench_thumbs_dispatch_does_not_require_tui_terminal(self):
        self.script(self.repo / "tools/flea-bench-thumbs",
                    f'import sys\nprint("thumbs", *sys.argv[1:])\nsys.exit({HEADLESS_DISPATCHED})\n',
                    shebang="#!/usr/bin/env python3\n")
        result = self.run_tool("tools/flea-field-bench", "--thumbs", "--help")
        self.assertEqual(result.returncode, HEADLESS_DISPATCHED, result.stderr)
        self.assertIn("thumbs --help", result.stdout)
        self.assertFalse(self.trace.exists())

    def test_touched_shell_syntax(self):
        for name in ("tests/ui.sh", "tests/drag.sh", "tools/flea-field-bench"):
            with self.subTest(name=name):
                result = subprocess.run(["/bin/bash", "-n", str(self.repo / name)],
                                        text=True, capture_output=True, timeout=5)
                self.assertEqual(result.returncode, 0, result.stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
