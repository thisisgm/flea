"""Retired updater sources must not remain in shipped catalogs or their indexes."""
import hashlib
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[2]
RETIRED = {"settings.about.source_aur", "settings.about.source_git",
           "settings.update.rolling", "settings.update.note_rolling"}


class OprOnlyTests(unittest.TestCase):
    def test_retired_sources_are_absent_from_every_catalog_and_index(self):
        for path in (ROOT / "i18n").glob("*/flea.json"):
            catalog = json.loads(path.read_text())
            self.assertFalse(RETIRED & catalog.keys(), str(path))
        index = json.loads((ROOT / "i18n/source-index.json").read_text())
        self.assertFalse(RETIRED & index["keys"].keys())
        manifest = json.loads((ROOT / "i18n/translation-manifest.json").read_text())
        for language, entries in manifest.items():
            self.assertFalse({"flea:" + key for key in RETIRED} & entries.keys(), language)

    def test_catalog_removal_keeps_index_digests_and_counts_current(self):
        path = ROOT / "i18n/en/flea.json"
        english = json.loads(path.read_text())
        index = json.loads((ROOT / "i18n/source-index.json").read_text())
        canonical = json.dumps(english, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
        self.assertEqual(index["catalog_sha256"], hashlib.sha256(canonical).hexdigest())
        self.assertEqual(index["catalog_file_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
        self.assertEqual(index["counts"]["keys"], len(english))
        self.assertEqual(index["counts"]["plurals"], sum(isinstance(value, dict) for value in english.values()))
        self.assertEqual(english["settings.about.source_opr"], "Omarchy Package Repository")
        self.assertEqual(english["settings.about.source_local"], "Local package")
