"""Roster and installed-reference release gates, using private marked fixtures."""
import copy
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOL = ROOT / "tools/flea-i18n-roster"
PRIVATE = ROOT / ".superpowers/m9-i50-03"


class RosterTests(unittest.TestCase):
    def setUp(self):
        PRIVATE.mkdir(parents=True, exist_ok=True)
        self.root = Path(tempfile.mkdtemp(prefix="roster.", dir=PRIVATE))
        (self.root / ".flea-test-sandbox").touch()
        self.addCleanup(self.cleanup_root)
        self.omarchy = self.root / "omarchy"
        self.catalogs = self.root / "i18n"
        self.source = self.root / "src"
        self.source.mkdir()
        self.pin = json.loads((ROOT / "ci/i18n-pin.json").read_text())
        self.registry = json.loads((ROOT / "src/i18n/fixtures/locales.json").read_text())
        for name in ("locales.json", "plural.jq"):
            target = self.omarchy / "default/i18n" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((ROOT / "src/i18n/fixtures" / name).read_bytes())
        for code in self.registry:
            self.write(self.catalogs / code / "flea.json", {"cancel": "omarchy:shell.common.cancel"})
            for kind in ("shell", "cli", "menu", "keybindings"):
                value = {"common.cancel": "Cancel", "count": {"other": "%1 items"}}
                if kind == "menu":
                    value = {"apps.fileManager": {"label": "File manager"}}
                if kind == "keybindings":
                    value = {"Close window": "Close window"}
                self.write(self.omarchy / "default/i18n" / code / f"{kind}.json", value)
        self.write(self.root / "pin.json", self.pin)

    def cleanup_root(self):
        root = self.root
        assert root.is_absolute() and str(root) and root.parent == PRIVATE
        assert not root.is_symlink() and (root / ".flea-test-sandbox").is_file()
        shutil.rmtree(root)

    def write(self, path, value):
        assert path.is_absolute() and path.is_relative_to(self.root) and ".." not in path.parts
        assert (self.root / ".flea-test-sandbox").is_file()
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False), encoding="utf-8")

    def run_tool(self, *args, status=0):
        command = [sys.executable, "-B", str(TOOL), "--omarchy-path", str(self.omarchy),
                   "--i18n-dir", str(self.catalogs), "--source-dir", str(self.source),
                   "--pin-file", str(self.root / "pin.json"), *args]
        result = subprocess.run(command, capture_output=True, text=True)
        self.assertEqual(result.returncode, status, result.stdout + result.stderr)
        return result

    def test_i50_roster_complete_installed(self):
        result = self.run_tool("--check-installed")
        self.assertIn(f"{len(self.registry)} catalogs; 1 installed references", result.stdout)

    def test_i50_roster_new_code_fails(self):
        self.registry["zz"] = copy.deepcopy(self.registry["en"])
        self.write(self.omarchy / "default/i18n/locales.json", self.registry)
        self.assertIn("new=['zz']", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_removed_code_fails(self):
        self.registry.pop("de")
        self.write(self.omarchy / "default/i18n/locales.json", self.registry)
        self.assertIn("missing=['de']", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_bad_direction_fails(self):
        for direction, diagnostic in [("sideways", "invalid direction: ar"), ("ltr", "locale metadata differs")]:
            with self.subTest(direction=direction):
                self.registry["ar"]["direction"] = direction
                self.write(self.omarchy / "default/i18n/locales.json", self.registry)
                self.assertIn(diagnostic, self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_missing_installed_ref_fails(self):
        for code in ("en", "de"):
            with self.subTest(code=code):
                path = self.omarchy / "default/i18n" / code / "shell.json"
                self.write(path, {"unrelated": "Other"})
                result = self.run_tool("--check-installed", status=1)
                self.assertIn(f"{code}: missing installed Omarchy reference: omarchy:shell.common.cancel", result.stderr)
                self.write(path, {"common.cancel": "Cancel"})

    def test_i50_roster_direct_and_catalog_plural_references(self):
        (self.source / "main.rs").write_text('fn main() { tr("omarchy:menu.apps.fileManager.label");\n'
                                            'tr(r#"omarchy:keybindings.Close window"#); }\n')
        self.write(self.catalogs / "fr/flea.json", {"count": {"other": "omarchy:shell.count"}})
        self.assertIn("4 installed references", self.run_tool("--check-installed").stdout)
        self.write(self.omarchy / "default/i18n/de/menu.json", {"apps.fileManager": {"hint": "Open"}})
        self.assertIn("omarchy:menu.apps.fileManager.label", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_missing_direct_reference_fails(self):
        (self.source / "main.rs").write_text('fn main() { tr("omarchy:shell.disappeared"); }\n')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_quote_char_cannot_hide_reference(self):
        (self.source / "main.rs").write_text("fn main() { let quote = '\"'; tr(\"omarchy:shell.disappeared\"); }\n")
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_rust_escaped_references(self):
        (self.source / "main.rs").write_text(r'fn main() { tr("omarchy:keybindings.Close\u{20}window"); }')
        self.assertIn("2 installed references", self.run_tool("--check-installed").stdout)
        (self.source / "main.rs").write_text(r'fn main() { tr("\x6fmarchy:shell.disappeared"); }')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_inline_tests_and_comments_excluded(self):
        (self.source / "main.rs").write_text('''
#[cfg(test)] mod tests { const MISSING: &str = "omarchy:shell.test"; }
#[cfg(test)] #[test] fn fixture() { tr("omarchy:shell.fixture"); }
fn main<'a>() { let brace = '}'; tr(r##"omarchy:keybindings.Close window"##); }
''')
        self.assertIn("2 installed references", self.run_tool("--check-installed").stdout)
        with (self.source / "main.rs").open("a") as stream:
            stream.write('fn production() { tr("omarchy:shell.disappeared"); }\n')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_nested_comments_excluded(self):
        (self.source / "main.rs").write_text('/* outer /* nested */ "omarchy:shell.comment" */\n')
        self.assertIn("1 installed references", self.run_tool("--check-installed").stdout)

    def test_i50_roster_test_constants_excluded(self):
        items = ['const FIXTURES: [&str; 1] = ["omarchy:shell.fake"];',
                 'const CHOICE: &str = if true { "omarchy:shell.left" } else { "omarchy:shell.right" };',
                 'pub(crate) const PAIR: (&str, &str) = ({ "omarchy:shell.left" }, "omarchy:shell.right");']
        for item in items:
            with self.subTest(item=item):
                path = self.source / "main.rs"
                path.write_text("#[cfg(test)] " + item + "\n")
                self.assertIn("1 installed references", self.run_tool("--check-installed").stdout)
                with path.open("a") as stream:
                    stream.write('fn live() { tr("omarchy:shell.disappeared"); }\n')
                self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_raw_byte_strings(self):
        path = self.source / "main.rs"
        path.write_text(r'fn main() { let bytes = br"\d+"; let raw = br#"\u{ignored}"#; }')
        self.assertIn("1 installed references", self.run_tool("--check-installed").stdout)
        with path.open("a") as stream:
            stream.write('\nfn live() { tr("omarchy:shell.disappeared"); }\n')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_inline_const_cannot_hide_reference(self):
        (self.source / "main.rs").write_text('fn live() { #[cfg(test)] const { assert!(true); } '
                                            'tr("omarchy:shell.disappeared"); }\n')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_macro_identifier_cannot_hide_reference(self):
        (self.source / "main.rs").write_text('#[cfg(test)] macro_rules! r#type { () => { "omarchy:shell.fixture" } }\n'
                                            'fn live() { tr("omarchy:shell.disappeared"); }\n')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_empty_installed_ref_fails(self):
        for value in ("", {"other": ""}, {"one": "One"}, {"other": "Count", "invalid": "Bad"}):
            with self.subTest(value=value):
                self.write(self.omarchy / "default/i18n/de/shell.json", {"common.cancel": value})
                self.assertIn("reference: omarchy:shell.common.cancel", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_missing_installed_data_fails(self):
        self.assertIn("No such file", self.run_tool("--omarchy-path", str(self.root / "absent"),
                                                    "--check-installed", status=1).stderr)
        path = self.omarchy / "default/i18n/plural.jq"
        assert path.is_absolute() and str(path) and path.is_relative_to(self.root)
        assert (self.root / ".flea-test-sandbox").is_file()
        path.unlink()
        self.assertIn("plural.jq", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_rc_not_final(self):
        self.pin["upstream"]["tag"] = "v4.5.0-rc2"
        self.write(self.root / "pin.json", self.pin)
        self.assertIn("final Omarchy v4.5.0 pin required", self.run_tool("--require-final-pin", status=1).stderr)
        # This synthetic identity exercises the final-tag schema, not upstream tag verification.
        self.pin["upstream"]["tag"] = "v4.5.0"
        self.pin["upstream"]["commit"] = "1" * 40
        self.write(self.root / "pin.json", self.pin)
        self.run_tool("--check-installed", "--require-final-pin")

    def test_i50_roster_fixture_provenance(self):
        for name, item in self.pin["fixtures"].items():
            with self.subTest(name=name):
                self.assertEqual(item["source"], f"default/i18n/{name}")
                self.assertEqual(item["fixture"], f"src/i18n/fixtures/{name}")
                self.assertEqual(item["sha256"], hashlib.sha256((ROOT / item["fixture"]).read_bytes()).hexdigest())
                changed = copy.deepcopy(self.pin)
                changed["fixtures"][name]["sha256"] = "0" * 64
                self.write(self.root / "pin.json", changed)
                self.assertIn(f"fixture SHA-256 mismatch: {name}", self.run_tool("--check-installed", status=1).stderr)

    def repin_fixture(self):
        repo = self.omarchy
        git = ["git", "-C", str(repo), "-c", "user.name=GM", "-c", "user.email=gianmarcomorales@icloud.com"]
        for command in (["init", "-q"], ["remote", "add", "origin", "https://example.invalid/omarchy.git"],
                        ["add", "default/i18n/locales.json", "default/i18n/plural.jq"],
                        ["commit", "-q", "-m", "fixture"], ["tag", "v4.5.0"]):
            subprocess.run([*git, *command], check=True, capture_output=True)
        self.assertEqual(subprocess.check_output([*git, "log", "-1", "--format=%an <%ae>"], text=True).strip(),
                         "GM <gianmarcomorales@icloud.com>")
        project = self.root / "project"
        tool = project / "tools/flea-i18n-roster"
        tool.parent.mkdir(parents=True)
        shutil.copyfile(TOOL, tool)
        shutil.copytree(ROOT / "src/i18n/fixtures", project / "src/i18n/fixtures")
        return repo, git, project, tool

    def test_i50_roster_repin_reads_tag_objects(self):
        repo, git, project, tool = self.repin_fixture()
        commit = subprocess.run([*git, "rev-parse", "HEAD"], check=True, capture_output=True, text=True).stdout.strip()
        (repo / "default/i18n/plural.jq").write_text("# working tree edit is ignored\n")
        pin = self.root / "repin.json"
        before = {name: (project / "src/i18n/fixtures" / name).read_bytes() for name in ("locales.json", "plural.jq")}
        (project / "src/i18n/fixtures/locales.json").write_text('{"stale":{}}\n')
        (project / "src/i18n/fixtures/plural.jq").write_text("# stale destination\n")
        result = subprocess.run([sys.executable, "-B", str(tool), "--repin", "v4.5.0", "--omarchy-path", str(repo),
                                 "--pin-file", str(pin)], capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn(f"v4.5.0 {commit}; {len(self.registry)} codes", result.stdout)
        written = json.loads(pin.read_text())
        self.assertEqual(written["upstream"], {"repository": "https://example.invalid/omarchy.git",
                                               "tag": "v4.5.0", "commit": commit})
        for name, data in before.items():
            self.assertEqual((project / "src/i18n/fixtures" / name).read_bytes(), data)
            self.assertEqual(written["fixtures"][name]["sha256"], hashlib.sha256(data).hexdigest())
        failed = subprocess.run([sys.executable, "-B", str(tool), "--repin", "v4.5.0-rc9", "--omarchy-path", str(repo),
                                 "--pin-file", str(pin)], capture_output=True, text=True)
        self.assertEqual(failed.returncode, 1)
        self.assertIn("repin failed", failed.stderr)

    def test_i50_roster_rejected_repin_preserves_files(self):
        repo, git, project, tool = self.repin_fixture()
        pin = project / "pin.json"
        self.write(pin, self.pin)
        paths = [project / "src/i18n/fixtures" / name for name in ("locales.json", "plural.jq")] + [pin]
        before = {path: path.read_bytes() for path in paths}
        for value in ([], {"en": {"direction": "sideways"}}):
            with self.subTest(value=value):
                self.write(repo / "default/i18n/locales.json", value)
                subprocess.run([*git, "add", "default/i18n/locales.json"], check=True, capture_output=True)
                subprocess.run([*git, "commit", "-q", "-m", "invalid fixture"], check=True, capture_output=True)
                subprocess.run([*git, "tag", "-f", "v4.5.0"], check=True, capture_output=True)
                result = subprocess.run([sys.executable, "-B", str(tool), "--repin", "v4.5.0", "--omarchy-path", str(repo),
                                         "--pin-file", str(pin)], capture_output=True, text=True)
                self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
                self.assertIn("repin failed", result.stderr)
                self.assertEqual({path: path.read_bytes() for path in paths}, before)

    def test_i50_roster_changed_installed_rules_fail(self):
        (self.omarchy / "default/i18n/plural.jq").write_text("# changed\n")
        self.assertIn("installed plural.jq differs", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_warn_only_is_development_only(self):
        path = self.catalogs / "de/flea.json"
        assert path.is_absolute() and str(path) and path.is_relative_to(self.root)
        assert (self.root / ".flea-test-sandbox").is_file()
        path.unlink()
        self.assertIn("1 missing catalogs: de", self.run_tool(status=1).stderr)
        self.assertIn("warning: 1 missing catalogs: de", self.run_tool("--warn-only").stderr)
        path.mkdir()
        self.assertIn("1 missing catalogs: de", self.run_tool(status=1).stderr)
        self.assertIn("1 missing catalogs: de", self.run_tool("--check-installed", status=1).stderr)
        self.assertIn("development-only", self.run_tool("--warn-only", "--require-final-pin", status=2).stderr)

    def test_i50_roster_warn_only_installed_missing_catalog_warns(self):
        self.remove_de_catalog()
        result = self.run_tool("--check-installed", "--warn-only")
        self.assertIn("warning:", result.stderr)
        self.assertIn("missing catalogs", result.stderr)

    def remove_de_catalog(self):
        path = self.catalogs / "de/flea.json"
        assert path.is_absolute() and str(path) and path.is_relative_to(self.root)
        assert (self.root / ".flea-test-sandbox").is_file()
        path.unlink()

    def test_i50_roster_warn_only_installed_hash_mismatch_fails(self):
        self.remove_de_catalog()
        (self.omarchy / "default/i18n/plural.jq").write_text("# changed\n")
        self.assertIn("installed plural.jq differs", self.run_tool("--check-installed", "--warn-only", status=1).stderr)

    def test_i50_roster_warn_only_installed_reference_fails(self):
        self.remove_de_catalog()
        (self.source / "main.rs").write_text('fn main() { tr("omarchy:shell.disappeared"); }\n')
        self.assertIn("omarchy:shell.disappeared", self.run_tool("--check-installed", "--warn-only", status=1).stderr)

    def test_i50_roster_warn_only_require_final_pin_rejected(self):
        result = self.run_tool("--warn-only", "--require-final-pin", status=2)
        self.assertIn("--warn-only is development-only and cannot bypass the final-pin check", result.stderr)

    def test_i50_roster_unknown_catalog_fails(self):
        self.write(self.catalogs / "zz/flea.json", {"label": "Unknown"})
        self.assertIn("unknown Flea catalog codes: zz", self.run_tool("--check-installed", status=1).stderr)

    def test_i50_roster_duplicate_json_fails(self):
        (self.omarchy / "default/i18n/de/shell.json").write_text('{"common.cancel":"Cancel","common.cancel":"Other"}')
        self.assertIn("duplicate JSON key: common.cancel", self.run_tool("--check-installed", status=1).stderr)


if __name__ == "__main__":
    unittest.main(verbosity=2)
