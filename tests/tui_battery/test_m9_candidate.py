"""Candidate composition exercises the real inventory, receipt and completion paths."""
import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import base64
import io
import xml.etree.ElementTree as ET
import zipfile

from . import candidate, driver, receipts
from .adapters import TERMINALS
from .evidence import Blocked, Commands, MARKER, digest

HERE = Path(__file__).resolve().parent


class CandidateTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-candidate.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned candidate regression\n")
        self.spec = driver.contract(Path(os.environ["FLEA_BATTERY_BOARDS"]))
        self.required = candidate.inventory(self.spec)

    def manifest(self):
        return {"mode": "native", "smoke": False, "candidate_inputs": {"keymap": "a", "catalog": "b"},
                "requested": self.required["requested"],
                "records": [dict(row, status="PASS") for row in self.required["requested"]],
                "controls": [dict(row, status="PASS") for row in self.required["controls"]],
                "calendar": {"records": [dict(row, status="PASS") for row in self.required["calendar"]]},
                "additional_surfaces": [dict(row, status="PASS") for row in self.required["surfaces"]]}

    def test_m9_candidate_counts_every_reference_and_variant(self):
        self.assertEqual(self.required["counts"], {"boards": 63, "pairs": 315, "captures": 325,
                         "calendar": 85, "controls": 135, "surfaces": 20})
        for terminal in TERMINALS:
            for state, variants in (("tui-small", {"wide-short", "narrow"}), ("tui-long", {"primary", "rest"})):
                self.assertEqual({r["variant"] for r in self.required["requested"]
                                  if (r["terminal"], r["state"]) == (terminal, state)}, variants)

    def test_m9_candidate_requires_calendar_controls_and_surfaces(self):
        passing = self.manifest()
        self.assertTrue(candidate.completion(passing, self.required)["complete_reference_matrix"])
        for group in ("records", "controls", "calendar", "additional_surfaces"):
            value = copy.deepcopy(passing)
            rows = value[group]["records"] if group == "calendar" else value[group]
            rows.pop()
            self.assertFalse(candidate.completion(value, self.required)["complete_reference_matrix"], group)
            rows.append(copy.deepcopy(rows[0]))
            self.assertFalse(candidate.completion(value, self.required)["complete_reference_matrix"], group)

    def test_m9_candidate_rejects_smoke_or_subset_completion(self):
        for change in ({"mode": "dry-run"}, {"smoke": True}, {"display_reason": "lost"},
                       {"candidate_inputs": {}}, {"requested": []}):
            value = dict(self.manifest(), **change)
            self.assertFalse(candidate.completion(value, self.required)["complete_reference_matrix"], change)
        value = self.manifest()
        value["records"][0]["status"] = "BLOCKED"
        self.assertFalse(candidate.completion(value, self.required)["complete_reference_matrix"])

    def test_m9_candidate_binds_catalog_and_keymap_identity(self):
        repo = self.root / "repo"
        (repo / "i18n/en").mkdir(parents=True)
        for name in ("keys.toml", "i18n/source-index.json", "i18n/translation-manifest.json", "i18n/en/flea.json"):
            (repo / name).write_text("original")
        original = candidate.input_identity(repo)
        for name in ("keys.toml", "i18n/source-index.json", "i18n/translation-manifest.json", "i18n/en/flea.json"):
            (repo / name).write_text("changed")
            self.assertNotEqual(candidate.input_identity(repo), original)
            (repo / name).write_text("original")

    def test_m9_candidate_private_cups_and_real_outputs_are_required(self):
        for label in ("tui-print", "tui-print-sent", "tui-convert", "tui-convert-pdf"):
            state = next(s for s in self.spec["states"] if s["id"] == label)
            self.assertTrue(state["requires"], label)
        controls = {r["id"]: r["checks"] for r in driver.load(HERE / "controls.json")["controls"]}
        self.assertIn("real-converted-output", controls["convert-variants"])
        self.assertIn("no-physical-print", controls["operations-refusals"])
        self.assertIn("CUPS-rejected", controls["operations-refusals"])
        missing = candidate.prerequisites(self.spec, {})
        self.assertTrue(missing)
        self.assertTrue(any("cups" in r["contract"].lower() for r in missing))
        with self.assertRaises(Blocked):
            candidate.output_proof(self.root, {"sources": [], "outputs": []})
        with self.assertRaises(Blocked):
            candidate.cups_proof(self.root, {"transport": "private-unix", "socket": "/fx/runtime/cups.sock", "requests": []})

    def test_real_fixture_output_bytes_and_cups_request_are_checked(self):
        from . import png
        fx = self.root / "fx/v/work"
        fx.mkdir(parents=True)
        source = fx / "source.pdf"
        source.write_bytes(b"%PDF-1.7\n" + b"original page\n" * 10 + b"%%EOF\n")
        (self.root / "fixtures.json").write_text(json.dumps({"copied": [{"path": "source.pdf", "sha256": digest(source)}]}))
        output = fx / "converted.png"
        png.write(output, 2, 2, bytes([255, 0, 0, 255]) * 4)
        for name in ("tmp", "home", "config", "cache", "state", "runtime", "data"):
            (self.root / "fx" / name).mkdir(exist_ok=True)
        from .bundle.outputs import decode_output
        commands = Commands()
        decoded = decode_output(self.root, output, "PNG", commands)
        (self.root / "commands.json").write_text(json.dumps(commands.records))
        record = {"sources": [{"path": "fx/v/work/source.pdf", "sha256": digest(source)}],
                  "outputs": [{"path": "fx/v/work/converted.png", "sha256": digest(output), "format": "PNG", "decode": decoded}]}
        self.assertTrue(candidate.output_proof(self.root, record))
        output.write_bytes(b"not a PNG" * 12)
        record["outputs"][0]["sha256"] = digest(output)
        with self.assertRaisesRegex(Blocked, "wrong actual format"):
            candidate.output_proof(self.root, record)
        body = b"\x02\x00\x00\x02\x00\x00\x00\x01\x03" + source.read_bytes()
        traffic = self.root / "cups-request.bin"
        traffic.write_bytes(b"POST /printers/flea HTTP/1.1\r\nContent-Type: application/ipp\r\n\r\n" + body)
        receipt = {"transport": "private-unix", "socket": "/fx/runtime/cups.sock",
                   "requests": [{"path": traffic.name, "sha256": digest(traffic)}]}
        with self.assertRaises(Blocked):
            candidate.cups_proof(self.root, receipt)
        receipt["socket"] = "/run/cups/cups.sock"
        with self.assertRaises(Blocked):
            candidate.cups_proof(self.root, receipt)

    def test_candidate_cli_lists_all_records_and_refuses_partial_execution(self):
        import contextlib
        out, error = io.StringIO(), io.StringIO()
        boards = os.environ["FLEA_BATTERY_BOARDS"]
        with contextlib.redirect_stdout(out):
            self.assertEqual(candidate.main(["--boards", boards, "--list"]), 0)
        self.assertEqual(json.loads(out.getvalue())["counts"], self.required["counts"])
        with contextlib.redirect_stderr(error), patch.object(driver, "execute_pair", side_effect=AssertionError("capture forbidden")):
            self.assertEqual(candidate.main(["--boards", boards, "--smoke"]), 2)
        self.assertFalse(json.loads(error.getvalue())["native_acceptance"])

    def test_join_preserves_bytes_and_rejects_stale_source(self):
        identity = {key: "same" for key in ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked")}
        identity["candidate_inputs"] = {"files": {"keys.toml": "bound"}}
        source = self.root / "source"
        directory = source / "foot/check"
        directory.mkdir(parents=True)
        (directory / "receipt").write_bytes(b"actual retained bytes")
        row = {"terminal": "foot", "kind": "control", "id": "check", "directory": "foot/check"}
        (source / "manifest.json").write_text(json.dumps({"identity": identity, "records": [row]}))
        destination = candidate.join_supplements(self.root / "joined", identity, [source])
        self.assertEqual((destination / "foot/check/receipt").read_bytes(), b"actual retained bytes")
        stale = dict(identity, untracked="changed")
        with self.assertRaises(Blocked):
            candidate.join_supplements(self.root / "stale", stale, [source])

    def test_m9_quicklook_has_picture_and_two_real_steps(self):
        state = next(s for s in self.spec["states"] if s["id"] == "tui-3d-quicklook")
        self.assertEqual([r["key"] for r in state["steps"] if r.get("key") == "Right"], ["Right", "Right"])
        self.assertNotIn("30", state["predicate"]["contains"])
        self.assertEqual(state["picture_steps"], {"orbit_degrees": 30, "presses": 2, "key": "Right", "plate": 1})

    def test_picture_steps_require_changed_frames_and_native_keys(self):
        state = next(s for s in self.spec["states"] if s["id"] == "tui-3d-quicklook")
        from .test_m9_pictures import pixels
        from . import png
        import hashlib
        directory = self.root / "pictures"
        directory.mkdir()
        (self.root / "picture-config.json").write_text(json.dumps({"native_rect": [0,0,4,6], "pixels": [4,6]}))
        frames = []
        for i in range(3):
            path = directory / (str(i) + ".png")
            rgba = pixels(i)
            png.write(path, 4, 6, rgba)
            frames.append({"mode": "half-blocks", "sha256": hashlib.sha256(rgba).hexdigest(), "end_ns": i * 10,
                           "observed_ns": i * 10, "capture": str(path.relative_to(self.root)),
                           "capture_sha256": digest(path), "rect": [0,0,4,6], "pixels": [4,6]})
        journal = [{"key": "Right", "delivery": "compositor", "ns": 1 + i * 10,
                    "before_picture": frames[i], "after_picture": frames[i+1]} for i in range(2)]
        log = self.root / "pty.log"
        events = [{"direction": "terminal-input", "ns": 2+i*10,
                   "base64": base64.b64encode(b"\x1b[C").decode()} for i in range(2)]
        log.write_text("".join(json.dumps(row)+"\n" for row in events))
        self.assertEqual(candidate.picture_steps(state, journal, log)["status"], "PASS")
        for defect in ("picture", "input", "synthetic", "count"):
            changed = copy.deepcopy(journal)
            actual = copy.deepcopy(events)
            if defect == "picture":
                changed[0]["after_picture"] = changed[0]["before_picture"]
            elif defect == "input":
                actual[0]["base64"] = ""
            elif defect == "synthetic":
                changed[0]["delivery"] = "pty-setup"
            else:
                changed.pop()
            log.write_text("".join(json.dumps(row)+"\n" for row in actual))
            with self.assertRaises(Blocked, msg=defect):
                candidate.picture_steps(state, changed, log)

    def test_native_picture_fingerprint_ignores_id_only_changes(self):
        from .test_m9_pictures import kitty, native_terminal, pixels
        terminal = native_terminal(self.root)
        def frame(ident, rgba):
            kitty(terminal, ident, rgba)
            return candidate.picture_frame(terminal)["sha256"]
        original = frame(1, pixels())
        self.assertEqual(frame(2, pixels()), original)
        self.assertNotEqual(frame(3, pixels(1)), original)

    def test_conversion_matrix_cannot_count_one_png_as_every_format(self):
        from . import png
        directory = self.root / "controls"
        case = directory / "captures/0"
        (case / "fx/v/work").mkdir(parents=True)
        source, output = case / "fx/v/work/source.pdf", case / "fx/v/work/output.png"
        source.write_bytes(b"%PDF-1.7\noriginal document\n%%EOF")
        png.write(output, 2, 2, bytes([255, 0, 0, 255]) * 4)
        (case / MARKER).write_text("owned conversion matrix control\n")
        for name in ("tmp", "home", "config", "cache", "state", "runtime", "data"):
            (case / "fx" / name).mkdir(exist_ok=True)
        from .bundle.outputs import decode_output
        commands = Commands()
        decoded = decode_output(case, output, "PNG", commands)
        (case / "commands.json").write_text(json.dumps(commands.records))
        (case / "fixtures.json").write_text(json.dumps({"copied": [{"path": "source.pdf", "sha256": digest(source)}]}))
        receipt = {"sources": [{"path": "fx/v/work/source.pdf", "sha256": digest(source)}],
                   "outputs": [{"path": "fx/v/work/output.png", "sha256": digest(output), "format": "PNG", "decode": decoded}]}
        (case / "operation-output.json").write_text(json.dumps(receipt))
        sha = digest(case / "operation-output.json")
        (case / "verdict.json").write_text(json.dumps({"artifact_sha256": {"operation-output.json": sha}}))
        proof = {"capture": "captures/0", "proof": {"artifacts": {"operation-output.json": sha}}}
        checks = {"real-converted-output": [proof], "image-JPEG-PNG-WebP-AVIF-HEIC-TIFF-BMP-PDF": [proof]}
        (directory / "checks.json").write_text(json.dumps(checks))
        with self.assertRaisesRegex(Blocked, "omitted actual format outputs"):
            candidate.validate_operations(directory, {"id": "convert-variants"})

    def test_model_archive_carries_actual_materials_bed_and_plate_membership(self):
        from .bundle import models
        with zipfile.ZipFile(io.BytesIO(models.shelf_kit_3mf(1200000))) as archive:
            model = ET.fromstring(archive.read("3D/3dmodel.model"))
            namespace = {"m": "http://schemas.microsoft.com/3dmanufacturing/core/2015/02"}
            bases = model.findall("m:resources/m:basematerials/m:base", namespace)
            self.assertEqual([b.attrib["displaycolor"] for b in bases], ["#FF6A13"])
            objects = model.findall("m:resources/m:object", namespace)
            self.assertEqual([(o.attrib.get("pid"), o.attrib.get("pindex")) for o in objects],
                             [(None, None), (None, None), ("1024", "0"), ("1024", "0")])
            metadata = {m.attrib["name"]: m.text for m in model.findall("m:metadata", namespace)}
            self.assertEqual(metadata["printable_area"], "0x0,256x0,256x256,0x256")
            settings = ET.fromstring(archive.read("Metadata/model_settings.config"))
            self.assertEqual([[int(n.attrib["value"]) for n in p.findall("model_instance/metadata[@key='object_id']")]
                              for p in settings.findall("plate")], [[1, 2, 3], [4]])

    def test_no_chafa_resolver_is_private_and_has_no_system_fallback(self):
        from .fixtures import no_chafa, namespace
        fx = self.root / "fx"
        (fx / "tools").mkdir(parents=True)
        system = self.root / "bin"
        system.mkdir()
        for name in ("chafa", "decoder"):
            path = system / name
            path.write_text("#!/bin/sh\nexit 0\n")
            path.chmod(0o700)
        receipt = no_chafa(fx, [system])
        self.assertEqual(receipt["PATH"], "/fx/tools-no-chafa")
        self.assertNotIn("chafa", receipt["entries"])
        self.assertEqual((fx / "tools-no-chafa/decoder").read_bytes(), (system / "decoder").read_bytes())
        self.assertTrue((system / "chafa").is_file())


if __name__ == "__main__":
    unittest.main()
