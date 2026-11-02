"""B7 candidate collector contracts. These stubs make no native timing claim."""

import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import types
import unittest
from unittest import mock

REPO = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(REPO / "tools/flea-bench-thumbs-lib"))
import cgroup_cpu
import gpui
import measure

if os.environ.get("B7_BASE_MEASURE"):
    loader = importlib.machinery.SourceFileLoader("base_measure", os.environ["B7_BASE_MEASURE"])
    spec = importlib.util.spec_from_loader(loader.name, loader)
    measure = importlib.util.module_from_spec(spec)
    loader.exec_module(measure)


class Candidate(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="gpui-", dir=os.environ["FLEA_B7_TEST_ROOT"]))
        self.fixture = self.root / "fixture"
        self.fixture.mkdir()
        self.files = []
        for index in range(16):
            path = self.fixture / f"image-{index:02d}.jpg"
            path.write_bytes(b"fixture")
            self.files.append(path)
        self.log = self.root / "app.log"
        self.log.touch()
        self.token = "sample"
        self.now = time.monotonic_ns()

    def row(self, index, kind="thumbnail", **changes):
        return dict(token=self.token, path=str(self.files[index]), kind=kind,
                    start_ns=self.now - 300_000_000, end_ns=self.now - 200_000_000 + index * 1_000_000,
                    worker_ms=20 + index, width=256 if kind == "thumbnail" else 800,
                    height=170 if kind == "thumbnail" else 533, bounds=[10, 10, 256, 170], frame=5, clock=1, **changes)

    def write(self, rows):
        self.log.write_text("".join("FLEA-B7 " + json.dumps(row) + "\n" for row in rows))

    def reader(self):
        reader = gpui.Receipts(self.log, self.token, self.fixture)
        reader.poll()
        return reader

    def test_bench_thumbs_reader_output_identity_and_edge_are_required(self):
        rows = [self.row(i) for i in range(16)]
        self.write(rows)
        self.assertEqual(self.reader().metrics(16)["all_thumbnails_ms"], 115)
        self.assertEqual(self.reader().metrics(16)["per_thumbnail_ms"], 27.5)
        self.assertIsNone(self.reader().metrics(15)["all_thumbnails_ms"])
        self.write(rows[:-1])
        self.assertIsNone(self.reader().metrics(16)["all_thumbnails_ms"])
        self.write([*rows, rows[0]])
        with self.assertRaisesRegex(ValueError, "duplicate"):
            self.reader()
        for key, value in [("clock", 2), ("end_ns", 1), ("frame", 0), ("bounds", [0, 0, 0, 5]), ("path", "/foreign.jpg")]:
            bad = dict(rows[0], **{key: value})
            self.write([bad, *rows[1:]])
            with self.assertRaises(ValueError):
                self.reader()

    def test_bench_thumbs_reader_never_promotes_diagnostics_or_foreign_tokens(self):
        self.log.write_text("pixel-change complete\nPREVIEW-QUALITY {}\nflea: trace row=0 total=12\n")
        self.assertIsNone(self.reader().metrics(16)["all_thumbnails_ms"])
        self.write([dict(self.row(i), token="foreign") for i in range(16)])
        self.assertIsNone(self.reader().metrics(16)["all_thumbnails_ms"])

    def test_bench_thumbs_reader_preview_and_delay_mutant_follow_presented_output(self):
        rows = [self.row(i) for i in range(16)] + [self.row(i, "preview") for i in range(16)]
        self.write(rows)
        first = self.reader().metrics(16)
        self.assertEqual(first["preview_800_ms"], 107.5)
        self.write([dict(row, end_ns=row["end_ns"] + 1_000_000_000) for row in rows])
        delayed = self.reader().metrics(16)
        self.assertEqual(delayed["all_thumbnails_ms"] - first["all_thumbnails_ms"], 1000)
        self.assertEqual(delayed["preview_800_ms"] - first["preview_800_ms"], 1000)
        self.write([*rows[:16], *[dict(row, width=799) for row in rows[16:]]])
        with self.assertRaisesRegex(ValueError, "800 px"):
            self.reader()

    def test_bench_thumbs_reader_complete_cgroup_receipt_is_mandatory(self):
        scope = cgroup_cpu.Scope(self.root)
        self.assertIsNone(scope.result())
        path = self.root / "cgroup-cpu.json"
        receipt = dict(boundary="private-session-command", start_user_usec=100, end_user_usec=300100, status=0, remaining_pids=[])
        path.write_text(json.dumps(receipt))
        self.assertEqual(scope.result(), 0.3)
        for change in [dict(remaining_pids=[44]), dict(status=1), dict(end_user_usec=100), dict(end_user_usec=0)]:
            path.write_text(json.dumps(dict(receipt, **change)))
            self.assertIsNone(scope.result())

    def test_bench_thumbs_reader_cpu_starts_after_sandbox_and_before_private_bus(self):
        scope = cgroup_cpu.Scope(self.root)
        command = scope.command(["dbus-run-session", "--", "/candidate"], {"FLEA_B7_BENCH": "sample"}, ["bwrap", "--unshare-pid"])
        self.assertLess(command.index("bwrap"), command.index("--start"))
        self.assertLess(command.index("--start"), command.index("dbus-run-session"))
        self.assertEqual(command[-3:], ["dbus-run-session", "--", "/candidate"])

    def test_bench_thumbs_reader_flea_reaches_collector_and_certifies_outputs(self):
        self.run_candidate()

    def test_bench_thumbs_reader_missing_presented_output_invalidates_run(self):
        self.run_candidate(missing=True)

    def run_candidate(self, missing=False):
        binary = self.root / "flea"
        binary.write_text("stub binary")
        sibling = self.root / "flea-gui"
        sibling.write_text("stub gui")
        from inventory import digest
        entrant = dict(id="flea", binary=str(binary), sha256=digest(binary), gui_sha256=digest(sibling), version="0.4.0")
        test = self
        launched = {}
        process = types.SimpleNamespace(pid=os.getpid(), poll=lambda: None, wait=lambda **kw: 0)

        def scope_command(scope, argv, env, prefix):
            launched.update(root=scope.root, argv=prefix + argv, env=env)
            return prefix + argv

        def launch(argv, **kwargs):
            self.log = launched["root"] / "app.log"
            self.token = launched["env"]["FLEA_B7_BENCH"]
            self.write([self.row(i) for i in range(15 if missing else 16)])
            (launched["root"] / "cgroup-cpu.json").write_text(json.dumps(dict(boundary="private-session-command", start_user_usec=0, end_user_usec=400000, status=0, remaining_pids=[])))
            return process

        class Display:
            socket = test.root / "wayland"

            def wait_window(self, *args):
                return dict(address="stub", pid=os.getpid())

            def focus(self, *args):
                pass

            def key(self, window, token, *keys):
                if "space" in keys:
                    self.index = 0
                elif "Right" in keys:
                    self.index += 1
                else:
                    return
                with test.log.open("a") as stream:
                    stream.write("FLEA-B7 " + json.dumps(test.row(self.index, "preview")) + "\n")

            def screenshot(self, root, window, token, name):
                (root / f"{name}.png").write_bytes(b"stub capture")

        with mock.patch.object(cgroup_cpu.Scope, "command", scope_command), mock.patch.object(cgroup_cpu.Scope, "stop"), \
             mock.patch.object(measure.version_probe, "probe", return_value=dict(version="0.4.0", version_output="flea 0.4.0", version_method="cli")), \
             mock.patch.object(measure.subprocess, "run", return_value=types.SimpleNamespace(returncode=1)), \
             mock.patch.object(measure.subprocess, "Popen", side_effect=launch), mock.patch.object(measure, "stop"), \
             mock.patch.object(measure, "quit_entrant", return_value="close"), \
             mock.patch.object(measure, "settled_count", return_value=16), mock.patch.object(measure, "verified_cache", return_value=set(range(16))):
            result = measure.run_one(REPO, self.root, Display(), self.fixture, entrant, 1, 0.01)
        if missing:
            self.assertIsNone(result["all_thumbnails_ms"])
            self.assertIsNone(result["user_cpu_s"])
            self.assertIn("GPUI did not present", " ".join(result["reasons"]))
        else:
            self.assertEqual(result["all_thumbnails_ms"], 115, result)
            self.assertEqual(result["per_thumbnail_ms"], 27.5)
            self.assertEqual(result["preview_800_ms"], 107.5)
            self.assertEqual(result["user_cpu_s"], 0.4)
            self.assertIn("dbus-run-session", launched["argv"])
            self.assertTrue((Path(result["evidence"]) / "preview-15-receipt.json").is_file())


if __name__ == "__main__":
    unittest.main()
