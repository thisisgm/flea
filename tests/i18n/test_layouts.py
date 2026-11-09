"""I50-06 contracts, discovered by the controller-owned i18n-quality suite."""

import hashlib
import importlib.machinery
import importlib.util
import json
from pathlib import Path
import re
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
FIXTURES = ROOT / "tests/i18n/fixtures/xkb"
MAIN = re.compile(r"(A[A-E]\d\d|TLDE|BKSL|LSGT):")
LOADER = importlib.machinery.SourceFileLoader("flea_i18n_layouts", str(ROOT / "tools/flea-i18n-layouts"))
SPEC = importlib.util.spec_from_loader(LOADER.name, LOADER)
layouts = importlib.util.module_from_spec(SPEC)
LOADER.exec_module(layouts)


class LayoutTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.policy = json.loads((ROOT / "i18n/keyboard-policy.json").read_text())
        cls.installer = json.loads((FIXTURES / "installer.json").read_text())
        cls.events = {}
        for layout in ("us", "de", "fr", "ru", "ara", "il"):
            keymap = layouts.Xkb({"layout": layout})
            try:
                cls.events[layout] = keymap.events(0)
            finally:
                keymap.close()

    def row(self, key, mods="text", action="first", layer="all", **extra):
        return dict(id=action, table="preset", layer=layer, key=key, mods=mods, action=action,
                    completes=action, contexts=["listing"], frontend="all", prefix="", sequence_kind="single",
                    strokes=[dict(key=key, mods=mods)], **extra)

    def resolve(self, layout, key, mods="text", choice="printed-character"):
        events = self.events[layout]
        return layouts.paths(dict(key=key, mods=mods), events, self.events["us"],
                             layouts.letter_group(events), choice)[0]

    def punct(self, layout, rows):
        measured = layouts.measure(rows, self.events[layout], self.events["us"], self.policy)
        cells = {row["action"]: cell for row, cell in zip(rows, measured["cells"])}
        for cell in cells.values():
            cell["keypad"] = [[p for p in stroke if not MAIN.match(p)] for stroke in cell["paths"]]
            cell["paths"] = [[p for p in stroke if MAIN.match(p)] for stroke in cell["paths"]]
        return cells

    def valid_matrix(self, console="us"):
        scratch = ROOT / ".superpowers/m9-i50-06"
        scratch.mkdir(parents=True, exist_ok=True)
        temporary = tempfile.TemporaryDirectory(prefix="check-fixture.", dir=scratch)
        root = Path(temporary.name).resolve()
        (root / ".flea-test-sandbox").touch()

        def cleanup():
            assert root.is_absolute() and root.parent == scratch.resolve()
            assert (root / ".flea-test-sandbox").is_file()
            temporary.cleanup()

        self.addCleanup(cleanup)
        setup = root / "omarchy/install/provisioning/setup-form.sh"
        setup.parent.mkdir(parents=True)
        setup.write_text("OMARCHY_KEYBOARD_LAYOUTS=$'Fixture|" + console + "'\n")
        keys = root / "keys.toml"
        keys.write_text('[[preset]]\nname="default"\nmods="none"\nkey="F5"\n'
                        'action="reload"\ncontext="listing"\nfrontend="tui"\n'
                        '[[preset]]\nname="default"\nmods="none"\nkey="F6"\n'
                        'action="second"\ncontext="listing"\nfrontend="tui"\n')
        rows = layouts.bindings(keys)
        entry, = layouts.installer_entries(root / "omarchy", FIXTURES / "kbd-model-map")
        keymap = layouts.Xkb(entry)
        try:
            layout = dict(entry, groups=[], baseline_groups=[], keymap_sha256=keymap.sha256)
            for index in range(keymap.keymap_num_layouts(keymap.keymap)):
                events = keymap.events(index)
                group = layouts.measure(rows, events, self.events["us"], self.policy)
                group.update(index=index, name=keymap.keymap_layout_get_name(keymap.keymap, index).decode(), events=events)
                baseline = dict(layouts.measure(rows, events, self.events["us"], self.policy), index=index)
                layouts.classify_collisions(group, baseline, rows, rows)
                layout["groups"].append(group)
                layout["baseline_groups"].append(baseline)
        finally:
            keymap.close()
        matrix = dict(schema=1, policy=self.policy, bindings=rows, baseline_bindings=rows.copy(),
                      us_events=self.events["us"], errors=[], layouts=[layout],
                      installer=dict(source=layouts.identity(setup), model_map=layouts.identity(FIXTURES / "kbd-model-map"),
                                     entries=[[entry["id"], entry["installer_row"]]]),
                      provenance={name: layouts.identity(path) for name, path in
                                  (("keys", keys), ("baseline", keys), ("policy", ROOT / "i18n/keyboard-policy.json"),
                                   ("tool", ROOT / "tools/flea-i18n-layouts"), ("linter", layouts.LINTER))})
        layout["lint"] = layouts.lint_layout(matrix, layout, root / "lint", layouts.LINTER)
        matrix["summary"] = layouts.summarize(matrix)
        self.assertEqual(layouts.check_matrix(matrix, 1), [])
        return matrix

    def test_i50_layout_count_pinned(self):
        source = self.installer
        setup = FIXTURES / source["fixture_source"]
        self.assertEqual(hashlib.sha256(setup.read_bytes()).hexdigest(), source["sources"][0]["sha256"])
        self.assertEqual(source["omarchy_commit"], self.policy["source_contract"]["omarchy_commit"])
        extracted = layouts.installer_entries(FIXTURES / "omarchy", FIXTURES / "kbd-model-map")
        self.assertEqual(extracted, source["entries"])
        self.assertEqual(len({r["id"] for r in extracted}), len(extracted))
        pinned = {"03ff1eff3812933f7088484533784d2f63eaf19a": 52, "5978d93ae7da0a325b7493501911e3f1266cadd3": 52}
        self.assertEqual(len(extracted), pinned[source["omarchy_commit"]])

    def test_i50_punctuation_rule_a_printed(self):
        cell = self.punct("us", [self.row("/", action="slash")])["slash"]
        self.assertEqual((cell["paths"], cell["keypad"]), ([["AB10:0"]], [["KPDV:0", "KPDV:1"]]))
        cell = self.punct("de", [self.row("/", action="slash"), self.row("-", action="minus")])
        self.assertEqual((cell["slash"]["paths"], cell["slash"]["punctuation"]["rule"]), ([["AE07:1"]], "a"))
        self.assertEqual((cell["minus"]["paths"], cell["minus"]["punctuation"]["rule"]), ([["AB10:0"]], "a"))
        cell = self.punct("fr", [self.row("!", action="bang")])["bang"]
        self.assertEqual((cell["paths"], cell["status"], cell["punctuation"]["rule"]), ([["AB10:0"]], "reachable", "a"))

    def test_i50_punctuation_rule_b_us_position(self):
        cell = self.punct("de", [self.row("[", action="bracket")])["bracket"]
        self.assertEqual(cell["punctuation"]["alternatives"]["printed-character"], [[]])
        self.assertEqual((cell["paths"], cell["punctuation"]["rule"]), ([["AD11:0"]], "b"))
        self.assertEqual(cell["punctuation"]["rejected"], ["printed-character-needs-altgr-or-absent"])
        digit = self.row("1", action="tab1", layer="digits")
        cell = self.punct("fr", [self.row("&", action="amp"), digit])
        self.assertEqual(cell["amp"]["punctuation"]["alternatives"]["printed-character"], [["AE01:0"]])
        self.assertEqual(cell["tab1"]["paths"], [["AE01:0"]])
        self.assertEqual((cell["amp"]["paths"], cell["amp"]["punctuation"]["rule"]), ([["AE07:1"]], "b"))
        self.assertEqual(cell["amp"]["punctuation"]["rejected"], ["printed-character-conflict"])
        self.assertEqual(cell["amp"]["status"], "reachable")

    def test_i50_punctuation_rule_c_nonlatin(self):
        cell = self.punct("ru", [self.row("/", action="slash"), self.row(".", action="dot")])
        self.assertEqual(next(e for e in self.events["ru"] if e["id"] == "LSGT:0")["text"], "/")
        self.assertEqual(self.resolve("ru", "/", choice="printed-character"), ["AB10:0", "KPDV:0", "KPDV:1"])
        self.assertEqual((cell["slash"]["paths"], cell["slash"]["punctuation"]["rule"]), ([["AB10:0"]], "c"))
        self.assertEqual((cell["dot"]["paths"], cell["dot"]["punctuation"]["rule"]), ([["AB09:0"]], "c"))
        physical = next(e for e in self.events["ru"] if e["id"] == "AB09:0")
        self.assertNotEqual(physical["text"], ".")

    def test_i50_punctuation_rule_d_conflict_fails(self):
        rows = [self.row("&", action="amp"), self.row("1", action="tab1", layer="digits"),
                self.row("7", mods="shift", action="seven")]
        measured = layouts.measure(rows, self.events["fr"], self.events["us"], self.policy)
        amp = measured["cells"][0]
        self.assertEqual(amp["punctuation"]["alternatives"], {"printed-character": [["AE01:0"]],
                                                              "physical-us-position": [["AE07:1"]]})
        self.assertEqual((amp["status"], amp["punctuation"]["rule"]), ("punctuation-conflict", "d"))
        self.assertEqual({(c["a"], c["b"]) for c in measured["collisions"]}, {("amp", "seven")})
        rows = [self.row("&", action="amp"), self.row("1", action="tab1", layer="digits")]
        events = [e for e in self.events["fr"] if e["id"] != "AE07:1"]
        amp = layouts.measure(rows, events, self.events["us"], self.policy)["cells"][0]
        self.assertEqual(amp["punctuation"]["alternatives"]["physical-us-position"], [[]])
        self.assertEqual((amp["status"], amp["punctuation"]["rule"]), ("punctuation-conflict", "d"))

    def test_i50_layout_nonlatin_letter(self):
        for layout in ("ru", "ara", "il"):
            with self.subTest(layout=layout):
                self.assertEqual(layouts.letter_group(self.events[layout]), "nonlatin")
                self.assertEqual(self.resolve(layout, "j"), ["AC07:0"])
                self.assertEqual(self.resolve(layout, "J"), ["AC07:1"])
                physical = next(e for e in self.events[layout] if e["id"] == "AC07:0")
                self.assertNotEqual(physical["text"], "j")
                self.assertFalse(layouts.current_matches(dict(key="j", mods="text"), physical))

    def test_i50_layout_azerty_digit(self):
        self.assertEqual(self.resolve("fr", "1", "ctrl"), ["AE01:2"])
        self.assertEqual(self.resolve("fr", "1"), ["AE01:0"])
        self.assertEqual(self.resolve("fr", "0", "ctrlshift"), ["AE10:3"])
        event = next(e for e in self.events["fr"] if e["id"] == "AE01:0")
        self.assertEqual(event["text"], "&")
        self.assertNotIn("KP1:0", self.resolve("fr", "1"))

    def test_i50_layout_qwertz_printed_letter(self):
        self.assertEqual(layouts.letter_group(self.events["de"]), "latin")
        self.assertEqual(self.resolve("de", "z"), ["AD06:0"])
        self.assertEqual(self.resolve("de", "y"), ["AB01:0"])
        self.assertEqual(self.resolve("de", "Z", "ctrl"), ["AD06:2"])
        self.assertEqual(self.resolve("fr", "a"), ["AD01:0"])

    def test_i50_layout_altgr_punctuation(self):
        self.assertEqual(self.resolve("de", "["), [])
        self.assertEqual(self.resolve("de", "[", choice="physical-us-position"), ["AD11:0"])
        for mods in ("text", "ctrl"):
            for layout in ("us", "de", "fr", "ru"):
                for key in ("[", "]", "{", "}", "\\", "|", "@", "~", "#"):
                    for choice in self.policy["punctuation"]["order"]:
                        self.assertFalse([p for p in self.resolve(layout, key, mods, choice)
                                          if int(p.rpartition(":")[2]) & layouts.MODS["altgr"]])
        self.assertEqual(self.policy["punctuation"]["altgr"], "text-input-only")
        event = next(e for e in self.events["de"] if e["id"] == "AE08:16")
        self.assertEqual(event["text"], "[")
        self.assertTrue(event["consumed_xkb"] & event["effective"])
        self.assertTrue(event["consumed_gtk"] & event["effective"])
        dead = [e for e in self.events["de"] if e["dead"] and e["held"] == 0]
        self.assertTrue(dead)
        for event in dead:
            self.assertNotIn(event["id"], self.resolve("de", "^", choice="printed-character"))
        samples = json.loads((FIXTURES / "samples.json").read_text())
        for sample in samples["samples"]:
            observed = next(e for e in self.events[sample["layout"]] if e["id"] == sample["event"]["id"])
            self.assertEqual(observed, sample["event"])

    def test_i50_layout_sequence_collision(self):
        sequence = self.row("g", action="sequence", layer="vim")
        sequence["strokes"] *= 2
        single = self.row("g", action="immediate", layer="shared")
        rows = [sequence, single]
        measured = layouts.measure(rows, self.events["us"], self.events["us"], self.policy)
        # The vim preset row wins over shared g in the resolver, so this is shadowing, not a collision.
        self.assertEqual(measured["collisions"], [])
        layouts.classify_collisions(measured, layouts.measure([], self.events["us"], self.events["us"], self.policy), rows, [])
        self.assertEqual({(f["rule"], f["binding"], f["by"]) for f in measured["shadowing_findings"]},
                         {("added-bound", "immediate", "sequence")})
        self.assertTrue(all(f["witnesses"] == ["AC05:0"] for f in measured["shadowing_findings"]))
        def findings():
            group = layouts.measure(rows, self.events["us"], self.events["us"], self.policy)
            layouts.classify_collisions(group, layouts.measure([], self.events["us"], self.events["us"], self.policy), rows, [])
            return group["collisions"], group["shadowing_findings"]
        single["layer"] = "mac"
        self.assertEqual(findings(), ([], []))
        single["layer"], single["contexts"] = "shared", ["editor"]
        self.assertEqual(findings(), ([], []))
        single["contexts"], single["frontend"], sequence["frontend"] = ["listing"], "tui", "gui"
        self.assertEqual(findings(), ([], []))
        measured["index"] = 0
        normalized = layouts.tomllib.loads(layouts.normalized_toml([sequence], [measured]))["preset"]
        self.assertEqual({(r["frontend"], r["key"], r["mods"], r["action"], r["context"]) for r in normalized},
                         {(frontend, "g", "text", "sequence", context) for frontend in ("gui", "tui")
                          for context in ("g0:vim:listing:", "g0:vim:listing::pair:sequence")})
        canonical = layouts.bindings(ROOT / "keys.toml")
        self.assertEqual({r["layer"] for r in canonical}, {"shared", "digits", "all", "default", "vim", "mac", "windows", "yazi"})
        # Every [[tui]] row in keys.toml reaches the canonical bindings, however many reviewed rows it holds.
        tui_rows = layouts.tomllib.loads((ROOT / "keys.toml").read_text())["tui"]
        self.assertEqual(sum(r["table"] == "tui" for r in canonical), len(tui_rows))
        self.assertTrue(any(len(r["strokes"]) == 2 and r["layer"] == "yazi" for r in canonical))
        self.assertTrue(any(r["sequence_kind"] == "confirm" and len(r["strokes"]) == 2 for r in canonical))
        baseline = layouts.bindings(FIXTURES / "baseline.keys.toml")
        self.assertEqual(hashlib.sha256((FIXTURES / "baseline.keys.toml").read_bytes()).hexdigest(),
                         "4c90829496f7deffe45c2dd722d70857470412751079f4d30b98f473e3787308")
        common = [{k: v for k, v in r.items() if k != "id"} for r in canonical if r["table"] != "tui"]
        additions = [r for r in common if r["action"] == "openAgent"]
        self.assertEqual(len(additions), 1)
        self.assertEqual((additions[0]["table"], additions[0]["key"], additions[0]["mods"]),
                         ("text", "A", "text"))
        # PR #302 adds foot's Control-only text size chords; every other non-TUI row is the frozen baseline.
        text_size = [r for r in common if r["table"] == "ctrl" and r["action"].startswith("textSize")]
        self.assertEqual([(r["key"], r["action"]) for r in text_size],
                         [("Plus", "textSizeUp"), ("Equal", "textSizeUp"), ("Minus", "textSizeDown"), ("0", "textSizeReset")])
        # Carry25 binds X to carry in the GUI listing; it is the one other row the baseline predates.
        carry = [r for r in common if r["action"] == "carry"]
        self.assertEqual([(r["table"], r["key"], r["mods"]) for r in carry], [("preset", "X", "text")])
        # Stage13 adds the reviewed Ctrl+G contents-search chord to the shared listing layer.
        contents = [r for r in common if r["action"] == "searchContents"]
        self.assertEqual([(r["table"], r["key"], r["mods"], r["contexts"], r["frontend"])
                          for r in contents], [("ctrl", "G", "ctrl", ["listing", "rail"], "all")])
        # The Quick Look sheet fix binds [ and ] to turn a workbook's sheets, only inside the preview and PDF contexts.
        sheets = [r for r in common if r["action"] in ("previousSheet", "nextSheet")]
        self.assertEqual([(r["table"], r["layer"], r["key"], r["mods"], r["action"], r["contexts"], r["frontend"])
                          for r in sheets], [("preset", "all", "[", "text", "previousSheet", ["preview", "pdf"], "all"),
                                             ("preset", "all", "]", "text", "nextSheet", ["preview", "pdf"], "all")])
        # Quick Look paging widens F and B to preview and pdf and adds four document paging rows there.
        paging = [r for r in common if r["action"] in ("previewPageForward", "previewPageBack")
                  or (r["action"] in ("pageDown", "pageUp") and r["contexts"] == ["preview", "pdf"])]
        self.assertEqual([(r["table"], r["layer"], r["key"], r["mods"], r["action"], r["contexts"], r["frontend"])
                          for r in paging],
                         [("preset", "all", "F", "text", "previewPageForward", ["listing", "preview", "pdf"], "gui"),
                          ("preset", "all", "B", "text", "previewPageBack", ["listing", "preview", "pdf"], "gui"),
                          ("preset", "all", "D", "ctrl", "pageDown", ["preview", "pdf"], "gui"),
                          ("preset", "all", "U", "ctrl", "pageUp", ["preview", "pdf"], "gui"),
                          ("preset", "all", "PageDown", "none", "pageDown", ["preview", "pdf"], "gui"),
                          ("preset", "all", "PageUp", "none", "pageUp", ["preview", "pdf"], "gui")])
        # Ctrl+Right and Ctrl+Left open and close one folder in place as tree rows; > and < do so for every marked folder.
        in_place = [r for r in common if r["action"] in ("openInPlace", "closeInPlace")]
        self.assertEqual([(r["table"], r["layer"], r["key"], r["mods"], r["action"], r["contexts"], r["frontend"])
                          for r in in_place], [("preset", "all", "Right", "ctrl", "openInPlace", ["listing"], "gui"),
                                               ("preset", "all", "Left", "ctrl", "closeInPlace", ["listing"], "gui"),
                                               ("preset", "all", ">", "text", "openInPlace", ["listing"], "gui"),
                                               ("preset", "all", "<", "text", "closeInPlace", ["listing"], "gui")])
        # Copy as gains Relative path on r inside the menu (PR #296).
        relative = [r for r in common if r["action"] == "copyRelative"]
        self.assertEqual([(r["table"], r["layer"], r["key"], r["mods"], r["action"], r["contexts"], r["frontend"])
                          for r in relative], [("preset", "all", "r", "text", "copyRelative", ["menu"], "all")])
        frozen = [{k: v for k, v in r.items() if k != "id"} for r in baseline]
        widened = [r for r in frozen if r["action"] in ("previewPageForward", "previewPageBack")]
        # The terminal player draws no mute state, so m mutes only in the window's media context.
        mute = [r for r in common if r["action"] == "mute" and r["contexts"] == ["media"]]
        unmuted = [r for r in frozen if r["action"] == "mute" and r["contexts"] == ["media"]]
        self.assertEqual([(r["table"], r["layer"], r["key"], r["mods"], r["frontend"]) for r in unmuted],
                         [("preset", "all", "m", "text", "all")])
        self.assertEqual(mute, [dict(r, frontend="gui") for r in unmuted])
        self.assertEqual([(r["key"], r["contexts"]) for r in widened], [("F", ["listing"]), ("B", ["listing"])])
        self.assertEqual([r for r in common if r["action"] in ("previewPageForward", "previewPageBack")],
                         [dict(r, contexts=["listing", "preview", "pdf"]) for r in widened])
        self.assertEqual([r for r in common if r["action"] not in ("openAgent", "carry", "searchContents") and r not in text_size
                          and r not in sheets and r not in paging and r not in in_place and r not in relative
                          and r not in mute],
                         [r for r in frozen if r not in widened and r not in unmuted])

    def test_i50_layout_unreachable_fails(self):
        matrix = self.valid_matrix()
        row = self.row("NoSuchKey", mods="none")
        measured = layouts.measure([row], self.events["us"], self.events["us"], self.policy)
        self.assertEqual(measured["cells"][0]["status"], "unreachable")
        cell = matrix["layouts"][0]["groups"][0]["cells"][0]
        cell["paths"] = [[]]
        cell["status"] = "unreachable"
        matrix["summary"] = layouts.summarize(matrix)
        self.assertTrue(any("'unreachable': 1" in message for message in layouts.check_matrix(matrix, 1)))
        cell["status"] = "reachable"
        matrix["summary"] = layouts.summarize(matrix)
        self.assertTrue(any("stored findings differ" in message for message in layouts.check_matrix(matrix, 1)))

    def test_i50_current_gui_requires_policy_path(self):
        cell = self.punct("de", [self.row("[", action="bracket")])["bracket"]
        self.assertTrue(cell["current_gui_paths"][0])
        self.assertEqual(cell["paths"], [["AD11:0"]])
        self.assertEqual(cell["current_gui_status"], "unreachable")
        # The fork's US position answers a digit first, so AZERTY's & key runs tab 1.
        row = self.row("1", action="tab1", layer="digits")
        cell = layouts.measure([row], self.events["fr"], self.events["us"], self.policy)["cells"][0]
        self.assertEqual(cell["paths"], [["AE01:0"]])
        self.assertIn("AE01:0", cell["current_gui_paths"][0])
        self.assertEqual(cell["current_gui_status"], "reachable")
        # Rule b: AZERTY's printed - is tab 6, so zoomOut answers at the US - position, AE11, where ) is unbound.
        rows = [self.row("-", action="zoomOut"), self.row("6", action="tab6", layer="digits")]
        cells = self.punct("fr", rows)
        self.assertEqual((cells["zoomOut"]["status"], cells["zoomOut"]["punctuation"]["rule"]), ("reachable", "b"))
        self.assertEqual(cells["zoomOut"]["paths"], [["AE11:0"]])
        self.assertIn("AE11:0", cells["zoomOut"]["current_gui_paths"][0])
        self.assertNotIn("AE06:0", cells["zoomOut"]["current_gui_paths"][0])
        self.assertFalse(any(cells["zoomOut"]["current_gui_missing_policy_paths"]))
        # AltGr+AE11 is text input, so it never falls back to the US position.
        self.assertNotIn("AE11:16", cells["zoomOut"]["current_gui_paths"][0])
        # AZERTY's Shift+M sits at the US : key, but an ASCII letter matches only where printed.
        cell = self.punct("fr", [self.row(":", action="pathBar")])["pathBar"]
        self.assertFalse(any(cell["current_gui_missing_policy_paths"]))
        self.assertNotIn("AC10:1", cell["current_gui_paths"][0])
        row = self.row("F5", mods="none", action="reload")
        cell = layouts.measure([row], self.events["de"], self.events["us"], self.policy)["cells"][0]
        self.assertFalse(any(cell["current_gui_missing_policy_paths"]))
        self.assertTrue(any(cell["current_gui_forbidden_altgr_paths"]))
        self.assertEqual(cell["current_gui_status"], "unreachable")

    def test_i50_current_gui_altgr_counts_only_core_keycodes(self):
        # xkeyboard-config 2.48 symbols/inet types 1 on every level of <I521> (KEY_NUMERIC_1), AltGr included.
        def numeric(name, code):
            return [dict(code=code, physical=name, held=held, effective=held, consumed_xkb=0, consumed_gtk=0,
                         keysym="XF86Numeric1", text="1", symbol_text="1", dead=False, group=0, id=f"{name}:{held}")
                    for held in (0, 1, 16, 17)]
        base = [e for e in self.events["fr"] if e["physical"] not in ("I521", "I255")]
        row = self.row("1", action="tab1", layer="digits")
        cell = layouts.measure([row], base + numeric("I521", 521), self.events["us"], self.policy)["cells"][0]
        self.assertIn("I521:16", cell["current_gui_paths"][0])
        self.assertFalse(any(cell["current_gui_forbidden_altgr_paths"]))
        self.assertEqual(cell["current_gui_status"], "reachable")
        # The same key at a core keycode is still an AltGr chord that dispatches.
        cell = layouts.measure([row], base + numeric("I255", 255), self.events["us"], self.policy)["cells"][0]
        self.assertEqual(cell["current_gui_forbidden_altgr_paths"], [["I255:16", "I255:17"]])
        self.assertEqual(cell["current_gui_status"], "unreachable")

    def test_i50_layout_canonical_inventory_required(self):
        matrix = self.valid_matrix()
        matrix["bindings"].pop()
        for group in matrix["layouts"][0]["groups"]:
            group["cells"].pop()
        matrix["summary"] = layouts.summarize(matrix)
        self.assertTrue(any("canonical inventory differs" in message for message in layouts.check_matrix(matrix, 1)))

    def test_i50_layout_scope_checks_fail(self):
        matrix = self.valid_matrix()
        layout = matrix["layouts"][0]
        group, baseline = layout["groups"][0], layout["baseline_groups"][0]
        matrix["provenance"]["resolvers"] = [dict(layouts.identity(ROOT / "src/keys.rs"), sha256="0" * 64)]
        group["preservation_findings"], group["shadowing_findings"], baseline["script"] = [{}], [{}], "nonlatin"
        group["variants"][0]["current_gui_status"] = "unreachable"
        group["scopes"].append(dict(frontend="gui", preset="default", context="listing", state="", shadowed=[], cells=[0]))
        messages = layouts.check_matrix(matrix, 1)
        for expected in ("Stale resolver identity", "stored baseline differs", "1 baseline actions shadowed", "1 unapproved overrides",
                         "legacy Python GUI model has unreachable rows", "normalized candidate differs from effective scopes"):
            self.assertTrue(any(expected in message for message in messages), (expected, messages))
        matrix = self.valid_matrix()
        del matrix["layouts"][0]["groups"][0]["scopes"]
        self.assertTrue(any("no effective scopes" in message for message in layouts.check_matrix(matrix, 1)))

    def test_i50_layout_all_active_groups_required(self):
        matrix = self.valid_matrix("ru")
        self.assertEqual(len(matrix["layouts"][0]["groups"]), 2)
        matrix["layouts"][0]["groups"].pop()
        matrix["summary"] = layouts.summarize(matrix)
        self.assertTrue(any("active group inventory differs" in message for message in layouts.check_matrix(matrix, 1)))

    def test_i50_layout_latin_physical_terminal_limit(self):
        row = self.row("1", action="tab1", layer="digits")
        cell = layouts.measure([row], self.events["fr"], self.events["us"], self.policy)["cells"][0]
        self.assertIn("legacy-no-physical-key-identity", cell["terminal_limits"])

    def test_i50_layout_unknown_keymap_fails(self):
        with self.assertRaisesRegex(ValueError, "Unknown or invalid XKB keymap"):
            layouts.Xkb({"layout": "flea_unknown_layout_50"})
        keymap = layouts.Xkb({"layout": "ru,us", "options": "grp:shifts_toggle"})
        try:
            self.assertEqual(keymap.keymap_num_layouts(keymap.keymap), 2)
            self.assertEqual(layouts.letter_group(keymap.events(0)), "nonlatin")
            self.assertEqual(layouts.letter_group(keymap.events(1)), "latin")
        finally:
            keymap.close()


if __name__ == "__main__":
    unittest.main()
