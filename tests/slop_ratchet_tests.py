"""tools/flea-slop-ratchet: growth fails and names the offender, equal passes, a raise is refused, tests are excluded."""
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
TOOL = REPO / "tools/flea-slop-ratchet"
FLAGS = "pub struct Flags {\n    a: bool,\n    b: bool,\n    c: bool,\n    d: bool,\n}\n"
BLOCK = "".join(f"let v{i} = call_{i}(x, y);\n" for i in range(9))
UNIQUE = "".join(f"let u{i} = only_{i}();\n" for i in range(40))


class Ratchet(unittest.TestCase):
    def setUp(self):
        parent = REPO / ".superpowers/slop-ratchet"
        parent.mkdir(parents=True, exist_ok=True)
        self.tmp = tempfile.TemporaryDirectory(dir=parent)
        self.root = Path(self.tmp.name)
        subprocess.run(["git", "init", "-q", str(self.root)], check=True)
        self.put("src/backend/a.rs", FLAGS + "fn f() { g().unwrap(); }\n")
        self.put("crates/flea-gui/src/b.rs", "use flea::backend::regfile;\n")
        self.put("src/c.rs", "fn one() {\n" + BLOCK + "}\nfn two() {\n" + BLOCK + "}\n")
        self.assertEqual(self.run_tool("--write-baseline").returncode, 0)
        self.baseline = (self.root / "tools/slop-ratchet-baseline.txt").read_text()

    def tearDown(self):
        self.tmp.cleanup()

    def put(self, rel, text):
        path = self.root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)

    def run_tool(self, *args):
        return subprocess.run([str(TOOL), str(self.root), *args], capture_output=True, text=True)

    def assert_grew(self, metric, offender):
        r = self.run_tool()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn(f"GREW {metric}", r.stdout)
        self.assertIn(offender, r.stdout)

    def assert_passes(self):
        r = self.run_tool()
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assertNotIn("GREW", r.stdout)

    def assert_write_refused(self, metrics):
        r = self.run_tool("--write-baseline")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn(f"REFUSED: --write-baseline would raise {metrics}\n", r.stdout)
        self.assertEqual((self.root / "tools/slop-ratchet-baseline.txt").read_text(), self.baseline)

    def test_equal_passes(self):
        self.assert_passes()

    def test_bool_struct_grows(self):
        self.put("src/d.rs", FLAGS.replace("Flags", "More"))
        self.assert_grew("bool_structs", "src/d.rs More 1, baseline 0")

    def test_bool_struct_behind_a_brace_in_a_doc_comment(self):
        self.put("src/d.rs", FLAGS.replace("{\n", "{\n    /// Shown as `{a}`.\n", 1).replace("Flags", "Doc"))
        self.assert_grew("bool_structs", "src/d.rs Doc 1, baseline 0")

    def test_backend_import_grows(self):
        self.put("crates/flea-gui/src/e.rs", "use flea::backend::ops;\n")
        self.assert_grew("backend_imports", "crates/flea-gui/src/e.rs flea::backend::ops 1")
        self.put("crates/flea-gui/src/e.rs", "")
        self.put("src/backend/f.rs", "use crate::gui::Thing;\n")
        self.assert_grew("backend_imports", "src/backend/f.rs crate::gui 1")

    def test_grouped_and_aliased_backend_imports_grow(self):
        for text, edge in (
            ("use flea::backend::{ops, regfile};\n", "flea::backend::ops"),
            ("use flea::{backend::{self, trash}, keys};\n", "flea::backend::trash"),
            ("use flea::backend;\nfn f() { backend::events::send(); }\n", "flea::backend::events"),
            ("use flea as core;\nfn f() { core::backend::listing::open(); }\n", "flea::backend::listing"),
            ("fn f() { ::flea::backend::run::go(); }\n", "flea::backend::run"),
        ):
            self.put("crates/flea-gui/src/e.rs", text)
            self.assert_grew("backend_imports", f"crates/flea-gui/src/e.rs {edge} 1")
        self.put("crates/flea-gui/src/e.rs", "")
        for text, edge in (
            ("use crate::{gui::Thing, keys};\n", "crate::gui"),
            ("fn f() { super::super::tui::draw(); }\n", "crate::tui"),
            ("use super::super::*;\n", "crate::*"),
        ):
            self.put("src/backend/f.rs", text)
            self.assert_grew("backend_imports", f"src/backend/f.rs {edge} 1")

    def test_backend_paths_that_cross_nothing_pass(self):
        self.put("crates/flea-gui/src/e.rs", "use crate::backend::Client;\nfn f() { crate::backend::send(); }\n// flea::backend::ops\n")
        self.put("src/backend/f.rs", "use super::*;\nuse crate::backend::listing;\nfn f() { super::ops::run(); }\n")
        self.put("src/tui/g.rs", "use crate::backend::listing;\n")
        self.assert_passes()

    def test_unwrap_grows(self):
        self.put("src/g.rs", "fn h() { x.expect(\"set\"); }\n")
        self.assert_grew("unwraps", "src/g.rs 1, baseline 0")

    def test_unwraps_in_comments_and_strings_do_not_count(self):
        self.put("src/g.rs", "/// let v = parse(s).unwrap();\nfn h() {\n    /* y.unwrap() */\n    let s = \".unwrap() and .expect(\";\n    let c = '}';\n}\n")
        self.assert_passes()

    def test_code_after_a_test_item_still_counts(self):
        for text in (
            "#[cfg(test)]\nmod a_tests;\nfn f() { g().unwrap(); }\n",
            "#[cfg(test)]\n#[path = \"x_tests.rs\"]\nmod tests;\nfn f() { g().unwrap(); }\n",
            "#[cfg(test)]\nfn helper() { y().unwrap(); }\nfn f() { g().unwrap(); }\n",
            "fn r() -> usize {\n    #[cfg(test)]\n    if flag() { return 0; }\n    \"tail\".len() + g().unwrap()\n}\n",
            "// #[cfg(test)]\nfn f() { g().unwrap(); }\n",
            "fn f() { let s = \"#[cfg(test)]\"; g().unwrap(); }\n",
            "fn f(x: u8) -> u8 {\n    match x {\n        #[cfg(test)]\n        n if n < 3 => 0,\n        _ => g().unwrap(),\n    }\n}\n",
            "fn f() { g().unwrap(); }\nmod t {\n    #![cfg(test)]\n    fn x() { y().unwrap(); }\n}\n",
            "#![cfg(not(test))]\nfn f() { g().unwrap(); }\n",
            "#[cfg(any(test, unix))]\nfn f() { g().unwrap(); }\n",
        ):
            self.put("src/g.rs", text)
            self.assert_grew("unwraps", "src/g.rs 1, baseline 0")

    def test_test_items_do_not_count(self):
        self.put("src/g.rs", "fn f() {}\n#[cfg(test)]\nfn helper() { y().unwrap(); }\n#[cfg(all(test, unix))]\nimpl<A, B> T<A, B> { fn z() { q().unwrap(); } }\nstruct S {\n    a: bool,\n    b: bool,\n    c: bool,\n    #[cfg(test)]\n    d: bool,\n}\n#[cfg(test)]\nfn helper<F>(f: F) -> u8\nwhere\n    F: Fn() -> Option<u8>,\n{\n    f().unwrap()\n}\nfn k() { call(1, #[cfg(test)] y().unwrap()); }\n")
        self.assert_passes()

    def test_fields_after_a_test_field_still_count(self):
        self.put("src/g.rs", "struct S {\n    #[cfg(test)]\n    hook: HashMap<u8, bool>,\n    a: bool,\n    b: bool,\n    c: bool,\n    d: bool,\n}\n")
        self.assert_grew("bool_structs", "src/g.rs S 1, baseline 0")

    def test_test_names_are_whole_words(self):
        self.put("src/latest_release.rs", "fn f() { g().unwrap(); }\n")
        self.assert_grew("unwraps", "src/latest_release.rs 1, baseline 0")

    def test_declared_test_modules_are_excluded_and_declared_prod_modules_count(self):
        self.put("src/lib.rs", "#[cfg(test)]\nmod gitfake;\n#[cfg(test)]\n#[path = \"probes/hold.rs\"]\nmod hold;\nmod real_tests;\n")
        self.put("src/gitfake.rs", "mod inner;\nfn f() { g().unwrap(); }\n")
        self.put("src/gitfake/inner.rs", "fn f() { g().unwrap(); }\n")
        self.put("src/probes/hold.rs", FLAGS)
        self.assert_passes()
        self.put("src/real_tests.rs", "fn f() { g().unwrap(); }\n")
        self.assert_grew("unwraps", "src/real_tests.rs 1, baseline 0")

    def test_clone_share_grows(self):
        self.put("src/h.rs", "fn three() {\n" + BLOCK + "}\n")
        self.assert_grew("clone_lines", "src/c.rs src/h.rs")
        self.assert_write_refused("clone_lines")

    def test_clone_hidden_in_a_large_addition_grows(self):
        self.put("src/h.rs", "".join(f"let w{i} = other_{i}();\n" for i in range(2000)) + "fn three() {\n" + BLOCK + "}\n")
        self.assert_grew("clone_lines", "src/c.rs src/h.rs")
        self.assert_write_refused("clone_lines")

    def test_deleting_unique_code_can_be_written(self):
        self.put("src/u.rs", UNIQUE)
        self.assertEqual(self.run_tool("--write-baseline").returncode, 0)
        self.put("src/u.rs", "")
        r = self.run_tool()
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("no new duplicated lines", r.stdout)
        self.put("src/g.rs", "fn h() { x.unwrap(); }\n")
        r = self.run_tool("--write-baseline")
        self.assertEqual(r.returncode, 1, r.stdout)
        self.assertIn("REFUSED: --write-baseline would raise unwraps\n", r.stdout)
        self.put("src/g.rs", "")
        r = self.run_tool("--write-baseline")
        self.assertEqual(r.returncode, 0, r.stdout)
        self.assert_passes()

    def test_raise_refused_and_lower_written(self):
        self.put("src/g.rs", "fn h() { x.unwrap(); }\n")
        r = self.run_tool("--write-baseline")
        self.assertEqual(r.returncode, 1)
        self.assertIn("REFUSED", r.stdout)
        self.assertEqual((self.root / "tools/slop-ratchet-baseline.txt").read_text(), self.baseline)
        self.put("src/g.rs", "")
        self.put("src/backend/a.rs", FLAGS + "fn f() { g().ok(); }\n")
        self.assertEqual(self.run_tool("--write-baseline").returncode, 0)
        self.assertIn("total unwraps 0", (self.root / "tools/slop-ratchet-baseline.txt").read_text())

    def test_test_code_excluded(self):
        self.put("src/backend/a.rs", FLAGS + "fn f() { g().unwrap(); }\n#[cfg(test)]\nmod t {\n    fn x() { y().unwrap(); }\n" + FLAGS.replace("Flags", "T") + "}\n")
        self.put("src/backend/a_tests.rs", FLAGS + "fn z() { q().unwrap(); }\n")
        self.put("crates/flea-gui/tests/i.rs", "use flea::backend::ops;\n" + FLAGS)
        self.put("src/m_fixture.rs", "fn three() {\n" + BLOCK + "}\n")
        self.put("src/n.rs", "//! Only built for tests.\n#![cfg(test)]\nfn z() { q().unwrap(); }\n")
        self.put("src/o.rs", "#![cfg(all(test, unix))]\nfn z() { q().unwrap(); }\n")
        self.assert_passes()

    def test_tree_without_git_skips_target_and_dot_dirs(self):
        shutil.rmtree(self.root / ".git")
        self.put("target/debug/build/x.rs", FLAGS.replace("Flags", "Built"))
        self.put(".cargo/y.rs", FLAGS.replace("Flags", "Hidden"))
        self.assert_passes()
        self.put("src/d.rs", FLAGS.replace("Flags", "More"))
        self.assert_grew("bool_structs", "src/d.rs More 1, baseline 0")


if __name__ == "__main__":
    unittest.main()
