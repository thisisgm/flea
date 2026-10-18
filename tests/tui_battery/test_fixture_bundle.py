"""The fixture bundle builder writes every family the states need, and import_family accepts it and refuses tampering."""
import contextlib
import io
import json
import os
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tempfile
import unittest
import zipfile

from .evidence import Blocked, MARKER
from .driver import main as battery
from .fixtures import WORK, build, import_family
from .bundle import standin

HERE = Path(__file__).resolve().parent
TOOL = HERE.parents[1] / "tools/flea-tui-fixture-bundle"
FAMILIES = ("work", "office", "prints", "ja", "ar", "trash")


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
        manifest["families"]["work"]["entries"][-1]["sha256"] = "0" * 64
        (tampered / "manifest.json").write_text(json.dumps(manifest))
        code, listed = self.listing(tampered)
        self.assertEqual(code, 2)
        self.assertEqual(listed["fixture_families"]["work"]["status"], "invalid")
        self.assertEqual(listed["fixture_families"]["office"]["status"], "missing")

    def test_existing_or_relative_output_is_refused(self):
        for output, reason in ((self.bundle, b"output must be new"), (Path("relative-bundle"), b"must be absolute")):
            with self.subTest(output=str(output)):
                result = builder(output, "--without-rasters")
                self.assertEqual(result.returncode, 2)
                self.assertIn(reason, result.stderr)
        self.assertFalse(Path("relative-bundle").exists())

if __name__ == "__main__":
    unittest.main()
