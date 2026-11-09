"""S3 catalog words: the arrows keycap, the running job heads and the German wrap caption are translated in every catalog; AirDrop adapter states are recorded against current English."""
import hashlib
import json
from pathlib import Path
import re
import unittest


ROOT = Path(__file__).resolve().parents[2]
HEADS = {
    "jobs.head.send_waiting": set(),
    "jobs.head.copying": {"%1", "%2", "%3"},
    "jobs.head.moving": {"%1", "%2", "%3"},
    "jobs.head.sending": {"%1", "%2", "%3"},
    "jobs.head.action": {"%1", "%2", "%3"},
    "jobs.head.checking": {"%1", "%2"},
    "jobs.head.copy_waiting": {"%1", "%2"},
    "jobs.head.move_waiting": {"%1", "%2"},
    "jobs.head.compress_waiting": {"%1", "%2"},
    "jobs.head.compressing": {"%1"},
}
AIRDROP_STATES = (
    "plugins.airdrop.adapter.broken",
    "plugins.airdrop.adapter.untested",
    "plugins.airdrop.adapter.no_service",
    "plugins.airdrop.adapter.not_ready",
)
# The broken state as nine catalogs translated the draft English "%1, does not work with AirDrop".
BROKEN_FROM_DRAFT = {
    "bg": "%1, не работи с AirDrop",
    "da": "%1, virker ikke med AirDrop",
    "fa": "⁨%1⁩، با AirDrop کار نمی‌کند",
    "ga": "%1, ní oibríonn le AirDrop",
    "he": "⁨%1⁩, לא פועל עם ⁨AirDrop⁩",
    "nl": "%1, werkt niet met AirDrop",
    "pl": "%1, nie działa z AirDrop",
    "ru": "%1, не работает с AirDrop",
    "uk": "%1, не працює з AirDrop",
}


def catalogs():
    for path in sorted((ROOT / "i18n").glob("*/flea.json")):
        yield path.parent.name, json.loads(path.read_text(encoding="utf-8"))


def forms(value):
    return list(value.values()) if isinstance(value, dict) else [value]


class S3CatalogTests(unittest.TestCase):
    def test_s3_2_every_catalog_names_the_arrows_in_its_own_words(self):
        for language, catalog in catalogs():
            with self.subTest(language=language):
                word = catalog.get("settings.keys.arrows")
                self.assertIsInstance(word, str)
                self.assertTrue(word.strip())
                if language != "en":
                    self.assertNotEqual(word, "arrows")

    def test_s7c_5_every_catalog_carries_the_running_heads_with_their_arguments(self):
        english = dict(catalogs())["en"]
        for language, catalog in catalogs():
            for key, wanted in HEADS.items():
                with self.subTest(language=language, key=key):
                    # A language's plural categories are the ones its existing menu.compressing plural already uses.
                    plural = catalog.get("menu.compressing")
                    self.assertIsInstance(plural, dict)
                    if language == "en":
                        self.assertEqual(set(plural), {"one", "other"})
                    value = catalog.get(key)
                    self.assertIsInstance(value, type(english[key]))
                    if isinstance(value, dict):
                        self.assertEqual(set(value), set(plural))
                    for form in forms(value):
                        self.assertTrue(form.strip())
                        self.assertEqual(set(re.findall(r"%[1-9]", form)), wanted, form)
                        if language != "en":
                            self.assertNotIn(form, forms(english[key]))

    def test_s3_12_the_german_wrap_caption_has_no_english_word(self):
        caption = json.loads((ROOT / "i18n/de/flea.json").read_text(encoding="utf-8"))["settings.caption.wrap"]
        self.assertEqual(caption, "Pfeil hoch ganz oben")

    def test_airdrop_adapter_states_are_recorded_against_current_english(self):
        # The manifest records the first 12 hex digits of the English sha256 each translation was made from.
        manifest = json.loads((ROOT / "i18n/translation-manifest.json").read_text(encoding="utf-8"))
        translated = dict(catalogs())
        english = translated.pop("en")
        self.assertLessEqual(set(BROKEN_FROM_DRAFT), set(translated))
        for key in AIRDROP_STATES:
            current = hashlib.sha256(english[key].encode("utf-8")).hexdigest()[:12]
            for language, catalog in translated.items():
                with self.subTest(language=language, key=key):
                    self.assertEqual(manifest[language][f"flea:{key}"], current)
                    self.assertNotEqual(catalog[key], english[key])
        for language, catalog in translated.items():
            with self.subTest(language=language, wording="current English"):
                # Only the drafts named AirDrop in these two states.
                self.assertNotIn("AirDrop", catalog["plugins.airdrop.adapter.untested"])
                self.assertNotIn("AirDrop", catalog["plugins.airdrop.adapter.not_ready"])
                self.assertNotEqual(catalog["plugins.airdrop.adapter.broken"], BROKEN_FROM_DRAFT.get(language))


if __name__ == "__main__":
    unittest.main()
