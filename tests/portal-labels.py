#!/usr/bin/env python3
"""Exercise caller action labels through the portal's shared request boundary."""
import runpy
from pathlib import Path
import unittest


ROOT = Path(__file__).resolve().parents[1]
request_for = runpy.run_path(str(ROOT / "tools/flea-portal"))["request_for"]
CASES = (
    ("_Open", "Open"),
    ("_OK", "OK"),
    ("Save _As", "Save As"),
    ("Under__score", "Under_score"),
    ("雪_を開く", "雪を開く"),
    ("_Écrire e\u0301", "Écrire e\u0301"),
    ("", ""),
    ("literal_", "literal_"),
    ("_", "_"),
    ("__", "_"),
    ("___", "__"),
    ("____", "__"),
    ("___Open____", "_Open__"),
    ("line_\nbreak", "line\nbreak"),
    ("Keep %1 words", "Keep %1 words"),
    ("_Save", "Save"),
)


class PortalLabels(unittest.TestCase):
    def test_all_methods_decode_mnemonics_once(self):
        for method, mode in (("OpenFile", "open"), ("SaveFile", "save"),
                             ("SaveFiles", "savefiles")):
            for before, after in CASES:
                with self.subTest(method=method, label=before):
                    options = {"accept_label": before}
                    request = request_for(method, "_Title__", "org.example._App", options)
                    self.assertEqual(request["accept"], after)
                    self.assertEqual(request["mode"], mode)
                    self.assertEqual(options["accept_label"], before)

    def test_missing_or_nontext_label_keeps_picker_fallback(self):
        for method in ("OpenFile", "SaveFile", "SaveFiles"):
            for options in ({}, {"accept_label": None}, {"accept_label": False},
                            {"accept_label": 42}, {"accept_label": ["_Open"]}):
                with self.subTest(method=method, options=options):
                    self.assertEqual(request_for(method, "", "", options)["accept"], "")

    def test_only_action_label_changes(self):
        options = {
            "accept_label": "_Open", "current_folder": b"/tmp/_folder__\0",
            "current_file": b"/tmp/_folder__/_file__\0", "current_name": "_name__",
            "files": [b"_one__\0", b"_two__\0"],
            "filters": [("_Files__", [(0, "*_pattern__"), (1, "text/_plain__")])],
            "current_filter": ("_Files__", [(0, "*_pattern__"), (1, "text/_plain__")]),
        }
        for method in ("OpenFile", "SaveFile", "SaveFiles"):
            with self.subTest(method=method):
                request = request_for(method, "_Title__", "org.example._App", options)
                self.assertEqual(request["title"], "_Title__")
                self.assertEqual(request["app"], "org.example._App")
                self.assertEqual(request["folder"], "/tmp/_folder__")
                self.assertEqual(request["file"], "/tmp/_folder__/_file__")
                self.assertEqual(request["name"], "_file__" if method == "SaveFile" else "_name__")
                self.assertEqual(request["files"], ["_one__", "_two__"])
                self.assertEqual(request["filters"], [{"label": "_Files__",
                                 "globs": ["*_pattern__"], "mimes": ["text/_plain__"]}])
                self.assertEqual(request["current"], "_Files__")


if __name__ == "__main__":
    unittest.main()
