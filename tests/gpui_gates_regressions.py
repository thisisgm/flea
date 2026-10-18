"""Gate-shell regressions: the strict flea-gui mutant is judged only through tools/flea-clippy."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parent.parent
GATE = REPO / "tests/gpui-gates.sh"

CARGO = """#!/bin/bash
# Stand-in cargo: fmt passes, and clippy reports the injected name the way the strict build does.
[ "$1" = fmt ] && exit 0
case "$MODE" in
  unrelated) printf 'warning: unused variable: `g10l_injected_warning`\\nerror: unrelated core compile failure\\n' ;;
  *) printf 'error: unused variable: `g10l_injected_warning`\\n' ;;
esac
exit 101
"""

WRAPPER = """#!/bin/bash
# Stand-in tools/flea-clippy: the control tree is clean, and the mutant tree behaves as MODE says.
if [ "$(basename "$PWD")" != mutant ]; then
  echo "clippy flea-gui: PASS cargo_rc=0; see clippy-gui.log"
  exit 0
fi
mkdir -p "$2"
case "$MODE" in
  silent) echo "clippy: PASS denied=0 rose=0 fell=0 findings=0 lints=0 cargo_rc=0"; exit 0 ;;
  unrelated) printf 'warning: unused variable: `g10l_injected_warning`\\nerror: unrelated core compile failure\\n' > "$2/clippy-gui.log" ;;
  *) printf 'error: unused variable: `g10l_injected_warning`\\n' > "$2/clippy-gui.log" ;;
esac
echo "clippy flea-gui: FAIL cargo_rc=101; see clippy-gui.log"
exit 1
"""


class GateShell(unittest.TestCase):
    def setUp(self):
        parent = REPO / ".superpowers/g10l"
        parent.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="gates-regress.", dir=parent))
        (self.root / ".flea-test-sandbox").touch()

    def tearDown(self):
        # Hard rule 9: only this test's own marked root is ever deleted.
        assert self.root.is_absolute() and self.root.name.startswith("gates-regress.")
        if (self.root / ".flea-test-sandbox").is_file():
            shutil.rmtree(self.root)

    def write(self, path, text, mode=0o644):
        dest = self.root / path
        assert dest.is_relative_to(self.root)
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_text(text)
        dest.chmod(mode)

    def run_gate(self, mode):
        self.write("tests/gpui-gates.sh", GATE.read_text(), 0o755)
        self.write("tools/flea-clippy", WRAPPER, 0o755)
        self.write("tools/flea-gpui-lint", "#!/bin/bash\nexit 0\n", 0o755)
        self.write("tools/clippy-baseline.txt", "")
        self.write("bin/cargo", CARGO, 0o755)
        self.write("Cargo.toml", "[workspace]\n")
        self.write("Cargo.lock", "version = 4\n")
        self.write("src/lib.rs", "pub fn gate_fixture() {}\n")
        self.write("crates/flea-gui/src/main.rs", "fn main() {}\n")
        self.write("ui/Main.qml", "import QtQuick\n")
        self.write("shelf/Shelf.qml", "import QtQuick\n")
        self.write("tests/gpui_gates_regressions.py", "raise SystemExit(0)\n")
        env = dict(os.environ, PATH=f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}", MODE=mode)
        return subprocess.run(["/bin/bash", str(self.root / "tests/gpui-gates.sh")], cwd=self.root,
                              env=env, capture_output=True, text=True)

    def test_silent_wrapper_cannot_accept_the_strict_gui_mutant(self):
        p = self.run_gate("silent")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn("not rejected by tools/flea-clippy", p.stdout)

    def test_unrelated_error_beside_a_warning_fails(self):
        p = self.run_gate("unrelated")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn("not rejected by tools/flea-clippy", p.stdout)

    def test_genuine_injected_denial_passes(self):
        p = self.run_gate("injected")
        self.assertEqual(0, p.returncode, p.stdout + p.stderr)
        self.assertIn("ok strict flea-gui warning mutant rejected", p.stdout)


if __name__ == "__main__":
    unittest.main()
