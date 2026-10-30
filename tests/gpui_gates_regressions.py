"""Gate-shell regressions: the strict flea-gui mutant is judged only through tools/flea-clippy."""
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

REPO = Path(__file__).resolve().parent.parent
GATE = REPO / "tests/gpui-gates.sh"

CARGO = """#!/bin/bash
# Stand-in cargo: fmt passes, and clippy reports the injected name the way the strict build does.
[ "$1" = fmt ] && exit 0
if [ "$1" = test ]; then
  case " $* " in
    *" font_gate "*)
      [ "$MODE" != font-failure ] || exit 101
      [ "$MODE" != font-empty ] || { echo 'running 0 tests'; exit 0; }
      echo 'FONT_GATE failed_surfaces=[]' ;;
    *)
      case "$MODE" in
        frame) echo 'test result: FAILED. SHORT_TESTS passed; 2 failed; 0 ignored; 0 measured; 3083 filtered out'; exit 101 ;;
        short) echo 'test result: ok. SHORT_TESTS passed; 0 failed; 0 ignored; 0 measured; 3085 filtered out' ;;
        surplus) echo 'test result: ok. SURPLUS_TESTS passed; 0 failed; 0 ignored; 0 measured; 3082 filtered out' ;;
        empty) ;;
        *) echo 'test result: ok. FRAME_TESTS passed; 0 failed; 0 ignored; 0 measured; 3083 filtered out' ;;
      esac ;;
  esac
  exit 0
fi
case "$MODE" in
  unrelated) printf 'warning: unused variable: `g10l_injected_warning`\\nerror: unrelated core compile failure\\n' ;;
  *) printf 'error: unused variable: `g10l_injected_warning`\\n' ;;
esac
exit 101
"""
# The stand-in reports the pinned count, two fewer for a short or failing run, one more for a surplus, and none when empty.
# frame_tests=34
PIN = re.search(r"^frame_tests=(\d+)$", GATE.read_text(), re.M)
if PIN is None:
    raise SystemExit(f"{GATE}: no frame_tests=N line pins the frame-gate test count")
FRAME_TESTS = int(PIN.group(1))
CARGO = CARGO.replace("SHORT_TESTS", str(FRAME_TESTS - 2)).replace("SURPLUS_TESTS", str(FRAME_TESTS + 1))
CARGO = CARGO.replace("FRAME_TESTS", str(FRAME_TESTS))


def count_failure(got):
    return f"FAIL frame-gate exited 0 but expected {FRAME_TESTS} passed frame tests, got: {got}"


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
        env = dict(os.environ, PATH=f"{self.root / 'bin'}{os.pathsep}{os.environ['PATH']}", MODE=mode,
                   HOME=str(self.root), XDG_DATA_HOME=str(self.root / "data"),
                   XDG_CONFIG_HOME=str(self.root / "config"), XDG_CACHE_HOME=str(self.root / "cache"))
        return subprocess.run(["/bin/bash", str(self.root / "tests/gpui-gates.sh")], cwd=self.root,
                              env=env, capture_output=True, text=True)

    def test_frame_failure_fails_the_existing_rust_gate(self):
        p = self.run_gate("frame")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn("GATE frame-gate rc=101", p.stdout)
        self.assertNotIn("FAIL frame-gate exited 0", p.stdout)

    def test_short_frame_run_fails_the_gate(self):
        p = self.run_gate("short")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn("GATE frame-gate rc=0", p.stdout)
        self.assertIn(count_failure(f"test result: ok. {FRAME_TESTS - 2} passed;"), p.stdout)

    def test_surplus_frame_run_names_the_reported_count(self):
        p = self.run_gate("surplus")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn(count_failure(f"test result: ok. {FRAME_TESTS + 1} passed;"), p.stdout)

    def test_frame_run_without_a_result_line_fails_and_continues(self):
        p = self.run_gate("empty")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn(count_failure("no test result line"), p.stdout)
        self.assertIn("GATE gate-regressions rc=0", p.stdout)
        later = p.stdout.index("GATE gate-regressions rc=0")
        self.assertLess(p.stdout.index(count_failure("no test result line")), later, p.stdout)

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

    def test_system_font_failure_fails_the_existing_gui_lane(self):
        p = self.run_gate("font-failure")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn("GATE system-font rc=101", p.stdout)

    def test_system_font_filter_matching_no_test_fails(self):
        p = self.run_gate("font-empty")
        self.assertNotEqual(0, p.returncode, p.stdout)
        self.assertIn("GATE system-font has no FONT_GATE failed_surfaces=[] line", p.stdout)


if __name__ == "__main__":
    unittest.main()
