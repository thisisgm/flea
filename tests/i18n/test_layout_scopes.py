"""Effective resolver scopes must precede layout policy selection."""

import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LOADER = importlib.machinery.SourceFileLoader("layout_scopes", str(ROOT / "tools/flea-i18n-layouts"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
layouts = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(layouts)


class LayoutScopeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = json.loads((ROOT / "i18n/keyboard-policy.json").read_text())
        cls.rows = layouts.bindings(ROOT / "keys.toml")
        cls.events = {}
        for name, entry in {"us": {"layout": "us"}, "dvorak": {"layout": "us", "variant": "dvorak"},
                            "jp": {"layout": "jp"}, "fr": {"layout": "fr"}, "be": {"layout": "be"}}.items():
            keymap = layouts.Xkb(entry)
            try:
                cls.events[name] = keymap.events(0)
            finally:
                keymap.close()
        cls.scratch = (ROOT / ".superpowers/m9-i50-keypolicy-scope").resolve()
        cls.scratch.mkdir(parents=True, exist_ok=True)

    def row(self, key, action, layer="shared", frontend="all", context="listing", mods="text", table=None):
        return dict(id=action, table=table or ("text" if layer == "shared" else "preset"),
                    layer=layer, key=key, mods=mods, action=action, completes=action,
                    contexts=[context], frontend=frontend, prefix="", sequence_kind="single",
                    strokes=[dict(key=key, mods=mods)])

    def measure(self, name, rows=None):
        return layouts.measure(self.rows if rows is None else rows, self.events[name], self.events["us"], self.policy)

    def cells(self, group, frontend, preset, context="listing", state=""):
        if "scopes" in group:
            return layouts.scope_cells(group, frontend, preset, context, state)
        return group["cells"]

    def selected(self, group, row, frontend="gui", preset="default", context="listing", state=""):
        matches = [c for c in self.cells(group, frontend, preset, context, state) if c["binding"] == row["id"]]
        self.assertEqual(len(matches), 1, (row["id"], frontend, preset, context, state))
        return matches[0]

    def real_row(self, **fields):
        return next(r for r in self.rows if all(r[k] == v for k, v in fields.items()))

    def lint(self, rows, baseline):
        temporary = tempfile.TemporaryDirectory(prefix="lint.", dir=self.scratch)
        root = Path(temporary.name).resolve()
        (root / ".flea-test-sandbox").touch()

        def cleanup():
            assert root.is_absolute() and root.parent == self.scratch
            assert (root / ".flea-test-sandbox").is_file()
            temporary.cleanup()

        self.addCleanup(cleanup)
        current, old = self.measure("us", rows), self.measure("us", baseline)
        current["index"], old["index"] = 0, 0
        layouts.classify_collisions(current, old, rows, baseline)
        layout = dict(id="fixture", groups=[current], baseline_groups=[old])
        matrix = dict(bindings=rows, baseline_bindings=baseline)
        receipt = layouts.lint_layout(matrix, layout, root, layouts.LINTER)["policy"]
        return receipt, json.loads(Path(receipt["result"]).read_text())

    def test_i50_scope_dvorak_gui_default_printed(self):
        full = self.measure("dvorak")
        reduced = self.measure("dvorak", [r for r in self.rows if r["table"] != "tui"])
        for key, action, forbidden in [("/", "filter", "AB10:0"), (",", "settings", "AB08:0")]:
            row = self.real_row(table="text", key=key)
            for context in ("listing", "rail"):
                with self.subTest(key=key, context=context):
                    cell = self.selected(full, row, context=context)
                    self.assertEqual(row["action"], action)
                    self.assertEqual(cell["punctuation"]["rule"], "a")
                    self.assertTrue(cell["paths"][0])
                    self.assertNotIn(forbidden, cell["paths"][0])
                    self.assertEqual(cell["paths"], self.selected(reduced, row, context=context)["paths"])
                    self.assertEqual({e["text"] for e in self.events["dvorak"] if e["id"] in cell["paths"][0]}, {key})
        for group in (full, reduced):
            group["index"] = 0
        rows = [r for r in self.rows if r["table"] != "tui"]
        us = self.measure("us", rows)
        us["index"] = 0
        self.assertEqual(layouts.normalized_toml(rows, [reduced]), layouts.normalized_toml(rows, [us]))

    def test_i50_scope_yazi_prefix_override(self):
        settings = self.real_row(table="text", key=",")
        arm = self.real_row(table="tui", layer="yazi", action="prefix:,")
        suffix = self.real_row(table="tui", layer="yazi", prefix=",", key="a")
        for name in ("us", "dvorak", "fr", "be"):
            with self.subTest(layout=name):
                group = self.measure(name)
                normal = self.cells(group, "tui", "yazi")
                self.assertIn(arm["id"], {c["binding"] for c in normal})
                self.assertNotIn(settings["id"], {c["binding"] for c in normal})
                self.assertNotIn(suffix["id"], {c["binding"] for c in normal})
                self.assertEqual(suffix["action"], "sortName")
                self.assertTrue(self.selected(group, suffix, "tui", "yazi", state="prefix:,")["paths"][-1])
                self.assertTrue(self.selected(group, settings)["paths"][0])
                self.assertEqual(group["collisions"], [])
                group["index"] = 0
                normalized = layouts.normalized_toml(self.rows, [group])
                self.assertIn("prefix:,".encode().hex(), normalized)
                contexts = [r["context"] for r in layouts.tomllib.loads(normalized)["preset"]]
                self.assertTrue(all("," not in c for c in contexts))
                if name in ("fr", "be"):
                    semicolon = self.real_row(table="tui", layer="yazi", key=";", prefix="")
                    self.assertEqual(self.selected(group, semicolon, "tui", "yazi")["punctuation"]["rule"], "a")

    def test_i50_scope_japanese_tilde_printed(self):
        row = self.real_row(table="tui", layer="yazi", key="~", prefix="")
        for rows in ([r for r in self.rows if r["table"] == "tui"], self.rows):
            with self.subTest(inherited=len(rows)):
                cell = self.selected(self.measure("jp", rows), row, "tui", "yazi")
                self.assertEqual(cell["punctuation"]["rule"], "a")
                self.assertTrue({"AE10:1", "AE12:1"} & set(cell["paths"][0]))
                self.assertFalse(any(int(p.rpartition(":")[2]) & layouts.MODS["altgr"] for p in cell["paths"][0]))

    def test_i50_scope_true_collision_still_fails(self):
        rows = [self.row("/", "first"), self.row("/", "second")]
        group = self.measure("us", rows)
        self.assertTrue(group["collisions"])
        conflicts = self.cells(group, "gui", "default")
        self.assertTrue(conflicts)
        self.assertTrue(all(c["status"] == "punctuation-conflict" and c["punctuation"]["rule"] == "d" for c in conflicts))
        receipt, report = self.lint(rows, [rows[0]])
        self.assertEqual(receipt["exit_code"], 1)
        self.assertGreater(report["counts"].get("added-bound", 0), 0)
        self.assertGreater(report["counts"].get("layout-collision", 0), 0)
        sequence, single = self.row("g", "sequence", "vim", table="preset"), self.row("g", "immediate", "vim", table="preset")
        sequence["strokes"] *= 2
        group = self.measure("us", [sequence, single])
        self.assertEqual({(c["kind"], tuple(c["witnesses"]), c["scope"]["preset"]) for c in group["collisions"]},
                         {("sequence-prefix", ("AC05:0",), "vim")})
        single["layer"] = "shared"
        group = self.measure("us", [sequence, single])
        layouts.classify_collisions(group, self.measure("us", []), [sequence, single], [])
        self.assertEqual(group["collisions"], [])
        self.assertEqual({(f["rule"], f["binding"], f["by"]) for f in group["shadowing_findings"]},
                         {("added-bound", "immediate", "sequence")})
        rows = [self.row("&", "amp", layer="all"), self.row("1", "digit", layer="digits"),
                self.row("7", "seven", layer="all", mods="shift")]
        group = self.measure("fr", rows)
        self.assertEqual(self.selected(group, rows[0])["punctuation"]["rule"], "d")

    def test_i50_scope_partial_shadow_is_not_a_collision(self):
        plus, keypad = self.row("+", "plus"), self.row("KP_Add", "keypadAdd", "vim", mods="none")
        group = self.measure("us", [plus, keypad])
        layouts.classify_collisions(group, self.measure("us", []), [plus, keypad], [])
        self.assertEqual(group["collisions"], [])
        cell = self.selected(group, plus, preset="vim")
        self.assertEqual((cell["status"], cell["punctuation"]["rule"]), ("reachable", "a"))
        self.assertIn("AE12:1", cell["paths"][0])
        self.assertEqual({(f["binding"], f["by"], tuple(f["witnesses"])) for f in group["shadowing_findings"]},
                         {("plus", "keypadAdd", ("KPAD:0",))})
        keypad["layer"], keypad["table"] = "shared", "text"
        group = self.measure("us", [plus, keypad])
        self.assertEqual({(c["kind"], tuple(c["witnesses"])) for c in group["collisions"]}, {("chord", ("KPAD:0",))})
        star, backslash = self.row("*", "star"), self.row("\\", "backslash", "all")
        cell = self.selected(self.measure("fr", [star, backslash]), star)
        self.assertEqual((cell["punctuation"]["rule"], cell["punctuation"]["rejected"]), ("b", ["printed-character-conflict"]))

    def test_i50_scope_refusal_rows_bind_no_command(self):
        for preset, frontend, context, table in [("yazi", "tui", "listing", "tui"), ("vim", "all", "listing", "preset")]:
            with self.subTest(preset=preset):
                refusal = self.row("Return", "unsupported:Unavailable here", preset, frontend, context, "shift", table)
                anchor = self.row("x", "old")
                receipt, report = self.lint([anchor, refusal], [anchor])
                self.assertEqual(receipt["exit_code"], 0, report)
                group = dict(self.measure("us", [refusal]), index=0)
                self.assertNotIn("unsupported:", layouts.normalized_toml([refusal], [group]))
                self.assertEqual({r["binding"] for r in report["refusals"]}, {refusal["id"]})
                self.assertTrue(all(r["paths"] == [["RTRN:1"]] for r in report["refusals"]))
                command = dict(refusal, id="openWith", action="openWith", completes="openWith")
                receipt, report = self.lint([anchor, command], [anchor])
                self.assertEqual(receipt["exit_code"], 1)
                self.assertEqual(report["counts"].get("modifier-special"), 1 if frontend == "tui" else 2)
        real = self.real_row(table="tui", layer="yazi", key="Return", mods="shift")
        group = self.measure("us")
        group["index"] = 0
        self.assertTrue(real["action"].startswith("unsupported:"))
        self.assertEqual(self.selected(group, real, "tui", "yazi")["status"], "reachable")
        self.assertNotIn(real["action"], layouts.normalized_toml(self.rows, [group]))

    def test_i50_scope_baseline_action_preserved(self):
        old = self.row(",", "settings")
        for layer, context, allowed in [("default", "listing", False), ("yazi", "listing", True),
                                        ("yazi", "menu", False)]:
            with self.subTest(layer=layer, context=context):
                baseline = [dict(old, contexts=[context])]
                if context == "menu":
                    baseline[0].update(table="preset", layer="all")
                added = self.row(",", "replacement", layer, "tui", context, table="tui")
                receipt, report = self.lint(baseline + [added], baseline)
                self.assertEqual(receipt["exit_code"], 0 if allowed else 1, report)
                if not allowed:
                    self.assertTrue(report["findings"])
        old = self.row("x", "old", layer="default")
        added = self.row("x", "new")
        receipt, report = self.lint([old, added], [old])
        self.assertEqual(receipt["exit_code"], 1)
        self.assertGreater(report["counts"].get("added-bound", 0), 0)
        self.assertFalse(any(f["rule"] == "baseline-action-shadowed" for f in report["findings"]))
        alias = dict(self.row("x", "old"), id="oldAlias")
        receipt, report = self.lint([old, alias], [old])
        self.assertEqual(receipt["exit_code"], 0, report)
        self.assertEqual(hashlib.sha256((ROOT / "i18n/keyboard-policy.json").read_bytes()).hexdigest(),
                         "0b9d2f13d1447eda82798c6bf5c3b1c1782962f5b47ef7c591736e1735520f55")


if __name__ == "__main__":
    unittest.main()
