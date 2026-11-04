"""Small fabricated catalogs exercise the public quality CLI and provenance gates."""
import base64
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


REPO = Path(__file__).resolve().parents[2]
TOOL = REPO / "tools/flea-i18n-quality"
FIXTURES = Path(__file__).parent / "fixtures/quality"


def digest(value):
    text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, sort_keys=True)
    return hashlib.sha256(text.encode()).hexdigest()[:12]


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")


class QualityTests(unittest.TestCase):
    def setUp(self):
        parent = REPO / ".superpowers/m9-i50-02"
        parent.mkdir(parents=True, exist_ok=True)
        self.temporary = tempfile.TemporaryDirectory(prefix="quality.", dir=parent)
        self.box = Path(self.temporary.name).resolve()
        (self.box / ".flea-test-sandbox").touch()
        self.addCleanup(self.temporary.cleanup)
        self.source = self.box / "source"
        self.index = self.source / "source-index.json"
        self.omarchy = self.box / "omarchy"
        self.git_revision = "1" * 40
        binaries = self.box / "bin"
        binaries.mkdir()
        git = binaries / "git"
        git.write_text('#!/usr/bin/env python3\nimport os, pathlib, sys\n'
                       'root = pathlib.Path(sys.argv[sys.argv.index("-C") + 1])\n'
                       'if "rev-parse" in sys.argv:\n'
                       '    print(root); print(os.environ["QUALITY_FIXTURE_GIT_SHA"])\n'
                       'elif "show" in sys.argv:\n'
                       '    sys.stdout.buffer.write((root / sys.argv[-1].split(":", 1)[1]).read_bytes())\n'
                       'else:\n    sys.exit(1)\n')
        git.chmod(0o755)
        self.root = self.box / "private"
        self.exceptions = self.box / "exceptions.json"
        self.language = "de"
        self.tool = TOOL
        self.english = {"action": "Open %1", "brand": "Flea",
                        "count": {"one": "%1 item", "other": "%1 items"}}
        self.catalog = {"action": "Oeffne %1", "brand": "Floh",
                        "count": {"one": "%1 Ding", "other": "%1 Dinge"}}
        write(self.source / "en/flea.json", self.english)
        write(self.exceptions, {"schema": 1, "language": self.language, "entries": {}})
        data = self.omarchy / "default/i18n"
        data.mkdir(parents=True)
        (data / "plural.jq").write_bytes((REPO / "src/i18n/fixtures/plural.jq").read_bytes())
        (data / "locales.json").write_bytes((REPO / "src/i18n/fixtures/locales.json").read_bytes())
        self.freeze()

    def freeze(self):
        data = self.omarchy / "default/i18n"
        records, references = {}, {}
        for key, value in sorted(self.english.items()):
            alias = value if isinstance(value, str) and value.startswith("omarchy:") else None
            resolved = value
            if alias:
                kind, name = alias.removeprefix("omarchy:").split(".", 1)
                resolved = json.loads((data / "en" / f"{kind}.json").read_text())[name]
                references[alias] = {"english": resolved, "source_hash": digest(resolved), "aliases": [key]}
            records[key] = {"english": value, "source_hash": digest(value),
                            "kind": "plural" if isinstance(value, dict) else "text",
                            "placeholders": ["%1"] if key != "brand" else [],
                            "plural_categories": sorted(value) if isinstance(value, dict) else [],
                            "context": "Fabricated quality test", "call_sites": [],
                            "coverage": "missing-callsite", "omarchy_reference": alias,
                            "resolved_english": resolved, "resolved_source_hash": digest(resolved)}
        write(self.index, {
            "schema": 1, "source_sha": "0" * 40, "source_ref": "HEAD", "source_ref_sha": "0" * 40,
            "catalog_sha256": hashlib.sha256(json.dumps(
                self.english, ensure_ascii=False, sort_keys=True, separators=(",", ":")
            ).encode()).hexdigest(),
            "catalog_file_sha256": sha(self.source / "en/flea.json"),
            "reference_checkout_sha": "1" * 40,
            "roster": {"sha256": hashlib.sha256(json.dumps(json.loads((data / "locales.json").read_text()),
                        ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
                       "file_sha256": sha(data / "locales.json"), "plural_sha256": sha(data / "plural.jq"),
                       "languages": sorted(json.loads((data / "locales.json").read_text()))},
            "keys": records, "references": references, "direct_references": {}})

    def cli(self, command, *extra, ok=True, root=None):
        args = [sys.executable, "-B", str(self.tool), command, "--language", self.language,
                "--source-index", str(self.index), "--catalog-root",
                str(self.source if command == "prepare" else root or self.root)]
        if command == "prepare":
            args += ["--out", str(root or self.root)]
        elif command == "check":
            args += ["--omarchy-path", str(self.omarchy), "--exceptions", str(self.exceptions), "--strict"]
        args += list(map(str, extra))
        environment = dict(os.environ, PATH=str(self.box / "bin") + os.pathsep + os.environ.get("PATH", ""),
                           QUALITY_FIXTURE_GIT_SHA=self.git_revision)
        result = subprocess.run(args, capture_output=True, text=True, timeout=40, env=environment)
        if ok:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0, result.stdout)
        return result

    def prepare(self):
        self.cli("prepare")
        self.save()

    def save(self):
        write(self.root / self.language / "flea.json", self.catalog)

    def refused(self, message):
        result = self.cli("check", ok=False)
        self.assertIn(message, result.stderr)

    def test_i50_quality_missing_key(self):
        self.prepare()
        del self.catalog["action"]
        self.save()
        self.refused("key parity")

    def test_kind_lang_optional_mime_override_keeps_receipt_provenance(self):
        self.english["mime.markdown"] = "Markdown document"
        write(self.source / "en/flea.json", self.english)
        self.freeze()
        index = json.loads(self.index.read_text())
        index["keys"]["mime.markdown"]["placeholders"] = []
        write(self.index, index)
        self.prepare()
        hashes = json.loads((self.root / "translation-manifest.json").read_text())
        hashes[self.language].pop("flea:mime.markdown")
        write(self.root / "translation-manifest.json", hashes)
        receipt = self.box / "mime-receipt.json"
        self.cli("check")
        self.cli("receipt", "--out", receipt)
        write(self.source / self.language / "flea.json", self.catalog)
        write(self.source / "translation-manifest.json", hashes)
        self.cli("prepare", "--translation-receipt", receipt, root=self.box / "mime-reopen")
        del self.catalog["action"]
        self.save()
        self.refused("key parity")

    def test_kind_lang_missing_override_does_not_skip_english_provenance(self):
        self.english["mime.markdown"] = "Markdown document"
        write(self.source / "en/flea.json", self.english)
        self.freeze()
        index = json.loads(self.index.read_text())
        index["keys"]["mime.markdown"]["placeholders"] = []
        write(self.index, index)
        self.prepare()
        hashes = json.loads((self.root / "translation-manifest.json").read_text())
        hashes[self.language].pop("flea:mime.markdown")
        write(self.root / "translation-manifest.json", hashes)
        self.cli("check")
        index["keys"]["mime.markdown"]["resolved_english"] = "Outdated source"
        index["keys"]["mime.markdown"]["resolved_source_hash"] = digest("Outdated source")
        write(self.index, index)
        write(self.root / "source-index.json", index)
        self.refused("resolved English reference drift")

    def test_i50_quality_duplicate_key(self):
        self.prepare()
        (self.root / "de/flea.json").write_bytes((FIXTURES / "duplicate.json").read_bytes())
        self.refused("duplicate JSON key")

    def test_i50_quality_holes_per_form(self):
        self.prepare()
        self.catalog["count"]["one"] = "Ein Ding"
        self.save()
        self.refused("placeholder")

    def language_case(self, language, categories):
        self.language = language
        write(self.exceptions, {"schema": 1, "language": language, "entries": {}})
        self.catalog["count"] = {category: f"%1 {category}x" for category in categories}
        self.prepare()

    def test_i50_quality_ar_six_categories(self):
        self.language_case("ar", ["zero", "one", "two", "few", "many", "other"])
        self.cli("check")
        del self.catalog["count"]["zero"]
        self.save()
        self.refused("plural categories")

    def test_i50_quality_pl_other_required(self):
        self.language_case("pl", ["one", "few", "many"])
        self.refused("other")
        self.catalog["count"]["other"] = "%1 inne"
        self.save()
        self.cli("check")

    def test_i50_quality_million_many(self):
        self.language_case("fr", ["one", "other"])
        self.refused("many")
        self.catalog["count"]["many"] = "%1 millions"
        self.save()
        self.cli("check")

    def test_i50_quality_stale_nonzero(self):
        self.prepare()
        path = self.root / "translation-manifest.json"
        manifest = json.loads(path.read_text())
        manifest["de"]["flea:action"] = "0123456789ab"
        write(path, manifest)
        self.refused("stale")
        del manifest["de"]["flea:action"]
        write(path, manifest)
        self.refused("missing source hash")

    def test_i50_quality_reference_target_missing(self):
        self.english["brand"] = "omarchy:shell.common.name"
        write(self.source / "en/flea.json", self.english)
        self.catalog["brand"] = self.english["brand"]
        write(self.omarchy / "default/i18n/en/shell.json", {"common.name": "Widget"})
        write(self.omarchy / "default/i18n/de/shell.json", {})
        self.freeze()
        self.prepare()
        self.refused("target-language reference")
        write(self.omarchy / "default/i18n/de/shell.json", {"common.name": "Ding"})
        self.cli("check")

    def test_i50_quality_english_exception_scoped(self):
        self.prepare()
        self.catalog["brand"] = "Flea"
        self.save()
        self.refused("English equality")
        write(self.exceptions, {"schema": 1, "language": "de", "entries": {
            "brand": {"rationale": "Product name", "reviewer": "Fixture reviewer",
                      "source_hash": digest("Flea"), "translation": "Flea"}}})
        self.cli("check")
        self.catalog["action"] = self.english["action"]
        self.save()
        self.refused("English equality")

    def test_i50_quality_receipt_tamper_refused(self):
        self.prepare()
        receipt = self.box / "receipt.json"
        self.cli("receipt", "--out", receipt, ok=False)
        self.cli("check")
        self.cli("receipt", "--out", receipt)
        data = json.loads(receipt.read_text())
        catalog_path = str(self.root / "de/flea.json")
        self.assertEqual(base64.b64decode(data["inputs"][catalog_path]["bytes"]),
                         (self.root / "de/flea.json").read_bytes())
        self.catalog["brand"] = "Anders"
        self.save()
        self.cli("receipt", "--out", receipt, ok=False)
        self.assertFalse(receipt.exists(), "A stale success receipt survived")

    def test_i50_quality_parallel_roots_no_shared_write(self):
        source_before = {str(p): p.read_bytes() for p in self.source.rglob("*") if p.is_file()}
        roots = [self.box / f"private-{i}" for i in range(8)]
        with ThreadPoolExecutor(max_workers=8) as pool:
            list(pool.map(lambda root: self.cli("prepare", root=root), roots))
        self.assertEqual(source_before, {str(p): p.read_bytes() for p in self.source.rglob("*") if p.is_file()})
        self.assertEqual(len({(root / "translation-manifest.json").stat().st_ino for root in roots}), 8)

    def test_extra_empty_shape_and_source_drift(self):
        self.prepare()
        for key, value, message in [("extra", "x", "key parity"), ("brand", " ", "nonempty"),
                                    ("count", "%1 Ding", "shape")]:
            with self.subTest(message=message):
                original = dict(self.catalog)
                self.catalog[key] = value
                self.save()
                self.refused(message)
                self.catalog = original
        self.save()
        write(self.root / "en/flea.json", {**self.english, "brand": "Changed"})
        self.refused("source index")

    def test_receipt_binds_policy_exceptions_and_tool(self):
        self.prepare()
        self.cli("check")
        self.exceptions.write_text(self.exceptions.read_text() + " ")
        self.cli("receipt", "--out", self.box / "receipt.json", ok=False)
        self.cli("check")
        path = self.omarchy / "default/i18n/plural.jq"
        original_rules = path.read_bytes()
        path.write_text(path.read_text() + "\n")
        self.cli("receipt", "--out", self.box / "receipt.json", ok=False)
        path.write_bytes(original_rules)
        copied_tools = self.box / "tool-copy"
        copied_tools.mkdir()
        self.tool = copied_tools / TOOL.name
        self.tool.write_bytes(TOOL.read_bytes())
        (copied_tools / "flea-i18n-manifest").write_bytes((REPO / "tools/flea-i18n-manifest").read_bytes())
        self.cli("check")
        self.tool.write_bytes(self.tool.read_bytes() + b"\n")
        self.cli("receipt", "--out", self.box / "receipt.json", ok=False)

    def test_previous_receipt_requires_exact_bytes(self):
        self.prepare()
        self.cli("check")
        receipt = self.box / "receipt.json"
        self.cli("receipt", "--out", receipt)
        write(self.source / "de/flea.json", self.catalog)
        write(self.source / "translation-manifest.json", json.loads((self.root / "translation-manifest.json").read_text()))
        self.cli("prepare", root=self.box / "missing", ok=False)
        self.cli("prepare", "--translation-receipt", receipt, root=self.box / "review")
        source_catalog = self.source / "de/flea.json"
        original_bytes = source_catalog.read_bytes()
        source_catalog.write_bytes(original_bytes + b" ")
        self.cli("prepare", "--translation-receipt", receipt, root=self.box / "bad-format", ok=False)
        source_catalog.write_bytes(original_bytes)
        write(self.source / "de/flea.json", {**self.catalog, "brand": "Tampered"})
        self.cli("prepare", "--translation-receipt", receipt, root=self.box / "bad-review", ok=False)

    def test_delta_advances_only_changed_provided_keys(self):
        self.prepare()
        self.cli("check")
        receipt = self.box / "receipt.json"
        self.cli("receipt", "--out", receipt)
        write(self.source / "de/flea.json", {**self.catalog, "action": "Neue %1"})
        write(self.source / "translation-manifest.json", json.loads((self.root / "translation-manifest.json").read_text()))
        old = digest(self.english["action"])
        self.english["action"] = "New %1"
        write(self.source / "en/flea.json", self.english)
        self.freeze()
        delta = self.box / "delta.json"
        write(delta, {"schema": 1, "source_base": "0" * 40,
                      "producers": [{"producer": "fixture", "delta_sha256": "f" * 64}],
                      "previous_source_index_sha256": json.loads(receipt.read_text())["source_index_sha256"],
                      "source_index_sha256": sha(self.index), "entries": [
                          {"key": "action", "previous_source_hash": old,
                           "source_hash": digest("New %1"), "kind": "text"}]})
        root = self.box / "delta-root"
        self.cli("prepare", "--translation-receipt", receipt, "--delta-index", delta, root=root)
        manifest = json.loads((root / "translation-manifest.json").read_text())
        self.assertEqual(manifest["de"]["flea:action"], digest("New %1"))
        self.cli("check", root=root)
        write(self.source / "de/flea.json", self.catalog)
        self.cli("prepare", "--translation-receipt", receipt, "--delta-index", delta,
                 root=self.box / "unchanged-delta", ok=False)

    def test_existing_catalog_cannot_mint_fresh_hashes(self):
        write(self.source / "de/flea.json", self.catalog)
        self.cli("prepare", ok=False)

    def test_i50_quality_producer_generated_schema(self):
        self.index.write_bytes((FIXTURES / "producer-source-index.json").read_bytes())
        (self.source / "en/flea.json").write_bytes((FIXTURES / "producer-en.json").read_bytes())
        self.cli("prepare")

    def test_alternate_schema_one_refused(self):
        self.prepare()
        index = json.loads(self.index.read_text())
        index["english_sha256"] = index.pop("catalog_sha256", "0" * 64)
        index["entries"] = list(index.pop("keys", {}).values())
        write(self.index, index)
        self.cli("check", ok=False)

    def test_alias_exception_binds_token_and_resolved_identities(self):
        reference = "omarchy:shell.common.name"
        self.english["brand"] = reference
        write(self.source / "en/flea.json", self.english)
        data = self.omarchy / "default/i18n"
        write(data / "en/shell.json", {"common.name": "Flea"})
        write(data / "de/shell.json", {"common.name": "Flea"})
        self.freeze()
        self.catalog["brand"] = reference
        self.prepare()
        exception = {"rationale": "Fixture product name", "reviewer": "Fixture reviewer",
                     "source_hash": digest(reference), "translation": reference,
                     "resolved_english_sha256": hashlib.sha256(b'"Flea"').hexdigest(),
                     "resolved_translation_sha256": hashlib.sha256(b'"Flea"').hexdigest()}
        write(self.exceptions, {"schema": 1, "language": "de", "entries": {"brand": exception}})
        self.cli("check")
        exception["source_hash"] = digest("Flea")
        write(self.exceptions, {"schema": 1, "language": "de", "entries": {"brand": exception}})
        self.refused("English equality")
        exception["source_hash"] = digest(reference)
        exception["resolved_translation_sha256"] = "0" * 64
        write(self.exceptions, {"schema": 1, "language": "de", "entries": {"brand": exception}})
        self.refused("resolved identities")

    def test_plural_form_alias_exception_requires_resolved_identities(self):
        self.prepare()
        data = self.omarchy / "default/i18n"
        write(data / "de/shell.json", {"common.count": "%1 items"})
        self.catalog["count"]["other"] = "omarchy:shell.common.count"
        self.save()
        write(self.exceptions, {"schema": 1, "language": "de", "entries": {"count": {
            "rationale": "Fixture only", "reviewer": "Fixture reviewer",
            "source_hash": digest(self.english["count"]), "translation": self.catalog["count"]}}})
        self.refused("resolved identities")
        exceptions = json.loads(self.exceptions.read_text())
        entry = exceptions["entries"]["count"]
        entry["resolved_english_sha256"] = hashlib.sha256(json.dumps(
            self.english["count"], sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        resolved = {**self.catalog["count"], "other": "%1 items"}
        entry["resolved_translation_sha256"] = hashlib.sha256(json.dumps(
            resolved, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
        write(self.exceptions, exceptions)
        self.cli("check")

    def test_null_review_metadata_refused(self):
        self.prepare()
        self.catalog["brand"] = "Flea"
        self.save()
        write(self.exceptions, {"schema": 1, "language": "de", "entries": {"brand": {
            "rationale": None, "reviewer": None, "source_hash": digest("Flea"), "translation": "Flea"}}})
        self.refused("English equality")

    def test_pinned_checkout_identity_required(self):
        self.prepare()
        index = json.loads(self.index.read_text())
        index["reference_checkout_sha"] = "2" * 40
        write(self.index, index)
        (self.root / "source-index.json").write_bytes(self.index.read_bytes())
        self.refused("Omarchy checkout")

    def test_prepare_does_not_copy_unvalidated_concurrent_edits(self):
        self.prepare()
        self.cli("check")
        receipt = self.box / "receipt.json"
        self.cli("receipt", "--out", receipt)
        self.english["action"] = "New %1"
        write(self.source / "en/flea.json", self.english)
        self.freeze()
        source_catalog = self.source / "de/flea.json"
        updated = {**self.catalog, "action": "Neue %1"}
        write(source_catalog, updated)
        write(self.source / "translation-manifest.json", json.loads((self.root / "translation-manifest.json").read_text()))
        delta = self.box / "delta.json"
        write(delta, {"schema": 1, "source_base": "0" * 40,
                      "previous_source_index_sha256": json.loads(receipt.read_text())["source_index_sha256"],
                      "source_index_sha256": sha(self.index),
                      "producers": [{"producer": "fixture", "delta_sha256": "f" * 64}],
                      "entries": [{"key": "action", "kind": "text", "previous_source_hash": digest("Open %1"),
                                   "source_hash": digest("New %1")}]})
        wrapper = self.box / "concurrent-prepare.py"
        wrapper.write_text('import json, runpy, sys\nfrom pathlib import Path\n'
                           f'namespace = runpy.run_path({str(TOOL)!r})\n'
                           f'target = Path({str(source_catalog)!r})\n'
                           f'changed = {repr({**updated, "brand": "Unreviewed"})}\n'
                           'original = Path.read_bytes\n'
                           'def read(path):\n'
                           '    if path == target:\n'
                           '        target.write_text(json.dumps(changed))\n'
                           '    return original(path)\n'
                           'Path.read_bytes = read\n'
                           'sys.exit(namespace["main"]())\n')
        self.tool = wrapper
        result = self.cli("prepare", "--translation-receipt", receipt, "--delta-index", delta,
                          root=self.box / "raced", ok=False)
        self.assertIn("outside named delta", result.stderr)

    def test_receipt_source_identity_matches_embedded_bytes(self):
        self.prepare()
        self.cli("check")
        receipt = self.box / "receipt.json"
        self.cli("receipt", "--out", receipt)
        write(self.source / "de/flea.json", self.catalog)
        write(self.source / "translation-manifest.json", json.loads((self.root / "translation-manifest.json").read_text()))
        index = json.loads(self.index.read_text())
        index["source_note"] = "new identity with same English"
        write(self.index, index)
        changed = json.loads(receipt.read_text())
        changed["source_index_sha256"] = sha(self.index)
        write(receipt, changed)
        result = self.cli("prepare", "--translation-receipt", receipt, root=self.box / "identity-mismatch", ok=False)
        self.assertIn("receipt source index", result.stderr)

    def test_delta_accepts_stable_alias_after_target_reference_repin(self):
        reference = "omarchy:shell.common.name"
        data = self.omarchy / "default/i18n"
        write(data / "en/shell.json", {"common.name": "Widget"})
        write(data / "de/shell.json", {"common.name": "Ding"})
        self.english["brand"] = reference
        self.catalog["brand"] = reference
        write(self.source / "en/flea.json", self.english)
        self.freeze()
        self.prepare()
        self.cli("check")
        receipt = self.box / "receipt.json"
        self.cli("receipt", "--out", receipt)
        write(self.source / "de/flea.json", self.catalog)
        write(self.source / "translation-manifest.json", json.loads((self.root / "translation-manifest.json").read_text()))
        write(data / "de/shell.json", {"common.name": "Geaendert"})
        self.git_revision = "2" * 40
        self.freeze()
        index = json.loads(self.index.read_text())
        index["reference_checkout_sha"] = self.git_revision
        write(self.index, index)
        delta = self.box / "alias-delta.json"
        write(delta, {"schema": 1, "source_base": "0" * 40,
                      "previous_source_index_sha256": json.loads(receipt.read_text())["source_index_sha256"],
                      "source_index_sha256": sha(self.index),
                      "producers": [{"producer": "fixture", "delta_sha256": "f" * 64}],
                      "entries": [{"key": "brand", "kind": "text", "previous_source_hash": digest(reference),
                                   "source_hash": digest(reference)}]})
        updated = self.box / "alias-updated"
        self.cli("prepare", "--translation-receipt", receipt, "--delta-index", delta, root=updated)
        self.cli("check", root=updated)

    def test_direct_and_plural_form_references(self):
        data = self.omarchy / "default/i18n"
        write(data / "en/shell.json", {"common.open": "Open %1", "common.count": "%1 files"})
        write(data / "de/shell.json", {"common.open": "Oeffne %1", "common.count": "%1 Dinge"})
        self.catalog["count"]["other"] = "omarchy:shell.common.count"
        index = json.loads(self.index.read_text())
        record = {"english": "Open %1", "source_hash": digest("Open %1"), "aliases": [], "call_sites": []}
        index["direct_references"] = {"omarchy:shell.common.open": record}
        index["references"] = dict(index["direct_references"])
        write(self.index, index)
        self.prepare()
        self.cli("check")
        write(data / "de/shell.json", {"common.count": "%1 Dinge"})
        self.refused("target-language reference")


if __name__ == "__main__":
    unittest.main()
