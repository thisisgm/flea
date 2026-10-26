"""Source freeze and producer handoff contracts, discovered by i18n-quality."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parents[2]
TOOL = REPO / "tools/flea-i18n-extract"


class ExtractTests(unittest.TestCase):
    def setUp(self):
        parent = Path(os.environ.get("FLEA_I18N_TEST_ROOT", REPO / ".superpowers/m9-i50-01"))
        parent.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="extract-", dir=parent)
        self.root = Path(self.temporary.name).resolve()
        (self.root / ".flea-test-sandbox").touch()
        self.addCleanup(self.temporary.cleanup)
        # The extractor records HEAD, so the fixture is its own repository, free of the caller's git environment and config.
        self.git_env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        self.git_env.update(GIT_CONFIG_GLOBAL="/dev/null", GIT_CONFIG_NOSYSTEM="1")
        subprocess.run(["git", "init", "-q", str(self.root)], check=True, env=self.git_env)
        subprocess.run(["git", "-C", str(self.root), "-c", "user.name=flea-test", "-c", "user.email=flea-test@invalid",
                        "commit", "-q", "--allow-empty", "-m", "fixture"], check=True, env=self.git_env)
        self.omarchy = self.root / "omarchy"
        self.index = self.root / "i18n/source-index.json"
        self.catalog = {"file.open": "Open", "files.count": {"one": "%1 file", "other": "%1 files"},
                        "unused.label": "Unused", "keys.action.mute": "Mute"}
        self.write("i18n/en/flea.json", self.catalog)
        self.write("src/example.rs", 'fn draw(i: &I18n) { i.tr("file.open"); i.trn("files.count", 2);\n'
                   'i.tr("omarchy:shell.common.cancel"); }\n')
        self.write("omarchy/default/i18n/locales.json", {"en": {"name": "English"}})
        self.write("omarchy/default/i18n/plural.jq", "def plural: \"other\";\n")
        for kind in ("shell", "menu", "cli", "keybindings"):
            self.write(f"omarchy/default/i18n/en/{kind}.json",
                       {"common.cancel": "Cancel"} if kind == "shell" else
                       {"Mute": "Mute"} if kind == "keybindings" else {})

    def write(self, name, value):
        path = self.root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value, ensure_ascii=False) if isinstance(value, dict) else value)

    def run_tool(self, mode, *extra, ok=True, error=None):
        args = [sys.executable, "-B", str(TOOL), mode, "--repo", str(self.root),
                "--source-index", str(self.index), "--omarchy-path", str(self.omarchy), *extra]
        result = subprocess.run(args, text=True, capture_output=True, env=self.git_env)
        self.assertEqual(result.returncode, 0 if ok else 1, result.stdout + result.stderr)
        self.assertNotIn("Traceback", result.stderr)
        if error is not None:
            self.assertIn(error, result.stderr)
        return result

    def freeze(self):
        self.run_tool("freeze", "--source-ref", "HEAD")
        return json.loads(self.index.read_text())

    def delta(self, entries, name="delta.json", **overrides):
        index = json.loads(self.index.read_text())
        self.write(name, {"schema": 1, "source_base": index["source_sha"], "producer": "I50-test",
                          "entries": entries, **overrides})
        return str(self.root / name)

    def entry(self, key="file.new", english="New %1", **extra):
        return {"key": key, "english": english, "kind": "text", "placeholders": ["%1"],
                "context": "New file action", "call_sites": ["src/new.rs:1"],
                "omarchy_reference": None, "replaces": None, **extra}

    def test_i50_extract_reproducible(self):
        index = self.freeze()
        before = self.index.read_bytes()
        self.freeze()
        self.assertEqual(before, self.index.read_bytes())
        self.assertEqual(index["schema"], 1)
        self.assertEqual(list(index["keys"]), sorted(self.catalog))
        self.assertEqual(index["catalog_sha256"], hashlib.sha256(
            json.dumps(self.catalog, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest())
        self.assertEqual(index["keys"]["unused.label"]["coverage"], "missing-callsite")
        self.run_tool("check")
        self.write("src/new.rs", 'fn draw() { show("A new literal"); }')
        self.run_tool("check", ok=False)

    def test_i50_extract_changed_english_refused(self):
        self.freeze()
        self.catalog["file.open"] = "Open file"
        self.write("i18n/en/flea.json", self.catalog)
        self.run_tool("check", ok=False)

    def test_i50_keymap_regeneration_preserves_aliases_and_frozen_index(self):
        self.catalog["keys.action.mute"] = "omarchy:keybindings.Mute"
        self.catalog["keys.retired"] = "omarchy:keybindings.Mute"
        self.write("i18n/en/flea.json", self.catalog)
        self.write("keys.toml", (REPO / "keys.toml").read_text())
        self.write("ui/js/Keymap.js", "")
        self.write("src/keys_generated.rs", "")
        command = [sys.executable, "-B", str(REPO / "tools/flea-keymap-gen"),
                   str(self.root / "ui/js/Keymap.js"), str(self.root / "keys.toml"),
                   "--rust", str(self.root / "src/keys_generated.rs"),
                   "--catalog", str(self.root / "i18n/en/flea.json")]
        result = subprocess.run(command, capture_output=True, text=True, env=self.git_env)
        self.assertEqual(result.returncode, 0, result.stderr)
        generated = json.loads((self.root / "i18n/en/flea.json").read_text())
        self.assertEqual(generated["keys.action.mute"], "omarchy:keybindings.Mute")
        self.assertEqual(generated["file.open"], self.catalog["file.open"])
        self.assertNotIn("keys.retired", generated)
        index = self.freeze()
        self.assertEqual(index["keys"]["keys.action.mute"]["resolved_english"], "Mute")
        catalog_bytes = (self.root / "i18n/en/flea.json").read_bytes()
        index_bytes = self.index.read_bytes()
        result = subprocess.run(command, capture_output=True, text=True, env=self.git_env)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual((self.root / "i18n/en/flea.json").read_bytes(), catalog_bytes)
        self.run_tool("check")
        self.assertEqual(self.index.read_bytes(), index_bytes)

    def test_i50_extract_direct_reference(self):
        index = self.freeze()
        reference = index["direct_references"]["omarchy:shell.common.cancel"]
        self.assertEqual(reference["english"], "Cancel")
        self.assertEqual(reference["call_sites"][0]["path"], "src/example.rs")
        self.assertEqual(index["keys"]["keys.action.mute"]["omarchy_reference"],
                         "omarchy:keybindings.Mute")
        self.write("omarchy/default/i18n/en/shell.json", {})
        self.run_tool("check", ok=False)

    def test_i50_delta_conflict_refused(self):
        self.freeze()
        self.write("src/new.rs", 'fn draw(i: &I18n) { i.tr("file.new"); }')
        one = self.delta([self.entry()])
        two = self.delta([self.entry(english="Different %1")], "other.json")
        before = (self.root / "i18n/en/flea.json").read_bytes()
        self.run_tool("delta", "--delta", one, "--delta", two, "--apply", ok=False)
        self.assertEqual(before, (self.root / "i18n/en/flea.json").read_bytes())

    def test_i50_delta_stale_base_refused(self):
        self.freeze()
        path = self.delta([], source_base="0" * 40)
        self.run_tool("delta", "--delta", path, ok=False)

    def test_i50_delta_removed_live_caller_refused(self):
        index = self.freeze()
        entry = self.entry("file.open", None, kind="remove", placeholders=[],
                           call_sites=["src/example.rs:1"],
                           previous_source_hash=index["keys"]["file.open"]["source_hash"])
        path = self.delta([entry])
        self.run_tool("delta", "--delta", path, "--apply", ok=False)

    def test_i50_plural_context_preserved(self):
        index = self.freeze()
        record = index["keys"]["files.count"]
        self.assertEqual(record["kind"], "plural")
        self.assertEqual(record["placeholders"], ["%1"])
        self.assertEqual(record["english"], self.catalog["files.count"])
        self.assertTrue(record["context"])
        self.assertTrue(record["call_sites"])
        path = self.delta([self.entry("files.count", {"one": "%1 entry", "other": "%1 entries"},
                                      kind="plural", context=record["context"],
                                      call_sites=["src/example.rs:1"],
                                      previous_source_hash="0" * 12)])
        self.run_tool("delta", "--delta", path, ok=False)

    def test_i50_delta_applies_and_checks_callers(self):
        self.freeze()
        self.write("src/new.rs", 'fn draw(i: &I18n) { i.tr("file.new"); }')
        path = self.delta([self.entry()])
        previous_digest = hashlib.sha256(self.index.read_bytes()).hexdigest()
        delta_index = self.root / "delta-index.json"
        self.run_tool("delta", "--delta", path, "--apply", "--output", str(delta_index))
        emitted = json.loads(delta_index.read_text())
        self.assertEqual(emitted["previous_source_index_sha256"], previous_digest)
        self.assertEqual(emitted["source_index_sha256"], hashlib.sha256(self.index.read_bytes()).hexdigest())
        self.assertEqual(emitted["entries"][0]["key"], "file.new")
        self.assertIsNone(emitted["entries"][0]["previous_source_hash"])
        index = json.loads(self.index.read_text())
        self.assertEqual(index["keys"]["file.new"]["context"], "New file action")
        self.run_tool("check")
        entry = self.entry("file.absent", call_sites=["src/new.rs:999"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False)

    def test_i50_delta_refuses_unrecorded_english_drift(self):
        self.freeze()
        self.catalog["file.open"] = "Different action"
        self.write("i18n/en/flea.json", self.catalog)
        self.run_tool("delta", "--delta", self.delta([]), ok=False)

    def test_i50_dynamic_caller_prevents_removal(self):
        self.write("src/dynamic.rs", 'fn draw(i: &I18n, name: &str) { i.tr(&format!("file.{name}")); }')
        index = self.freeze()
        self.assertTrue(any(site["usage"] == "dynamic-candidate"
                            for site in index["keys"]["file.open"]["call_sites"]))
        self.write("src/example.rs", "")
        entry = self.entry("file.open", None, kind="remove", placeholders=[],
                           call_sites=sorted({f"{site['path']}:{site['line']}" for site in index["keys"]["file.open"]["call_sites"]}),
                           previous_source_hash=index["keys"]["file.open"]["source_hash"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False)

    def test_i50_comments_are_not_callers(self):
        self.write("src/example.rs", '// i.tr("file.open");\n/* i.tr("files.count"); */\n')
        index = self.freeze()
        self.assertEqual(index["keys"]["file.open"]["call_sites"], [])
        self.assertEqual(index["keys"]["files.count"]["call_sites"], [])

    def test_i50_rust_quote_character_does_not_hide_callers(self):
        self.write("src/example.rs", "fn draw(i: &I18n) { let quote = '\"'; i.tr(\"file.open\"); }\n")
        index = self.freeze()
        self.assertTrue(index["keys"]["file.open"]["call_sites"])

    def test_i50_declaration_after_call_is_not_translation_coverage(self):
        self.write("src/example.rs", 'fn draw(i: &I18n) { i.tr("file.open"); let unused = "unused.label"; }\n')
        index = self.freeze()
        self.assertEqual(index["keys"]["unused.label"]["call_sites"][0]["usage"], "key-declaration")

    def removal(self, index):
        record = index["keys"]["file.open"]
        sites = sorted({f"{site['path']}:{site['line']}" for site in record["call_sites"]})
        return self.entry("file.open", None, kind="remove", placeholders=[], call_sites=sites,
                          context=record["context"], previous_source_hash=record["source_hash"])

    def test_i50_positional_dynamic_caller_prevents_removal(self):
        self.write("src/dynamic.rs", 'fn draw(i: &I18n, name: &str) { i.tr(&format!("file.{}", name)); }')
        index = self.freeze()
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False)

    def test_i50_unresolved_key_expression_prevents_removal(self):
        self.write("src/dynamic.rs", 'fn draw(i: &I18n, key: &str) { i.tr(key); }')
        index = self.freeze()
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False)

    def test_i50_javascript_comment_marker_inside_string(self):
        self.write("ui/example.js", "function draw(i) { const marker = '//'; return i.tr('file.open'); }\n")
        index = self.freeze()
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False)

    def test_i50_ambiguous_javascript_regex_fails_closed(self):
        self.write("ui/example.js", "const marker = /[!'()*~]/g;\nfunction draw(i) { return i.tr('file.open'); }\n")
        index = self.freeze()
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False)

    def test_i50_javascript_regex_double_quote_prevents_removal(self):
        self.write("ui/example.js", 'const marker = /[\"]/g; function draw(i) { return i.tr("file.open"); }\n')
        index = self.freeze()
        self.assertTrue(any(site["path"] == "ui/example.js" and site["usage"] == "translated-call"
                            for site in index["keys"]["file.open"]["call_sites"]))
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False,
                      error="live caller: ui/example.js:1")

    def test_i50_javascript_lexical_tokens_allow_dead_key_removal(self):
        for source in ('// harmless comment\n', '/* harmless / comment */\n',
                       'const ratio = width / 2; const value = a / b / c;\n',
                       'const url = "https://example.test/"; const slash = "`/";\n',
                       'const pattern = /[\"\']/g; const escaped = /a\\/b/;\n'):
            with self.subTest(source=source):
                self.write("ui/example.js", source)
                self.write("src/example.rs", 'fn draw(i: &I18n) { i.tr("file.open"); }\n')
                self.write("i18n/en/flea.json", self.catalog)
                index = self.freeze()
                self.assertFalse(any(item.get("blocks_removals") for item in index["dynamic_callsite_handoffs"]))
                self.write("src/example.rs", "")
                self.run_tool("delta", "--delta", self.delta([self.removal(index)]), "--apply")
                self.run_tool("check")

    def test_i50_owner_can_resolve_ambiguous_javascript(self):
        self.write("ui/example.js", 'if (flag) /[\"]/g.test(value);\n')
        index = self.freeze()
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False,
                      error="javascript-unresolved-slash")
        self.write("ui/example.js", 'if (flag) new RegExp(\'[\"]\', "g").test(value);\n')
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), "--apply")
        self.run_tool("check")

    def test_i50_javascript_division_cannot_hide_callers(self):
        for left in ("width", "options.return", "options.await", "(width)", "width++", "await"):
            with self.subTest(left=left):
                self.write("ui/example.js", f'const value = {left} / i.tr("file.open") / scale;\n')
                index = self.freeze()
                self.write("src/example.rs", "")
                result = self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False)
                self.assertTrue("live caller: ui/example.js:1" in result.stderr or
                                "javascript-unresolved-slash at ui/example.js:1" in result.stderr, result.stderr)

    def test_i50_javascript_template_remains_an_explicit_blocker(self):
        self.write("ui/example.js", 'const value = `text ${i.tr("file.open")}`;\n')
        index = self.freeze()
        self.write("src/example.rs", "")
        self.run_tool("delta", "--delta", self.delta([self.removal(index)]), ok=False,
                      error="javascript-template-needs-owner-verification at ui/example.js:1")

    def test_i50_ambiguous_javascript_preserves_later_callers(self):
        for prefix in ('width: (parent.width - 20) / 2\n',
                       'const banner = `unverified template`;\n',
                       'const banner = `raw "quote\nand another\' line`;\n',
                       'const banner = `text ${"inner ` marker"}`;\n'):
            with self.subTest(prefix=prefix):
                self.write("ui/example.qml", prefix + 'text: i.tr("file.open")\n')
                index = self.freeze()
                site = next(site for site in index["keys"]["file.open"]["call_sites"]
                            if site["path"] == "ui/example.qml")
                self.assertEqual((site["line"], site["usage"]),
                                 (prefix.count("\n") + 1, "unverified-source-occurrence"))
                self.assertTrue(any(item.get("blocks_removals") and item["path"] == "ui/example.qml"
                                    for item in index["dynamic_callsite_handoffs"]))

    def test_i50_delta_addition_after_ambiguous_slash_keeps_its_caller(self):
        self.freeze()
        self.write("ui/example.qml", 'width: (parent.width - 20) / 2\ntext: i.tr("file.new")\n')
        entry = self.entry(call_sites=["ui/example.qml:2"])
        self.run_tool("delta", "--delta", self.delta([entry]), "--apply")
        index = json.loads(self.index.read_text())
        site = index["keys"]["file.new"]["call_sites"][0]
        self.assertEqual((site["path"], site["line"], site["usage"]),
                         ("ui/example.qml", 2, "unverified-source-occurrence"))
        self.run_tool("check")

    def test_i50_transformed_format_key_prevents_removal(self):
        self.write("src/dynamic.rs", 'fn draw(i: &I18n, name: &str) { i.tr(&format!("file.{}", name).replace("file.", "unused.")); }')
        index = self.freeze()
        entry = self.entry("unused.label", None, kind="remove", placeholders=[], call_sites=[],
                           previous_source_hash=index["keys"]["unused.label"]["source_hash"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False,
                      error="unresolved-key-expression at src/dynamic.rs:1")

    def test_i50_conditional_format_key_prevents_removal(self):
        self.write("src/dynamic.rs", 'fn draw(i: &I18n, key: &str) { i.tr(&if flag { format!("file.{}", name) } else { key.to_owned() }); }')
        index = self.freeze()
        entry = self.entry("unused.label", None, kind="remove", placeholders=[], call_sites=[],
                           previous_source_hash=index["keys"]["unused.label"]["source_hash"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False,
                      error="unresolved-key-expression at src/dynamic.rs:1")

    def test_i50_delta_requires_all_live_callers(self):
        self.freeze()
        self.write("src/new.rs", 'fn draw(i: &I18n) { i.tr("file.new"); }')
        self.write("src/other.rs", 'fn draw(i: &I18n) { i.tr("file.new"); }')
        self.run_tool("delta", "--delta", self.delta([self.entry()]), ok=False)

    def test_i50_direct_reference_does_not_block_dead_key_removal(self):
        index = self.freeze()
        entry = self.entry("unused.label", None, kind="remove", placeholders=[], call_sites=[],
                           previous_source_hash=index["keys"]["unused.label"]["source_hash"])
        self.run_tool("delta", "--delta", self.delta([entry]), "--apply")
        self.run_tool("check")

    def test_i50_existing_key_argument_contract_cannot_change(self):
        index = self.freeze()
        old = index["keys"]["file.open"]
        entry = self.entry("file.open", "Open %1", context=old["context"],
                           previous_source_hash=old["source_hash"], call_sites=["src/example.rs:1"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False)

    def test_i50_delta_preserves_semantics_and_placeholders(self):
        index = self.freeze()
        entry = self.entry("file.open", "New meaning %1", placeholders=["%1"],
                           previous_source_hash=index["keys"]["file.open"]["source_hash"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False)
        entry = self.entry("files.count", {"one": "%1 entry", "other": "%2 entries"}, kind="plural",
                           context=index["keys"]["files.count"]["context"],
                           previous_source_hash=index["keys"]["files.count"]["source_hash"])
        self.run_tool("delta", "--delta", self.delta([entry]), ok=False)


if __name__ == "__main__":
    unittest.main()
