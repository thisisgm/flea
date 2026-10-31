"""Shared RTL policy and pinned Omarchy source contract."""
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE_SHA = "5978d93ae7da0a325b7493501911e3f1266cadd3"
SOURCE_PATHS = {
    "default/i18n/locales.json", "shell/Commons/I18n.qml", "shell/Commons/Util.qml",
    *[f"shell/Ui/{name}.qml" for name in (
        "OverlayWindow", "PopupCard", "KeyboardPanel", "PanelKeyCatcher",
        "PanelSlider", "ToggleSwitch", "SizedImage")],
    "shell/plugins/menu/Menu.qml", "shell/plugins/dock/DockWindow.qml",
    "shell/plugins/services/media/BarWidget.qml",
    "shell/plugins/panels/weather/Panel.qml", "shell/plugins/panels/wifiqr/Panel.qml",
}
ROLES = {
    "panels", "rail", "columns", "menu_arrows", "navigation", "path_crumbs",
    "sliders", "toggles", "alignment", "numbers", "dates", "keycaps",
    "nondirectional_icons", "media", "file_content", "dock", "weather", "qr",
}


class RtlPolicyTests(unittest.TestCase):
    def policy(self):
        return json.loads((ROOT / "i18n/rtl-policy.json").read_text(encoding="utf-8"))

    def test_i50_rtl_policy_source_complete(self):
        policy = self.policy()
        self.assertEqual(policy["schema"], 1)
        self.assertEqual(policy["source"]["sha"], SOURCE_SHA)
        self.assertEqual(policy["source"]["tag"], "v4.5.0-rc2")
        self.assertEqual(policy["status"], "provisional-source-contract")
        self.assertEqual(policy["native_reference"]["status"], "owed")
        self.assertEqual(policy["native_reference"]["languages"], ["ar", "he"])
        sources = policy["sources"]
        self.assertEqual({s["path"] for s in sources.values()}, SOURCE_PATHS)
        for entry in sources.values():
            self.assertRegex(entry["sha256"], r"^[0-9a-f]{64}$")
            self.assertGreaterEqual(entry["line"], 1)
            self.assertTrue(entry["excerpt"].strip())
            self.assertIn(f"/{SOURCE_SHA}/{entry['path']}#L{entry['line']}", entry["url"])
        self.assertEqual(set(policy["roles"]), ROLES)
        for role in policy["roles"].values():
            self.assertIn(role["action"], ["mirror", "keep"])
            self.assertIn(role["basis"], ["shell-source", "flea-adaptation", "shared-text-contract"])
            self.assertTrue(role["rule"].strip())
            self.assertTrue(role["sources"])
            self.assertLessEqual(set(role["sources"]), set(sources))

    def test_i50_rtl_four_codes(self):
        roster = json.loads((ROOT / "src/i18n/fixtures/locales.json").read_text())
        codes = sorted(code for code, entry in roster.items() if entry.get("direction") == "rtl")
        self.assertEqual(codes, ["ar", "fa", "he", "ur"])
        self.assertEqual(self.policy()["rtl_languages"], codes)
        self.assertEqual(self.policy()["direction_signal"], "Locale::right_to_left")

    def test_i50_rtl_exemptions(self):
        roles = self.policy()["roles"]
        for role in ["numbers", "dates", "keycaps", "nondirectional_icons", "media", "file_content", "dock", "weather", "qr"]:
            self.assertEqual(roles[role]["action"], "keep", role)
        for role in ["panels", "rail", "columns", "menu_arrows", "navigation", "path_crumbs", "sliders", "toggles", "alignment"]:
            self.assertEqual(roles[role]["action"], "mirror", role)
        self.assertEqual(self.policy()["text_runs"], {
            "names_and_paths": "FSI+unchanged-text+PDI",
            "numbers_dates_keycaps": "LRI+unchanged-text+PDI",
            "reverse_bytes": False,
            "strip_raw_display_controls": True,
            "backend_identity": "original-bytes",
        })

    @unittest.skipUnless(os.environ.get("FLEA_RTL_OMARCHY_SOURCE"), "optional read-only pinned source audit")
    def test_i50_rtl_pinned_source_bytes(self):
        checkout = Path(os.environ["FLEA_RTL_OMARCHY_SOURCE"])
        sha = subprocess.check_output(["git", "-C", str(checkout), "rev-parse", "HEAD"], text=True).strip()
        self.assertEqual(sha, SOURCE_SHA)
        for entry in self.policy()["sources"].values():
            content = subprocess.check_output(["git", "-C", str(checkout), "show", f"{sha}:{entry['path']}"])
            self.assertEqual(hashlib.sha256(content).hexdigest(), entry["sha256"])
            lines = content.decode().splitlines()
            count = len(entry["excerpt"].splitlines())
            self.assertEqual("\n".join(lines[entry["line"] - 1:entry["line"] - 1 + count]), entry["excerpt"])


class RtlHoleIsolationTests(unittest.TestCase):
    """flea-i18n-manifest refuses a placeholder outside an isolate run in an rtl_languages catalog."""

    def check(self, text, rtl=("ar",)):
        with tempfile.TemporaryDirectory(prefix="flea-rtl-holes.") as box:
            root = Path(box)
            for code, value in (("en", "Open %1 in %2"), ("ar", text), ("de", "%1 in %2 öffnen")):
                (root / code).mkdir()
                (root / code / "flea.json").write_text(json.dumps({"open": value}, ensure_ascii=False), encoding="utf-8")
            subprocess.run([sys.executable, "-B", str(ROOT / "tools/flea-i18n-manifest"), "--i18n-dir", str(root)],
                           check=True, capture_output=True)
            (root / "rtl-policy.json").write_text(json.dumps({"rtl_languages": list(rtl)}), encoding="utf-8")
            return subprocess.run([sys.executable, "-B", str(ROOT / "tools/flea-i18n-manifest"), "--check",
                                   "--i18n-dir", str(root)], capture_output=True, text=True)

    def test_i50_rtl_isolated_holes_pass(self):
        for text in ("افتح \u2068%1\u2069 في \u2068%2\u2069", "\u2066%1 / %2\u2069", "\u2068%1%2\u2069",
                     "\u2067\u2068%1\u2069\u2069 \u2068%2\u2069"):
            result = self.check(text)
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_i50_rtl_bare_hole_fails(self):
        for text, hole in (("افتح \u2068%1\u2069 في %2", "%2"), ("افتح \u2068%1 في %2", "%1"),
                           ("\u2069%1 \u2068%2\u2069", "%1"), ("\u2067%1\u2069 \u2068%2\u2069", "%1"),
                           ("\u2068%1 \u2067%2\u2069\u2069", "%2"), ("\u2067%1 \u2068%2\u2069", "%1")):
            result = self.check(text)
            self.assertEqual(result.returncode, 1)
            self.assertIn(f"ar:open: right-to-left placeholder {hole} is not isolated", result.stderr)

    def test_i50_rtl_hole_rule_only_for_rtl_languages(self):
        self.assertEqual(self.check("افتح %1 في %2", rtl=()).returncode, 0)

    def test_i50_rtl_repo_catalogs_isolate_every_hole(self):
        loader = importlib.machinery.SourceFileLoader("flea_i18n_manifest", str(ROOT / "tools/flea-i18n-manifest"))
        manifest = importlib.util.module_from_spec(importlib.util.spec_from_loader(loader.name, loader))
        loader.exec_module(manifest)
        codes = manifest.rtl_languages(ROOT / "i18n")
        self.assertEqual(codes, {"ar", "fa", "he", "ur"})
        for code in sorted(codes):
            for key, value in manifest.read(ROOT / "i18n" / code / "flea.json").items():
                for text in manifest.forms(value, f"{code}:{key}"):
                    self.assertEqual(manifest.bare_holes(text), [], f"{code}:{key}")


if __name__ == "__main__":
    unittest.main()
