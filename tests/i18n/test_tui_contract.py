"""The terminal UI consumes the shared catalogs, direction policy and keycap snapshot."""
import json
from pathlib import Path
import re
import runpy
import unittest

ROOT = Path(__file__).resolve().parents[2]
TUI = ROOT / "src/tui"
LANGUAGES = 50  # English plus the 49 translated catalogs.
# Floors well under today's counts, so a scan that silently sees nothing fails.
MIN_LIVE_FILES, MIN_SPEAKING_FILES, MIN_SHEET_ROWS, MIN_TUI_KEYS = 50, 50, 60, 400
EXTRACT = runpy.run_path(str(ROOT / "tools/flea-i18n-extract"))
# Sample input: let q = '"'; // "note" and r#"raw"# are masked so code searches skip them.
TOKEN = re.compile(r"""(?P<char>b?'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]+\}|.)|[^'\\\n])')"""
                   r"""|(?P<comment>//[^\n]*|/\*.*?\*/)|(?P<raw>r(?P<marks>\#*)".*?"(?P=marks))|(?P<text>"(?:\\.|[^"\\])*")""", re.S)


def blank(text, start, end):
    return text[:start] + re.sub(r"[^\n]", " ", text[start:end]) + text[end:]


def masked(text, strings):
    """Blank comments, and string and char bodies too when strings is set."""
    out = text
    for match in TOKEN.finditer(text):
        if match.group("comment") or strings:
            out = blank(out, match.start(), match.end())
    return out


ITEM_KEYWORDS = {"fn", "impl", "mod", "struct", "enum", "static", "use", "type", "trait", "let", "macro_rules", "const"}


def spans_under_test_attribute(code):
    """Spans of items under #[cfg(test)] or #[cfg(all(test, ..))], found on code with strings already blanked."""
    spans = []
    for attribute in re.finditer(r"#\[cfg\((?:all\()?test\b[^\]]*\]", code):
        # Sample input: #[cfg(test)] const P: [&str; 3] = [..]; or #[cfg(test)] probe: u8, inside a struct.
        head = re.match(r"(?:\s|#\[[^\]]*\])*(?:pub(?:\([^)]*\))?\s+)?(?:(?:async|unsafe|extern)\s+)*(\w+)", code[attribute.end():])
        # Only a field, variant or match arm ends at a comma; an item's generics hold depth-0 commas.
        item = head is not None and head.group(1) in ITEM_KEYWORDS
        depth, index = 0, attribute.end()
        while index < len(code):
            char = code[index]
            if char in ")]}" and depth == 0:
                break
            index += 1
            if char in "([{":
                depth += 1
            elif char in ")]}":
                depth -= 1
                if depth == 0 and char == "}":
                    break
            elif depth == 0 and (char == ";" or char == "," and not item):
                break
        spans.append((attribute.start(), index))
    return spans


# Sample input: block(code, code.index("while !model.quit")) on "while !model.quit { .. }" returns that loop whole.
def block(code, start):
    """The text from start through the brace that closes the first block opened after it."""
    depth, index = 0, code.index("{", start)
    while True:
        depth += {"{": 1, "}": -1}.get(code[index], 0)
        index += 1
        if depth == 0:
            return code[start:index]


def literals(text):
    """Each string literal body in Rust source, lexed with TOKEN so char literals and raw strings stay paired."""
    for match in TOKEN.finditer(text):
        if match.group("text") or match.group("raw"):
            token = match.group()
            yield match.start(), match.end(), token[token.index('"') + 1:token.rindex('"')]


def live_sources():
    """Every shipped TUI source as (relative path, code without strings, text with strings), test items blanked."""
    sources = []
    for path in sorted(TUI.rglob("*.rs")):
        relative = path.relative_to(ROOT).as_posix()
        # The extractor's own test-file rule, so the two never disagree on what ships.
        if "test" in path.stem.split("_") or path.stem.endswith("tests") or "/fixtures/" in relative:
            continue
        text = path.read_text(encoding="utf-8")
        code, with_strings = masked(text, True), masked(text, False)
        for start, end in spans_under_test_attribute(code):
            code, with_strings = blank(code, start, end), blank(with_strings, start, end)
        sources.append((relative, code, with_strings))
    return sources


def catalogs():
    return {path.parent.name: json.loads(path.read_text(encoding="utf-8"))
            for path in sorted((ROOT / "i18n").glob("*/flea.json"))}


# Sample input: ("move", &["cursorDown"], "keys.cursorDown", &["j", "down"]), one row of key_sheet.rs ROWS.
def key_sheet_rows():
    text = masked((TUI / "key_sheet.rs").read_text(encoding="utf-8"), False)
    body = text[text.index("const ROWS: &[Row] = &["):]
    body = body[:body.index("\n];")]
    return [(group, re.findall(r'"([^"]+)"', actions), key)
            for group, actions, key in re.findall(r'\(\s*"([^"]+)",\s*&\[([^\]]*)\],\s*"([^"]+)"', body)]


class TuiContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.sources = live_sources()
        cls.catalogs = catalogs()
        cls.english = cls.catalogs["en"]
        cls.callers, _, cls.missing, cls.dynamic, cls.scanned = EXTRACT["scan"](ROOT, cls.english)

    def tui_keys(self):
        return {key for key, sites in self.callers.items() if any(site["surface"] == "tui" for site in sites)}

    def test_i50_tui_source_masks(self):
        sample = 'let q = \'"\'; let r = r#"a "b" c"#; let s = "Open it now";\n'
        self.assertEqual([body for _, _, body in literals(sample)], ['a "b" c', "Open it now"])
        self.assertNotIn("Open", masked(sample + "// Open\n", True))
        live = "struct M {\n    #[cfg(test)]\n    probe: u8,\n}\nfn f() { I18n::load(p) }\n"
        code = masked(live, True)
        for start, end in spans_under_test_attribute(code):
            code = blank(code, start, end)
        self.assertIn("I18n::load(", code)
        self.assertNotIn("probe", code)
        for test_only in ("#[cfg(test)]\nconst P: [&str; 3] = [\"a\", \"b\", \"c\"];\n",
                          "#[cfg(test)]\nfn fixture() -> Result<A, B> { Catalog::parse(x) }\n",
                          "#[cfg(test)]\nconst R: Result<u8, u8> = Ok(1);\n",
                          "#[cfg(test)]\nconst B: bool = A < C;\n"):
            code = masked(test_only + "fn g() {}\n", True)
            for start, end in spans_under_test_attribute(code):
                code = blank(code, start, end)
            self.assertEqual(code.split(), ["fn", "g()", "{}"])
        for live in ("enum E {\n    #[cfg(test)]\n    Probe\n}\nfn f() { I18n::load(p) }\n",
                     "match k {\n    #[cfg(test)]\n    K::Probe => 1,\n    K::Live => I18n::load(p),\n}\n",
                     "S {\n    #[cfg(test)]\n    verbose: Probe > 2,\n    live: I18n::load(p),\n}\n"):
            code = masked(live, True)
            for start, end in spans_under_test_attribute(code):
                code = blank(code, start, end)
            self.assertIn("I18n::load(", code)
            self.assertNotIn("Probe", code)

    def test_i50_tui_no_second_catalog_loader(self):
        self.assertGreater(len(self.sources), MIN_LIVE_FILES)
        frame_catalogs = []
        for path, code, text in self.sources:
            for banned in ("I18n::load(", "i18n::Paths", "i18n::catalog", "i18n::document", "Catalog::"):
                self.assertNotIn(banned, code, f"{path} builds its own catalog with {banned}")
            self.assertNotRegex(text, r"flea\.json|i18n/[a-z]{2}(?:-[A-Z]{2})?/", f"{path} names a catalog file")
            for include in re.findall(r'include_(?:str|bytes)!\(\s*"([^"]*)"', text):
                if "i18n/" in include:
                    self.assertEqual(include, "../../i18n/rtl-policy.json", path)
            frame_catalogs += [path] * code.count("I18n::default()")
        # model.rs builds the one frame catalog; words.rs only renders a held Text for Display.
        self.assertEqual(sorted(frame_catalogs), ["src/tui/model.rs", "src/tui/words.rs"])
        model = dict((path, code) for path, code, _ in self.sources)["src/tui/model.rs"]
        self.assertRegex(model, r"#\[cfg\(not\(test\)\)\]\s*let i18n = crate::i18n::I18n::default\(\);")
        # The run loop follows a language change through the frame catalog, with no restart.
        run = dict((path, code) for path, code, _ in self.sources)["src/tui/mod.rs"]
        loop = block(run, run.index("while !model.quit"))
        self.assertIn("model.refresh_frame_locale()", loop)
        words = dict((path, code) for path, code, _ in self.sources)["src/tui/words.rs"]
        self.assertIn("self.i18n.poll_changed()", words)

    def test_i50_tui_shared_keycap_api(self):
        code = dict((path, code) for path, code, _ in self.sources)
        text = dict((path, text) for path, _, text in self.sources)
        self.assertIn("flea::keys::display::{KeycapError, KeycapSnapshot}", code["src/tui/keycaps.rs"])
        self.assertRegex(code["src/tui/keycaps.rs"], r"KeycapSnapshot::(?:new|from_group_name)\(")
        # I50-11-AMBIG resolves each chord part on its own, through the legend half of the shared API.
        self.assertIn("flea::keys::display::chord_legend(", code["src/tui/words.rs"])
        self.assertIn("snapshot.legend(", code["src/tui/key_sheet.rs"])
        run = code["src/tui/mod.rs"]
        self.assertIn("keycaps.apply(&mut model, &map);", block(run, run.index("while !model.quit")))
        setter = block(code["src/tui/words.rs"], code["src/tui/words.rs"].index("pub fn set_keycaps("))
        # Sample input: self.keycap_labels = labels; or m.keycap_labels.insert(k, v) writes; m.keycap_labels.get(k) reads.
        writes = re.compile(r"\.keycap_labels\s*(?:=(?!=)|\.(?:insert|extend|clear|remove|retain|drain|entry|get_mut|iter_mut|values_mut)\b)")
        for path, body in code.items():
            self.assertNotRegex(body, r"\b(?:KEYCAP_DEFAULTS|KEYCAP_LAYOUTS|KeycapLayout)\b", f"{path} copies the keycap table")
            self.assertNotRegex(text[path], r'"(?:dvorak|colemak|azerty|qwertz)"', f"{path} names a layout of its own")
            outside = body.replace(setter, "") if path == "src/tui/words.rs" else body
            self.assertNotRegex(outside, writes, f"{path} writes key labels outside set_keycaps")
        self.assertRegex(setter, writes)
        rows = key_sheet_rows()
        self.assertGreater(len(rows), MIN_SHEET_ROWS)
        # Where the TUI row and the GUI sheet use one English term, every language must translate it once.
        drift = []
        for _, names, key in rows:
            for action in names:
                shared = f"keys.action.{action}"
                if self.english.get(shared) != self.english.get(key):
                    continue
                drift += [f"{code_}: {key} {catalog.get(key)!r} != {shared} {catalog.get(shared)!r}"
                          for code_, catalog in self.catalogs.items() if catalog.get(key) != catalog.get(shared)]
        self.assertEqual(drift, [])

    def test_i50_tui_english_delta_joined(self):
        keys = self.tui_keys()
        self.assertGreater(len(keys), MIN_TUI_KEYS)
        index = json.loads((ROOT / "i18n/source-index.json").read_text(encoding="utf-8"))
        indexed = {key for key, entry in index["keys"].items() if any(site["surface"] == "tui" for site in entry["call_sites"])}
        self.assertEqual(sorted(keys - indexed), [], "TUI keys absent from the joined source index")
        self.assertEqual(sorted(key for key in keys if key not in self.english), [])
        self.assertEqual(len(self.catalogs), LANGUAGES)
        gaps = [f"{code}: {key}" for code, catalog in self.catalogs.items()
                for key in sorted(keys) if not catalog.get(key)]
        self.assertEqual(gaps, [], "TUI keys that would fall back to English")
        unknown = [site for site in self.missing if site["surface"] == "tui" and site["reason"] == "key-not-in-English"]
        self.assertEqual(unknown, [])

    def test_i50_tui_source_coverage(self):
        live = {path for path, _, _ in self.sources}
        # The extractor audits every shipped TUI file, so no key or literal escapes the source index.
        self.assertEqual(sorted(live - set(self.scanned)), [], "shipped TUI files the extractor skips")
        callers = {site["path"] for sites in self.callers.values() for site in sites if site["surface"] == "tui"}
        speaking = {path for path, _, text in self.sources
                    if re.search(r'\b(?:tr|tr_args|trn|trn_args|say_key|fail_key)\(\s*"|Text::(?:Key|Args|Plural)\(\s*"', text)}
        self.assertEqual(sorted(speaking - callers), [], "files that name a key the scan cannot see")
        self.assertGreater(len(live & callers), MIN_SPEAKING_FILES)
        # A built key name must resolve to catalog keys, or no language could ever translate it.
        unresolved = [f"{site['path']}:{site['line']} {site['pattern']}" for site in self.dynamic
                      if site["surface"] == "tui" and site["pattern"] and site["path"] in live
                      and not site["possible_keys"] and re.match(r"[a-z]+\.", site["pattern"])]
        self.assertEqual(unresolved, [])
        # A shipped English sentence in a live path must travel with its catalog key.
        sentences = {}
        for key, value in self.english.items():
            if isinstance(value, str) and re.search(r"[A-Za-z]{2,} [A-Za-z]{2,} [A-Za-z]{2,}", value):
                sentences.setdefault(value, set()).add(key)
        loose = []
        for path, _, text in self.sources:
            # Sample input: pub const UNSHOWN: &str = "preview.unshown"; a key held in a constant.
            constants = dict(re.findall(r'\bconst ([A-Z_]+): &str = "([^"]+)";', text))
            for start, end, body in literals(text):
                keys = sentences.get(body)
                if keys is None:
                    continue
                # The key may sit in the statement before, as in failure = Key(..); error = "legacy".
                prior = text.rfind(";", 0, start)
                before = text.rfind(";", 0, prior) if prior > 0 else -1
                statement = text[before + 1:text.find(";", end)]
                named = {constants.get(name) for name in re.findall(r"\b([A-Z_]+)\b", statement)}
                if not any(f'"{key}"' in statement or key in named for key in keys):
                    loose.append(f"{path}:{text.count(chr(10), 0, start) + 1} {body!r}")
        self.assertEqual(loose, [], "English sentences drawn without a catalog key")

    def test_i50_tui_shared_direction(self):
        policy = json.loads((ROOT / "i18n/rtl-policy.json").read_text(encoding="utf-8"))
        code = dict((path, code) for path, code, _ in self.sources)
        text = dict((path, text) for path, _, text in self.sources)
        roles = {role for body in text.values() for role in re.findall(r'mirrors\("([a-z_]+)"\)', body)}
        self.assertTrue(roles)
        self.assertLessEqual(roles, set(policy["roles"]))
        self.assertTrue(all(policy["roles"][role]["action"] == "mirror" for role in roles))
        self.assertIn("m.i18n.locale().right_to_left", code["src/tui/layout.rs"])
        for path, body in text.items():
            self.assertNotRegex(body, r'"(?:ar|he|fa|ur)"', f"{path} keeps its own right-to-left language list")
        # Isolates come from the policy; only cells.rs spells one, and its pushed opening must be names_and_paths.
        controls = {"FSI": "\\u{2068}", "LRI": "\\u{2066}", "RLI": "\\u{2067}"}
        opening = controls[policy["text_runs"]["names_and_paths"].split("+")[0]]
        isolate = re.compile(r"\\u\{(?:206[6-9]|202[a-e])\}|[\u2066-\u2069\u202a-\u202e]", re.I)
        for path, body in text.items():
            if path != "src/tui/cells.rs":
                self.assertNotRegex(body, isolate, f"{path} spells its own isolate")
        pushed = re.findall(r"push\('(\\u\{206[678]\})'\)", text["src/tui/cells.rs"])
        self.assertEqual(set(pushed), {opening})
        self.assertTrue(policy["text_runs"]["strip_raw_display_controls"])
        self.assertIn("'\\u{2066}'..='\\u{206f}'", text["src/tui/cells.rs"])


if __name__ == "__main__":
    unittest.main()
