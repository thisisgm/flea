"""S2 pointer wording must preserve the label shared with search results."""
import hashlib
import json
from pathlib import Path
import tomllib
import unittest


ROOT = Path(__file__).resolve().parents[2]


class S2CatalogTests(unittest.TestCase):
    def test_s2_each_live_pointer_label_keeps_its_locale_translation(self):
        pointers = tomllib.loads((ROOT / "keys.toml").read_text())["pointer"]
        gestures = [("listing", "result"), ("neighbour", "file")]
        for place, row in gestures:
            binding = next(item for item in pointers if item["where"] == place
                           and item["row"] == row and item["press"] == "left")
            key = "keys.label." + hashlib.sha256(binding["label"].encode()).hexdigest()[:12]
            for path in sorted((ROOT / "i18n").glob("*/flea.json")):
                with self.subTest(locale=path.parent.name, gesture=(place, row)):
                    value = json.loads(path.read_text()).get(key)
                    self.assertIsInstance(value, str, f"missing live pointer label {key}")
                    self.assertTrue(value.strip())

    def test_s2_manifest_keeps_the_shared_search_result_label(self):
        manifest = json.loads((ROOT / "i18n/translation-manifest.json").read_text())
        for path in sorted((ROOT / "i18n").glob("*/flea.json")):
            with self.subTest(locale=path.parent.name):
                self.assertIn("flea:keys.label.bfa65befcad0", manifest[path.parent.name])


if __name__ == "__main__":
    unittest.main()
