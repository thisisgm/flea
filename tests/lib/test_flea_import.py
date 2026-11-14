"""Sibling tool imports work without the removed load_module API."""
import importlib.machinery
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "tools"))
from flea_import import load_source


class SourceImports(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="flea-import."))
        (self.root / ".flea-test-sandbox").touch()
        self.addCleanup(self.remove_root)
        self.name = "flea_import_fixture"
        self.addCleanup(sys.modules.pop, self.name, None)
        self.source = self.root / "source with spaces"

    def remove_root(self):
        import shutil
        root = self.root
        if root.is_absolute() and root.name.startswith("flea-import.") and (root / ".flea-test-sandbox").is_file():
            shutil.rmtree(root)

    def test_registration_dataclasses_and_reexecution_keep_identity(self):
        self.source.write_text(
            "from __future__ import annotations\n"
            "from dataclasses import dataclass\n"
            "import importlib, sys\n"
            "identity = importlib.import_module(__name__)\n"
            "executions = globals().get('executions', 0) + 1\n"
            "@dataclass\n"
            "class Node:\n"
            "    parent: Node | None = None\n")
        first = load_source(self.name, self.source)
        self.assertIs(first.identity, first)
        self.assertIs(sys.modules[self.name], first)
        self.assertIsNone(first.Node().parent)
        self.assertIs(load_source(self.name, self.source), first)
        self.assertEqual(first.executions, 2)
        self.assertEqual(first.__file__, str(self.source))
        self.assertEqual(first.__spec__.name, self.name)

    def test_failed_first_import_is_removed_but_existing_identity_survives(self):
        self.source.write_text("raise RuntimeError('fixture refused')\n")
        with self.assertRaisesRegex(RuntimeError, "fixture refused"):
            load_source(self.name, self.source)
        self.assertNotIn(self.name, sys.modules)
        self.source.write_text("value = 7\n")
        original = load_source(self.name, self.source)
        self.source.write_text("raise SystemExit('fixture exit')\n")
        with self.assertRaisesRegex(SystemExit, "fixture exit"):
            load_source(self.name, self.source)
        self.assertIs(sys.modules[self.name], original)

    def test_real_nested_and_lazy_siblings_without_load_module(self):
        def removed(*args, **kwargs):
            raise AssertionError("removed load_module API called")

        with patch.object(importlib.machinery.SourceFileLoader, "load_module", removed, create=True):
            nav = load_source("flea_field_nav", REPO / "tools/flea-field-nav")
            instant = load_source("flea_field_instant", REPO / "tools/flea-field-instant")
            census = load_source("flea_census_probe", REPO / "tools/flea-census-probe")
            self.assertIs(instant.N, nav)
            self.assertIs(census.I, instant)
            self.assertIs(census.N.S, sys.modules["flea_field_search"])
            with patch.object(os, "access", return_value=False):
                self.assertIn("not writable", instant.touchpad()[1])
            self.assertIn("flea_touchpad", sys.modules)

            class FixtureLoaded(Exception):
                pass

            def stop_before_build(name, path):
                module = load_source(name, path)
                self.assertIs(module, sys.modules["flea_nav_fixture"])
                raise FixtureLoaded

            with patch.object(instant, "LOCAL_PARENT", str(self.root.parent)), \
                    patch.object(instant, "load_source", side_effect=stop_before_build):
                (self.root / instant.MARKER).touch()
                with self.assertRaises(FixtureLoaded):
                    instant.build_fixtures(str(self.root))
            rename = load_source("flea_rename_lag", REPO / "tests/rename-lag.py")
            self.assertIs(rename.Backend, sys.modules["rename_timing"].Backend)
            self.assertTrue(callable(rename.main))


if __name__ == "__main__":
    unittest.main()
