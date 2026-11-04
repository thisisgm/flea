"""Current approved inventory through the real offline loader and battery CLI."""
import io
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

from . import offline
from .adapters import TERMINALS
from .evidence import Blocked, MARKER, Owned, digest

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[1]


class M9InventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.manifest = json.loads((HERE / "fixtures/references.json").read_text())
        cls.inventory = cls.manifest["inventory"]
        cls.spec = json.loads((HERE / "states.json").read_text())
        cls.boards = Path(os.environ["FLEA_BATTERY_BOARDS"])

    def setUp(self):
        parent = Path(os.environ["FLEA_BATTERY_TEST_ROOT"])
        self.root = Path(tempfile.mkdtemp(prefix="m9-inventory.", dir=parent))
        (self.root / MARKER).write_text("owned inventory regression\n")
        self.addCleanup(self.cleanup)

    def cleanup(self):
        if (self.root.is_absolute() and self.root.name.startswith("m9-inventory.")
                and (self.root / MARKER).read_text().strip()):
            shutil.rmtree(self.root)

    def listing(self, *arguments, boards=None):
        result = subprocess.run(
            [sys.executable, "-B", str(REPO / "tools/flea-tui-battery"),
             "--boards", str(boards or self.boards), "--list", *arguments],
            stdin=subprocess.DEVNULL, capture_output=True, text=True, timeout=30)
        return result, json.loads(result.stdout if result.returncode == 0 else result.stderr)

    def test_m9_current_reference_inventory_is_complete(self):
        self.assertEqual(digest(HERE / "states.json"), self.inventory["state_manifest_sha256"])
        self.assertEqual(digest(HERE / "fixtures/references.tar.xz"), self.manifest["archive_sha256"])
        self.assertEqual(list(TERMINALS), self.inventory["terminals"])
        for group, key, count in (("states", "original_states", 61),
                                  ("additional_states", "additional_states", 2)):
            self.assertEqual([state["id"] for state in self.spec[group]], self.inventory[key])
            self.assertEqual(len(self.spec[group]), count)
        required = self.inventory["original_states"] + self.inventory["additional_states"]
        self.assertEqual(len(set(required)), 63)
        self.assertEqual(self.spec["excluded"], self.inventory["excluded"])
        self.assertEqual(self.spec["excluded"], [{"id": "tui-send-with", "reason":
                         "Provider submenu not selected; providers are separate file-menu rows."}])
        self.assertEqual(self.spec["reserved"], [])
        expected = {f"boards/{name}.{suffix}" for name in required + ["tui-send-with"]
                    for suffix in ("png", "html")}
        self.assertEqual(set(self.manifest["files"]), expected)
        self.assertEqual({str(p.relative_to(self.boards)) for p in (self.boards / "boards").iterdir()}, expected)
        for name, record in self.manifest["files"].items():
            with self.subTest(reference=name):
                path = self.boards / name
                self.assertEqual(path.stat().st_size, record["size"])
                self.assertEqual(digest(path), record["sha256"])
        result, listing = self.listing()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual({r["state"] for r in listing["requested"]}, set(required))
        self.assertEqual(listing["excluded"], self.spec["excluded"])
        self.assertEqual(listing["logical_pairs_requested"], 315)
        print("PASS test_m9_current_reference_inventory_is_complete: 63 boards, 128 verified inputs", flush=True)

    def test_m9_convert_is_required_in_all_five_terminals(self):
        result, listing = self.listing("--states", "tui-convert,tui-convert-pdf")
        self.assertEqual(result.returncode, 0, result.stderr)
        requested = [(r["terminal"], r["state"], r["variant"]) for r in listing["requested"]]
        expected = {(terminal, state, "primary") for terminal in TERMINALS
                    for state in ("tui-convert", "tui-convert-pdf")}
        self.assertEqual(len(requested), 10)
        self.assertEqual(set(requested), expected)
        self.assertEqual(listing["logical_pairs_requested"], 10)
        self.assertEqual(listing["required_additional_records"], 10)
        print("PASS test_m9_convert_is_required_in_all_five_terminals: 10 mandatory pairs", flush=True)

    def test_m9_variant_counts_include_small_and_long(self):
        result, listing = self.listing()
        self.assertEqual(result.returncode, 0, result.stderr)
        requested = [(r["terminal"], r["state"], r["variant"]) for r in listing["requested"]]
        self.assertEqual(len(requested), 325)
        self.assertEqual(len(set(requested)), 325)
        self.assertEqual(len({(t, s) for t, s, _ in requested}), 315)
        for terminal in TERMINALS:
            for state, variants in (("tui-long", {"primary", "rest"}),
                                     ("tui-small", {"wide-short", "narrow"})):
                self.assertEqual({v for t, s, v in requested if t == terminal and s == state}, variants)
        states = {s["id"]: s for s in self.spec["states"]}
        self.assertEqual([(v["columns"], v["rows"], v["origin"]) for v in states["tui-small"]["variants"]],
                         [(130, 4, [4, 9]), (11, 6, [4, 175])])
        self.assertEqual(self.inventory["counts"], dict(required_boards=63, excluded_boards=1,
                         reference_inputs=128, terminal_state_pairs=315, variant_captures=325))
        print("PASS test_m9_variant_counts_include_small_and_long: 315 pairs, 325 captures", flush=True)

    def snapshot(self, name, mutation):
        fixture = self.root / name / "fixtures"
        fixture.mkdir(parents=True)
        manifest = json.loads(json.dumps(self.manifest))
        archive = fixture / "references.tar.xz"
        target = "boards/tui-convert.png"
        with tarfile.open(HERE / "fixtures/references.tar.xz", "r:xz") as source:
            with tarfile.open(archive, "w:xz", preset=0) as output:
                for member in source:
                    data = source.extractfile(member).read()
                    if member.name == target:
                        if mutation == "missing":
                            continue
                        if mutation == "tampered":
                            data = bytes([data[0] ^ 1]) + data[1:]
                        if mutation == "size":
                            data += b"x"
                            member.size = len(data)
                    output.addfile(member, io.BytesIO(data))
                    if member.name == target and mutation == "duplicate":
                        output.addfile(member, io.BytesIO(data))
                if mutation == "new":
                    member = tarfile.TarInfo("boards/tui-unreviewed.png")
                    member.size = 1
                    output.addfile(member, io.BytesIO(b"x"))
        manifest["archive_sha256"] = digest(archive)
        if mutation == "archive":
            with archive.open("ab") as output:
                output.write(b"unreviewed")
        (fixture / "references.json").write_text(json.dumps(manifest))
        owner = fixture.parent / "owner"
        owner.mkdir()
        (owner / MARKER).write_text("owned negative reference loader\n")
        return fixture.parent, Owned(owner)

    def test_m9_unreviewed_reference_change_is_rejected(self):
        reasons = {"tampered": "member identity mismatch", "size": "member size mismatch",
                   "missing": "snapshot is incomplete", "new": "unexpected entry",
                   "duplicate": "unexpected entry", "archive": "archive identity mismatch"}
        for mutation, reason in reasons.items():
            with self.subTest(mutation=mutation):
                source, owner = self.snapshot(mutation, mutation)
                with patch.object(offline, "HERE", source), self.assertRaisesRegex(Blocked, reason):
                    offline.references(owner)
        for mutation in ("missing", "new"):
            source = self.root / ("dispatch-" + mutation)
            boards = source / "boards"
            boards.mkdir(parents=True)
            for path in (self.boards / "boards").iterdir():
                if mutation == "missing" and path.name == "tui-convert.png":
                    continue
                (boards / path.name).symlink_to(path)
            if mutation == "new":
                (boards / "tui-unreviewed.png").write_bytes(b"not an approved reference")
            result, listing = self.listing(boards=source)
            self.assertEqual(result.returncode, 2, result.stderr)
            self.assertEqual(listing["status"], "BLOCKED")
            self.assertIn("reference inventory mismatch", listing["reason"])
        print("PASS test_m9_unreviewed_reference_change_is_rejected: archive/member/dispatch negatives", flush=True)


if __name__ == "__main__":
    unittest.main()
