"""Display-free CLI flow tests; stub observers never claim native coverage."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
TOOL = Path(sys.argv.pop(1)) if len(sys.argv) > 1 else REPO / "tools/flea-acceptance"
STUB = r'''
OMARCHY_CLIPBOARD_LUA=$REPO/clipboard.lua
derive_bindings() { printf 'row|default|gui|listing|none|Down|cursorDown\n'; }
derive_menu_actions() { printf 'open|open|Open|file\n'; }
derive_pointer() { printf 'listing|left|file|open\n'; }
derive_omarchy_clipboard_binds() { printf 'copy|SUPER|C\n'; }
derive_omarchy_names() { printf 'copy\n'; }
drive_validate_groups() { return 0; }
drive_wants() { [[ -z $DRIVE_GROUPS || " $DRIVE_GROUPS " == *" $1 "* ]]; }
drive_guard() { return 0; }
drive_stop() { return "$1"; }
drive_private_state() { :; }
drive_private_trash() { :; }
drive_provenance() { :; }
drive_session_setup() { :; }
drive_all() {
  local group
  for group in ${DRIVE_GROUPS:-key menu omarchy pointer}; do
    printf '%s\n' "$group" >> "$REPO/observers-called"
    skip "$group" test-observer 'stub observer; no native coverage'
  done
}
'''


class References(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="flea-acceptance-refs.", dir="/tmp")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / ".flea-test-sandbox").touch()
        self.repo = self.root / "repo"
        for directory in ("tools", "ui/js", "docs", "tests"):
            (self.repo / directory).mkdir(parents=True, exist_ok=True)
        shutil.copyfile(TOOL, self.repo / "tools/flea-acceptance")
        shutil.copyfile(REPO / "tools/flea-sandbox-guard", self.repo / "tools/flea-sandbox-guard")
        (self.repo / "tools/flea-acceptance-drive").write_text(STUB)
        (self.repo / "tools/flea-keymap-gen").write_text("import pathlib, sys\npathlib.Path(sys.argv[1]).write_text('matrix')\n")
        (self.repo / "ui/js/Keymap.js").write_text("matrix")
        (self.repo / "ui/js/Menu.js").write_text('var columns = [["mode", "Mode"], ["size", "Size"], ["date", "Date"], ["kind", "Kind"]]\n')
        (self.repo / "ui/Sidebar.qml").write_text('group: "network", kind: "share"\n')
        (self.repo / "docs/protocol.md").write_text("## Requests\n## Responses\n")
        (self.repo / "clipboard.lua").touch()
        (self.repo / "tests/example.sh").touch()
        self.manifest = self.repo / ".superpowers/flea/design-completion-20260908/references/SHA256SUMS"

    def run_tool(self, *args):
        return subprocess.run(["bash", str(self.repo / "tools/flea-acceptance"), *args],
                              env=dict(os.environ, FLEA_FIXTURE_ROOT=str(self.root), FLEA_BIN="/usr/bin/true", FLEA_BOARD_DIR=""),
                              capture_output=True, text=True, timeout=10)

    def test_missing_references_do_not_block_independent_groups(self):
        for group in ("key", "menu", "omarchy", "pointer"):
            with self.subTest(group=group):
                marker = self.repo / "observers-called"
                marker.unlink(missing_ok=True)
                result = self.run_tool("--drive", group)
                self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
                self.assertEqual(marker.read_text(), group + "\n")
                self.assertEqual(result.stderr.count("reference manifest unavailable"), 1)
                self.assertIn("0 passed, 0 failed, 1 derived but undriven", result.stdout)
                self.assertNotIn("PASS ", result.stdout)

    def test_unselected_artboards_are_explicitly_undriven(self):
        result = self.run_tool("--drive")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual((self.repo / "observers-called").read_text(), "key\nmenu\nomarchy\npointer\n")
        self.assertIn("UNDRIVEN artboard reference-set", result.stdout)
        self.assertNotIn("PASS ", result.stdout)

    def test_inventory_retains_independent_derivations(self):
        result = self.run_tool("--inventory")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertIn("row|default|gui", result.stdout)
        self.assertIn("open|open|Open|file", result.stdout)
        self.assertFalse((self.repo / "observers-called").exists())

    def test_present_but_invalid_references_still_fail(self):
        self.manifest.parent.mkdir(parents=True)
        self.manifest.write_text("invalid manifest control\n")
        result = self.run_tool("--drive", "menu")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("current board authority check failed", result.stderr)
        self.assertFalse((self.repo / "observers-called").exists())

    def test_dangling_reference_link_still_fails(self):
        self.manifest.parent.mkdir(parents=True)
        self.manifest.symlink_to(self.root / "absent-store/SHA256SUMS")
        result = self.run_tool("--drive", "menu")
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertIn("current board authority check failed", result.stderr)
        self.assertFalse((self.repo / "observers-called").exists())

    def test_valid_references_derive_every_artboard(self):
        self.manifest.parent.mkdir(parents=True)
        rows = [f"{index:064x}  board{index:02d}.html" for index in range(30)]
        self.manifest.write_text("\n".join(rows) + "\n")
        inventory = self.run_tool("--inventory")
        self.assertEqual(inventory.returncode, 0, inventory.stdout + inventory.stderr)
        self.assertIn(f"board29.html|{29:064x}", inventory.stdout)
        result = self.run_tool("--drive")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual(result.stdout.count("UNDRIVEN artboard"), 30)
        self.assertNotIn("reference-set", result.stdout)
        self.assertNotIn("reference manifest unavailable", result.stderr)


if __name__ == "__main__":
    unittest.main()
