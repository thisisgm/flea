"""The fixture bundle builder writes every family the states need, and import_family accepts it and refuses tampering."""
import contextlib
import io
import json
import os
import re
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from unittest import mock
import zipfile
import zlib

from . import offline
from .evidence import Blocked, MARKER, Owned, output_root
from .driver import main as battery
from .fixtures import WORK, build, discard, import_family, inventory
from .bundle import build as bundle_build, office, standin

HERE = Path(__file__).resolve().parent
TOOL = HERE.parents[1] / "tools/flea-tui-fixture-bundle"
FAMILIES = ("work", "office", "prints", "ja", "ar", "trash")
WORK_STATE = [{"id": "tui-list", "family": "work", "requires": []}]


def unlock(root):
    """Restores owner write on a read-only bundle or a locked import inside this test's own marked root."""
    if (root / MARKER).is_file() or any((p / MARKER).is_file() for p in root.parents):
        for path in [root, *root.rglob("*")]:
            if path.is_dir() and not path.is_symlink():
                path.chmod(0o755)


def builder(output, *extra):
    env = {"PATH": "/usr/bin:/bin", "HOME": str(output.parent), "LANG": "C.UTF-8", "TZ": "UTC"}
    return subprocess.run([sys.executable, "-B", str(TOOL), str(output), *extra], env=env, stdin=subprocess.DEVNULL,
                          capture_output=True, timeout=600)


class FixtureBundleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        parent = Path(os.environ.get("FLEA_BATTERY_TEST_ROOT", HERE.parents[1] / ".superpowers/t12a"))
        cls.scratch = Path(tempfile.mkdtemp(prefix="bundle.", dir=parent.absolute()))
        (cls.scratch / MARKER).write_text("fixture bundle tests\n")
        # Stand-in seeds keep the offline suite free of encoders; the native run seeds real bench media.
        standin.seed(cls.scratch / "seed")
        cls.bundle = cls.scratch / "bundle"
        result = builder(cls.bundle, "--seed", str(cls.scratch / "seed"), "--without-rasters")
        cls.receipt = json.loads(result.stdout or b"{}") if result.returncode == 0 else {"error": result.stderr.decode()}
        cls.manifest = json.loads((cls.bundle / "manifest.json").read_text()) if result.returncode == 0 else {}

    @classmethod
    def tearDownClass(cls):
        unlock(cls.scratch)

    def case(self):
        root = Path(tempfile.mkdtemp(prefix="case.", dir=self.scratch))
        (root / MARKER).write_text("owned import case\n")
        self.addCleanup(unlock, root)
        return root

    def test_builder_reports_every_family(self):
        self.assertNotIn("error", self.receipt, self.receipt.get("error"))
        self.assertEqual(set(self.manifest["families"]), set(FAMILIES))
        self.assertEqual(self.receipt["families"], {f: len(self.manifest["families"][f]["entries"]) for f in FAMILIES})

    def test_every_family_imports_with_its_auxiliary_files(self):
        for family in FAMILIES:
            with self.subTest(family=family):
                fx, receipt = build(self.case(), family, self.bundle)
                self.addCleanup(discard, fx.parent)
                files = [e for e in self.manifest["families"][family]["entries"] if e["kind"] == "file"]
                self.assertEqual(len(receipt["copied"]), len(files))
                self.assertTrue(set(WORK).issubset(p.name for p in (fx / "v/work").iterdir()))
                self.assertTrue(os.access(fx / "tools/tailscale", os.X_OK))
                self.assertEqual((fx / "v/work/Locked").stat().st_mode & 0o777, 0)
                self.assertEqual((fx / "v/work/notes.txt").stat().st_mtime_ns, 1749981600 * 10 ** 9)
                if family == "trash":
                    self.assertEqual((fx / "data/Trash/files/old-draft.txt").read_bytes(), b"an old draft\n")
                    self.assertIn(b"DeletionDate=2025-06-14T09:30:00", (fx / "data/Trash/info/old-draft.txt.trashinfo").read_bytes())
                if family == "prints":
                    stl = (fx / "v/work/Prints/bracket.stl").read_bytes()
                    self.assertEqual((len(stl), struct.unpack_from("<I", stl, 80)[0]), (210684, 4212))
                if family in ("ja", "ar", "office"):
                    self.assertEqual(len(list((fx / "v/work/Docs").iterdir())), 8)

    def test_office_documents_are_real_containers(self):
        docs = self.bundle / "families/office/Docs"
        for name in ("budget.xlsx", "garden-report.docx", "talk.odp"):
            with self.subTest(name=name), zipfile.ZipFile(docs / name) as archive:
                self.assertIsNone(archive.testzip())
        with zipfile.ZipFile(docs / "talk.odp") as archive:
            self.assertEqual(archive.namelist()[0], "mimetype")
        lease = (docs / "lease.docx").read_bytes()
        self.assertEqual((lease[:8].hex(), len(lease)), ("d0cf11e0a1b11ae1", 8192))
        self.assertIn("EncryptedPackage".encode("utf-16-le"), lease)
        report = (docs / "garden-report.pdf").read_bytes()
        xref = int(report.rsplit(b"startxref\n", 1)[1].split(b"\n", 1)[0])
        self.assertTrue(report.startswith(b"%PDF-1.7") and report[xref:xref + 4] == b"xref")
        self.assertIn(b"/Count 3", report)

    def test_wrong_size_and_missing_family_are_refused(self):
        tampered = self.case() / "tampered"
        shutil.copytree(self.bundle / "families/office", tampered / "families/office", symlinks=True)
        manifest = json.loads(json.dumps(self.manifest))
        manifest["families"] = {"office": manifest["families"]["office"]}
        for row in manifest["families"]["office"]["entries"]:
            if row["path"] == "Docs/budget.xlsx":
                row["size"] += 1
        (tampered / "manifest.json").write_text(json.dumps(manifest))
        with self.assertRaisesRegex(Blocked, "identity mismatch: office/Docs/budget.xlsx"):
            import_family(tampered, "office", self.case() / "target")
        with self.assertRaisesRegex(Blocked, "lacks family: ja"):
            import_family(tampered, "ja", self.case() / "target")

    def listing(self, bundle):
        boards = os.environ.get("FLEA_BATTERY_BOARDS")
        if not boards:
            raise Blocked("FLEA_BATTERY_BOARDS is unset: set it to the design-hig directory that holds boards/")
        out = io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(io.StringIO()):
            code = battery(["--boards", boards, "--fixtures", str(bundle), "--list"])
        return code, json.loads(out.getvalue() or "{}")

    def test_list_shows_every_needed_family_present(self):
        code, manifest = self.listing(self.bundle)
        self.assertEqual(code, 0)
        families = manifest["fixture_families"]
        self.assertEqual(set(families), set(FAMILIES))
        self.assertTrue(all(row["status"] == "present" for row in families.values()), families)
        self.assertEqual(families["prints"]["representations"], ["tui-3d-quicklook"])

    def test_list_refuses_a_tampered_or_partial_bundle(self):
        tampered = self.case() / "tampered"
        shutil.copytree(self.bundle / "families/work", tampered / "families/work", symlinks=True)
        manifest = json.loads(json.dumps(self.manifest))
        manifest["families"] = {"work": manifest["families"]["work"]}
        manifest["families"]["work"]["auxiliary"] = []
        row = next(row for row in manifest["families"]["work"]["entries"] if row["kind"] == "file")
        row["sha256"] = "0" * 64
        (tampered / "manifest.json").write_text(json.dumps(manifest))
        code, listed = self.listing(tampered)
        self.assertEqual(code, 2)
        self.assertEqual(listed["fixture_families"]["work"]["status"], "invalid")
        self.assertEqual(listed["fixture_families"]["work"]["reason"],
                         f"fixture source identity mismatch: work/{row['path']}: sha256 differs from the manifest")
        self.assertEqual(listed["fixture_families"]["office"]["status"], "missing")

    def tampered_manifest(self, change):
        """A copy of the work family whose manifest change() edits in place; returns the copy's root."""
        tampered = self.case() / "tampered"
        shutil.copytree(self.bundle / "families/work", tampered / "families/work", symlinks=True)
        shutil.copytree(self.bundle / "aux", tampered / "aux", symlinks=True)
        manifest = json.loads(json.dumps(self.manifest))
        manifest["families"] = {"work": manifest["families"]["work"]}
        manifest = change(manifest) or manifest
        (tampered / "manifest.json").write_text(json.dumps(manifest) if isinstance(manifest, (dict, list)) else manifest)
        return tampered

    def test_list_names_a_tampered_auxiliary_file(self):
        row = self.manifest["families"]["work"]["auxiliary"][0]
        tampered = self.tampered_manifest(lambda m: m["families"]["work"]["auxiliary"][0].update(sha256="0" * 64))
        listed = inventory(tampered, WORK_STATE)["work"]
        self.assertEqual(listed["status"], "invalid")
        self.assertEqual(listed["reason"], f"fixture source identity mismatch: work/{row['source']}: sha256 differs from the manifest")

    def test_corrupt_or_misshapen_manifest_is_blocked(self):
        for text in ("{not json", "[]", '{"families": []}'):
            with self.subTest(manifest=text):
                tampered = self.tampered_manifest(lambda m: text)
                with self.assertRaisesRegex(Blocked, "fixture manifest"):
                    inventory(tampered, WORK_STATE)
        tampered = self.tampered_manifest(lambda m: m["families"].update(work=["not", "a", "family"]))
        self.assertEqual(inventory(tampered, WORK_STATE)["work"],
                         {"status": "invalid", "states": ["tui-list"], "reason": "fixture family work is not an object"})

    def test_recipe_report_pdf_ignores_the_writer_clock(self):
        calls, pixels = [], bytes(i % 251 for i in range(600 * 800 * 3))

        def stamping(argv, scratch, timeout=180):
            # A writer that stamps Info, XMP and /ID from the clock, as ImageMagick may; raw pixels carry no stamp.
            out, n = argv[-1], len(calls)
            calls.append(argv)
            if out.startswith("rgb:"):
                Path(out[4:]).write_bytes(pixels)
            else:
                Path(out).write_bytes(b"%%PDF-1.3\n<< /CreationDate (D:2026101812000%d) >>\n<xmp:CreateDate>2026-10-18T12:00:0%dZ"
                                      b"</xmp:CreateDate>\ntrailer << /ID [<%02d> <%02d>] >>\n%%%%EOF\n" % (n, n, n, n))

        made = []
        with mock.patch.object(bundle_build, "run", stamping):
            for _ in range(2):
                scratch = self.case()
                made.append(bundle_build.seed_bytes("report.pdf", None, scratch, {}))
        self.assertEqual(made[0], made[1])
        self.assertEqual(len(made[0]), 21840)
        self.assertNotIn(b"/ID", made[0])
        report = made[0]
        xref = int(report.rsplit(b"startxref\n", 1)[1].split(b"\n", 1)[0])
        count = int(re.match(rb"xref\n0 (\d+)\n", report[xref:]).group(1))
        rows = report[xref:].split(b"\n")[3:2 + count]
        self.assertEqual(len(rows), count - 1)
        for number, row in enumerate(rows, 1):
            offset = int(row[:10])
            self.assertTrue(report[offset:].startswith(b"%d 0 obj\n" % number), number)
        head = re.search(rb"/Subtype /Image .*?/Length (\d+) >>\nstream\n", report)
        stream = report[head.end():report.index(b"\nendstream", head.end())]
        self.assertEqual(len(stream), int(head.group(1)))
        self.assertEqual(zlib.decompress(stream), pixels)

    def test_two_builds_without_a_seed_in_different_seconds_match(self):
        receipts, starts = [], []
        for name in ("first", "second"):
            while starts and int(time.time()) == starts[-1]:
                time.sleep(0.05)
            starts.append(int(time.time()))
            result = builder(self.case() / name, "--without-rasters")
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            receipts.append(json.loads(result.stdout))
        self.assertNotEqual(starts[0], starts[1])
        self.assertEqual(receipts[0]["bundle_sha256"], receipts[1]["bundle_sha256"])
        self.assertEqual(receipts[0]["manifest_sha256"], receipts[1]["manifest_sha256"])

    def test_failed_office_build_is_not_cached(self):
        saved = dict(bundle_build._OFFICE)
        self.addCleanup(lambda: (bundle_build._OFFICE.clear(), bundle_build._OFFICE.update(saved)))
        bundle_build._OFFICE.clear()
        with mock.patch.object(office, "xlsx", lambda target: b"short"):
            with self.assertRaisesRegex(Blocked, "budget.xlsx is 5 bytes"):
                bundle_build.office_documents()
        self.assertEqual(bundle_build._OFFICE, {})
        docs = bundle_build.office_documents()
        self.assertEqual({name: len(data) for name, data in docs.items()}, bundle_build.OFFICE_SIZES)

    def test_offline_bundle_failure_leaves_a_removable_root(self):
        owner = self.case()

        def failing(argv):
            root = output_root(Path(argv[0])).path
            (root / "families").mkdir()
            for path in (root / "families", root):
                path.chmod(0o555)
            return 2

        with mock.patch.object(offline, "fixture_bundle", failing):
            with self.assertRaisesRegex(Blocked, "did not build"):
                offline.bundle_dry_run(Owned(owner), owner / "boards")
        shutil.rmtree(owner / "bundle")
        self.assertFalse((owner / "bundle").exists())

    def test_existing_or_relative_output_is_refused(self):
        for output, reason in ((self.bundle, b"output must be new"), (Path("relative-bundle"), b"must be absolute")):
            with self.subTest(output=str(output)):
                result = builder(output, "--without-rasters")
                self.assertEqual(result.returncode, 2)
                self.assertIn(reason, result.stderr)
        self.assertFalse(Path("relative-bundle").exists())

if __name__ == "__main__":
    unittest.main()
