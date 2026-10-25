"""Shared RTL policy and pinned Omarchy source contract."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]
SOURCE_SHA = "03ff1eff3812933f7088484533784d2f63eaf19a"
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
        self.assertEqual(policy["source"]["tag"], "v4.5.0-rc1")
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


if __name__ == "__main__":
    unittest.main()
