"""Tests for tools/flea-keymap-lint."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
LINT = ROOT / "tools/flea-keymap-lint"
KEYS = ROOT / "keys.toml"

ONE_ROW = (
    "[[preset]]\n"
    'name = "all"\n'
    'mods = "none"\n'
    'key = "F5"\n'
    'action = "reload"\n'
    'context = "listing"\n'
    'frontend = "all"\n'
)


class KeymapLintTests(unittest.TestCase):
    def run_lint(self, keys, baseline, proposed=None):
        cmd = [sys.executable, str(LINT), str(keys), "--baseline", str(baseline)]
        if proposed is not None:
            cmd += ["--proposed", proposed]
        proc = subprocess.run(cmd, capture_output=True, text=True)
        try:
            data = json.loads(proc.stdout)
        except ValueError:
            data = {}
        return proc.returncode, data, proc.stdout + proc.stderr

    def write(self, directory, name, text):
        path = Path(directory) / name
        path.write_text(text, encoding="utf-8")
        return path

    def test_repo_keys_against_itself_clean(self):
        rc, data, out = self.run_lint(KEYS, KEYS)
        self.assertEqual(rc, 0, out)
        self.assertEqual(data.get("total"), 0, out)

    def test_proposed_two_key_sequence_and_unknown_word(self):
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", ONE_ROW)
            rc, data, out = self.run_lint(keys, keys, proposed="gt")
            self.assertEqual(rc, 0, out)
            self.assertEqual(data["proposed"][0]["sequence"], "gt", out)
            rc, data, out = self.run_lint(keys, keys, proposed="pause")
            self.assertEqual(rc, 2, out)
            self.assertIn("cannot read the proposed key", out)

    def test_preset_context_must_be_string(self):
        text = ONE_ROW.replace('context = "listing"', 'context = ["listing"]')
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", text)
            rc, data, out = self.run_lint(keys, keys)
            self.assertEqual(rc, 2, out)
            self.assertIn("must be a string", out)

    def test_shared_row_action_must_be_string(self):
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", ONE_ROW + '[[ctrl]]\nkey = "D"\naction = ["pageDown"]\n')
            rc, data, out = self.run_lint(keys, keys)
            self.assertEqual(rc, 2, out)
            self.assertIn("[[ctrl]] row's action must be a string", out)
            keys = self.write(td, "keys.toml", ONE_ROW + '[[ctrl]]\nkey = "D"\naction = "pageDown"\nkeys = ["ctrl-d"]\n')
            rc, data, out = self.run_lint(keys, keys)
            self.assertEqual(rc, 2, out)
            self.assertIn("[[ctrl]] row's keys must be a string", out)

    def test_action_set_twice_fails(self):
        text = ONE_ROW.replace('action = "reload"\n', 'action = "reload"\naction = "again"\n')
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", text)
            rc, data, out = self.run_lint(keys, keys)
            self.assertEqual(rc, 2, out)
            self.assertIn("set a second time", out)

    def test_multi_line_value_set_twice_fails(self):
        text = ONE_ROW + 'context = ["a",\n  "b"]\n'
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", text)
            rc, data, out = self.run_lint(keys, keys)
            self.assertEqual(rc, 2, out)
            self.assertIn("context is set a second time", out)
            keys = self.write(td, "keys.toml", ONE_ROW + 'keys = ["a",\n  "b"]\nkeys = "x"\n')
            rc, data, out = self.run_lint(keys, keys)
            self.assertEqual(rc, 2, out)
            self.assertIn("keys is set a second time", out)

    def test_added_all_row_added_bound(self):
        existing = (
            "[[preset]]\n"
            'name = "default"\n'
            'mods = "text"\n'
            'key = "x"\n'
            'action = "first"\n'
            'context = "listing"\n'
            'frontend = "all"\n'
        )
        added = (
            "[[preset]]\n"
            'name = "all"\n'
            'mods = "text"\n'
            'key = "x"\n'
            'action = "second"\n'
            'context = "listing"\n'
            'frontend = "all"\n'
        )
        both = existing + "\n" + added
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", both)
            baseline = self.write(td, "baseline.toml", existing)
            rc, data, out = self.run_lint(keys, baseline)
            self.assertEqual(rc, 1, out)
            self.assertIn("added-bound", [f["rule"] for f in data.get("findings", [])], out)
            full = self.write(td, "full.toml", both)
            rc, data, out = self.run_lint(keys, full)
            self.assertEqual(rc, 0, out)
            self.assertEqual(data.get("total"), 0, out)

    def test_shift_t_and_capital_t_proposed_twice(self):
        with tempfile.TemporaryDirectory() as td:
            keys = self.write(td, "keys.toml", ONE_ROW)
            rc, data, out = self.run_lint(keys, keys, proposed="shift-t,T")
            self.assertEqual(rc, 1, out)
            self.assertIn("proposed-twice", [f["rule"] for f in data.get("findings", [])], out)


if __name__ == "__main__":
    unittest.main()
