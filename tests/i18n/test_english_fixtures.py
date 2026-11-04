"""Pinned English data for native GUI and TUI tests must resolve the catalog's desktop aliases."""
import hashlib
import json
from pathlib import Path
import sys
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "src/i18n/fixtures/omarchy"
sys.path.insert(0, str(ROOT / "tools"))
from keymap_rust import catalog_entries


class EnglishFixtureTests(unittest.TestCase):
    def test_english_fixture_provenance_matches_the_roster_pin(self):
        pin = json.loads((ROOT / "ci/i18n-pin.json").read_text())
        provenance = json.loads((FIXTURES / "provenance.json").read_text())
        self.assertEqual(provenance["upstream"], pin["upstream"])
        self.assertEqual(set(provenance["files"]), {"shell", "keybindings"})
        for kind, record in provenance["files"].items():
            source = "default/i18n/en/" + kind + ".json"
            self.assertEqual(record["source"], source)
            self.assertEqual(hashlib.sha256((FIXTURES / source).read_bytes()).hexdigest(), record["sha256"])

    def test_every_catalog_alias_has_pinned_english_data(self):
        catalog = json.loads((ROOT / "i18n/en/flea.json").read_text())
        documents = {}
        for key, value in catalog.items():
            if not isinstance(value, str) or not value.startswith("omarchy:"):
                continue
            kind, name = value.removeprefix("omarchy:").split(".", 1)
            if kind not in documents:
                documents[kind] = json.loads((FIXTURES / "default/i18n/en" / (kind + ".json")).read_text())
            self.assertIsInstance(documents[kind][name], str, key)
            self.assertTrue(documents[kind][name].strip(), key)
        generated = catalog_entries(tomllib.loads((ROOT / "keys.toml").read_text()))
        for key, label in generated.items():
            value = catalog[key]
            if isinstance(value, str) and value.startswith("omarchy:"):
                kind, name = value.removeprefix("omarchy:").split(".", 1)
                self.assertEqual(documents[kind][name], label, key)


if __name__ == "__main__":
    unittest.main()
