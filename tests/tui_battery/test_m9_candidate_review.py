"""Real-path regressions for cold candidate acceptance findings."""
import json
import contextlib
import io
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

from . import candidate, receipts, driver, reconcile
from .evidence import Blocked, MARKER, Owned, digest


class CandidateReviewTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-review.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned candidate review regression\n")

    def test_missing_auxiliary_identity_cannot_be_relabelled_at_join(self):
        identity = {key: "same" for key in ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked")}
        identity["candidate_inputs"] = {"auxiliary": {"font": "current"}}
        source = self.root / "old"
        source.mkdir()
        old = {k: v for k, v in identity.items() if k != "candidate_inputs"}
        (source / "manifest.json").write_text(json.dumps({"identity": old, "records": []}))
        with self.assertRaisesRegex(Blocked, "inputs changed"):
            candidate.join_supplements(self.root / "joined", identity, [source])

    def test_copied_calendar_verdict_cannot_fill_a_different_label(self):
        from .test_m9_calendar import CalendarTests
        from . import calendar_native
        helper = CalendarTests()
        helper.root = self.root
        old, new = "folder-a5-preview", "folder-older-preview"
        source, identity = helper.bundle(labels=[old])
        manifest = json.loads((source / "manifest.json").read_text())
        row = manifest["records"][0]
        shutil.copytree(source / row["directory"], source / "kitty" / new)
        row.update(id=new, directory="kitty/" + new)
        (source / "manifest.json").write_text(json.dumps(manifest))
        result = receipts.supplement(source, identity, ["kitty"], [],
                                     {"states": [new], "driver_sha256": calendar_native.ORIGINAL_DRIVER_SHA256})
        self.assertEqual(result[0]["status"], "BLOCKED")
        self.assertIn("another terminal or state", result[0]["reason"])

    def test_truncated_output_signatures_never_count_as_conversion(self):
        work = self.root / "fx/v/work"
        work.mkdir(parents=True)
        source = work / "source.txt"
        source.write_text("original input\n")
        (self.root / "fixtures.json").write_text(json.dumps({"copied": [{"path": "source.txt", "sha256": digest(source)}]}))
        (self.root / "commands.json").write_text("[]")
        for name, data, kind in (("partial.png", b"\x89PNG\r\n\x1a\n", "PNG"), ("partial.pdf", b"%PDF-", "PDF")):
            with self.subTest(kind=kind):
                output = work / name
                output.write_bytes(data)
                record = {"sources": [{"path": "fx/v/work/source.txt", "sha256": digest(source)}],
                          "outputs": [{"path": "fx/v/work/" + name, "sha256": digest(output), "format": kind}]}
                with self.assertRaises(Blocked):
                    candidate.output_proof(self.root, record)

    def retained(self):
        from .test_review_r2 import Review2Tests
        helper = Review2Tests()
        helper.root = self.root
        return helper.retained_case()

    def check_retained(self, source, identity, row, config):
        with patch.object(reconcile, "visual_review", return_value={"status": "PASS", "review": {}}):
            return reconcile.retained(source, identity, [row], self.root, {},
                       {"foot/tui-list-text/primary": config})[0]

    def test_retained_edge_machine_failures_never_become_visual_passes(self):
        source, case, identity, row, config = self.retained()
        path = case / "verdict.json"
        original = json.loads(path.read_text())
        self.assertEqual(self.check_retained(source, identity, row, config)["status"], "PASS")
        for status in ("FAIL", "BLOCKED"):
            value = dict(original, edges={"status": status})
            path.write_text(json.dumps(value))
            self.assertNotEqual(self.check_retained(source, identity, row, config)["status"], "PASS")

    def test_raw_bad_shutdown_cannot_hide_behind_matching_hashes(self):
        source, case, identity, row, config = self.retained()
        path = case / "verdict.json"
        verdict = json.loads(path.read_text())
        (case / "launch-exit.json").write_text('{"exit_code":9}')
        candidate.seal_extra(case, verdict["artifact_sha256"])
        path.write_text(json.dumps(verdict))
        self.assertEqual(self.check_retained(source, identity, row, config)["status"], "FAIL")

    def test_recognized_review_pending_reason_reconciles_after_approval(self):
        source, case, identity, row, config = self.retained()
        path = case / "verdict.json"
        verdict = json.loads(path.read_text())
        verdict["reason"] = "controller native visual review required"
        path.write_text(json.dumps(verdict))
        self.assertEqual(self.check_retained(source, identity, row, config)["status"], "PASS")

    def test_candidate_preflight_uses_only_existing_preflight_path(self):
        boards = os.environ["FLEA_BATTERY_BOARDS"]
        with patch.object(driver, "main", return_value=0) as run:
            self.assertEqual(candidate.main(["--boards", boards, "--preflight"]), 0)
        run.assert_called_once_with(["--boards", boards, "--preflight"])

    def test_supplemental_png_and_html_changes_are_identity_changes(self):
        reference = self.root / "references"
        (reference / "boards").mkdir(parents=True)
        for name in candidate.SURFACE_REFERENCES:
            (reference / name).write_bytes(b"original reference")
        before = candidate.input_identity(boards=reference)
        self.assertEqual(before["missing_surface_references"], [])
        (reference / "boards/jobs.png").write_bytes(b"changed reference")
        self.assertNotEqual(candidate.input_identity(boards=reference), before)

    def calendar_args(self, output, inputs=None):
        from types import SimpleNamespace
        return SimpleNamespace(output=output, boards=Path(os.environ["FLEA_BATTERY_BOARDS"]),
                               binary=Path("/not-executed"), identity_inputs=inputs, terminals="kitty",
                               preset="default", timeout=1, profiles=None, reviews=None)

    def test_calendar_capsule_records_inputs_before_calling_accepted_producer(self):
        from . import calendar_native
        from .test_m9_calendar import CalendarTests
        helper = CalendarTests()
        helper.root = self.root
        source, identity = helper.bundle(labels=["folder-a5-preview"])
        output = self.root / "capsule"
        before_raw = (source / "manifest.json").read_bytes()
        def capture(args):
            self.assertTrue((output / "capture-inputs.json").is_file())
            self.assertFalse((output / "capture-finish.json").exists())
            shutil.copytree(source, args.output)
            return 2
        with patch.object(calendar_native, "run", side_effect=capture) as run, \
                patch.object(candidate, "source_identity", return_value=identity):
            self.assertEqual(candidate.capture_calendar(self.calendar_args(output)), 2)
        run.assert_called_once()
        self.assertEqual((output / "raw/manifest.json").read_bytes(), before_raw)
        self.assertEqual((source / "manifest.json").read_bytes(), before_raw)
        capsule = json.loads((output / "manifest.json").read_text())
        self.assertEqual(capsule["identity"]["candidate_inputs"], candidate.input_identity(boards=self.calendar_args(output).boards))
        self.assertFalse(capsule["native_acceptance"])
        self.assertEqual((output / "kitty/folder-a5-preview/native.png").read_bytes(),
                         (source / "kitty/folder-a5-preview/native.png").read_bytes())

    def test_calendar_inputs_changed_during_capture_never_get_stamped_current(self):
        from . import calendar_native
        from .test_m9_calendar import CalendarTests
        helper = CalendarTests()
        helper.root = self.root
        source, identity = helper.bundle(labels=["folder-a5-preview"])
        auxiliary = self.root / "font-receipt"
        auxiliary.write_text("before")
        inputs = self.root / "inputs.json"
        inputs.write_text(json.dumps({"font": str(auxiliary)}))
        output = self.root / "changed"
        def capture(args):
            shutil.copytree(source, args.output)
            auxiliary.write_text("after")
            return 2
        with patch.object(calendar_native, "run", side_effect=capture), \
                patch.object(candidate, "source_identity", return_value=identity):
            with self.assertRaisesRegex(Blocked, "changed during capture"):
                candidate.capture_calendar(self.calendar_args(output, inputs))
        self.assertFalse((output / "manifest.json").exists())

    def calendar_capsule(self):
        from . import calendar_native
        from .test_m9_calendar import CalendarTests
        helper = CalendarTests()
        helper.root = self.root
        source, identity = helper.bundle(labels=["folder-a5-preview"])
        output = self.root / "capsule"
        def capture(args):
            shutil.copytree(source, args.output)
            return 2
        with patch.object(calendar_native, "run", side_effect=capture), \
                patch.object(candidate, "source_identity", return_value=identity):
            candidate.capture_calendar(self.calendar_args(output))
        manifest = json.loads((output / "manifest.json").read_text())
        return output, manifest["identity"]

    def test_calendar_capsule_provenance_is_checked_retained_and_rechecked(self):
        source, identity = self.calendar_capsule()
        saved = {name: (source / name).read_bytes() for name in
                 ("capture-inputs.json", "capture-finish.json", "raw/manifest.json")}
        for i, (name, data) in enumerate(saved.items()):
            (source / name).write_bytes(b"{}")
            with self.assertRaisesRegex(Blocked, "receipt.*changed"):
                candidate.join_supplements(self.root / ("bad-" + str(i)), identity, [source])
            (source / name).write_bytes(data)
        joined = candidate.join_supplements(self.root / "joined", identity, [source])
        manifest = json.loads((joined / "manifest.json").read_text())
        capsule = joined / manifest["calendar_capsules"][0]["directory"]
        for name, data in saved.items():
            self.assertEqual((capsule / name).read_bytes(), data)
        candidate.validate_calendar_inputs(joined, identity, manifest)
        Owned(self.root).child((capsule / "capture-finish.json").relative_to(self.root)).unlink()
        with self.assertRaises((Blocked, OSError)):
            receipts.supplement(joined, identity, ["kitty"], [], {"states": ["folder-a5-preview"]})

    def test_calendar_capture_rejects_conflicting_modes_before_producer(self):
        from . import calendar_native
        common = ["--boards", os.environ["FLEA_BATTERY_BOARDS"], "--capture-calendar", "--binary", "/not-executed",
                  "--output", str(self.root / "never-launched")]
        for flag in (["--dry-run"], ["--reconcile", str(self.root)], ["--smoke"], ["--list"]):
            with patch.object(calendar_native, "run", side_effect=AssertionError("capture forbidden")) as run, \
                    contextlib.redirect_stderr(io.StringIO()):
                self.assertEqual(candidate.main(common + flag), 2)
                run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
