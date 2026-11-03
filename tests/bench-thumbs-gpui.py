"""B7 candidate collector contracts. These stubs make no native timing claim."""

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

    def test_bench_thumbs_reader_batch_and_preview_follow_presented_receipts(self):
        rows = [self.row(i) for i in range(16)] + [self.row(i, "preview") for i in range(16)]
        self.write(rows)
        first = self.reader().metrics(16)
        self.assertEqual(first["preview_800_ms"], 107.5)
        # Rust withholds every receipt until the first paint plus the delay; modelled here on end_ns.
        until = rows[0]["end_ns"] + 1_000_000_000
        self.write([dict(row, end_ns=max(row["end_ns"], until)) if row["kind"] == "thumbnail" else row for row in rows])
        delayed = self.reader().metrics(16)
        self.assertEqual(first["all_thumbnails_ms"], 115)
        self.assertEqual(delayed["all_thumbnails_ms"], 1100)
        self.assertEqual(delayed["preview_800_ms"], 107.5)
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

    def test_bench_thumbs_cgroup_membership_names_only_the_b7_scope(self):
        line = "0::/user.slice/user-1000.slice/user@1000.service/app.slice/flea-b7-sample.scope\n"
        self.assertEqual(str(cgroup_cpu.scope_group(line)), "/sys/fs/cgroup/" + line.strip().removeprefix("0::/"))
        for membership in ["0::/user.slice/app.slice/other.scope\n", "0::/user.slice/flea-b7-sample.service\n", line + line, "1:name=systemd:/flea-b7-a.scope\n"]:
            with self.assertRaises(ValueError):
                cgroup_cpu.scope_group(membership)

    def fake_group(self):
        group = self.root / "group"
        group.mkdir(exist_ok=True)
        (group / "cpu.stat").write_text("usage_usec 300\nuser_usec 200\nsystem_usec 100\n")
        (group / "cgroup.procs").write_text(f"{os.getpid()}\n")
        return group

    def collect(self, group, lingering=(), helper=None):
        # Stands in for mark_start: the start marker, the namespace record and the session's own CPU.
        script = (f"import json, pathlib, time; root = pathlib.Path({str(self.root)!r}); group = pathlib.Path({str(group)!r})\n"
                  "(root / 'cgroup-start.json').write_text(json.dumps(dict(cgroup=str(group), user_usec=200, at_ns=time.monotonic_ns())))\n"
                  f"(root / 'cgroup-namespace.json').write_text(json.dumps(dict(status=0, remaining_pids={list(lingering)!r})))\n"
                  "(group / 'cpu.stat').write_text('usage_usec 500300\\nuser_usec 400200\\nsystem_usec 100100\\n')\n")
        if helper:
            script += f"(group / 'cgroup.procs').write_text('{os.getpid()}\\n{helper}\\n')\n"
        with mock.patch.object(cgroup_cpu, "current_group", return_value=group), mock.patch.object(cgroup_cpu, "GRACE_S", 0.05):
            return cgroup_cpu.collect(str(self.root / "cgroup-cpu.json"), {"PATH": os.environ["PATH"]}, [sys.executable, "-B", "-c", script])

    def test_bench_thumbs_cgroup_collector_certifies_only_a_drained_scope(self):
        self.assertEqual(self.collect(self.fake_group()), 0)
        self.assertEqual(cgroup_cpu.Scope(self.root).result(), 0.4)
        self.assertEqual(self.collect(self.fake_group(), lingering=[7]), 1)
        self.assertEqual(json.loads((self.root / "cgroup-cpu.json").read_text())["remaining_pids"], ["sandbox:7"])
        self.assertIsNone(cgroup_cpu.Scope(self.root).result())
        self.assertEqual(self.collect(self.fake_group(), helper=99999), 1)
        self.assertEqual(json.loads((self.root / "cgroup-cpu.json").read_text())["remaining_pids"], ["99999"])
        self.assertIsNone(cgroup_cpu.Scope(self.root).result())

    def test_bench_thumbs_cgroup_collector_refuses_a_busy_scope(self):
        group = self.fake_group()
        (group / "cgroup.procs").write_text(f"{os.getpid()}\n4242\n")
        with self.assertRaisesRegex(ValueError, "not empty"):
            self.collect(group)
        self.assertFalse((self.root / "cgroup-cpu.json").exists())

    def test_bench_thumbs_cgroup_start_marker_refuses_the_host_pid_namespace(self):
        group = self.fake_group()
        with mock.patch.object(cgroup_cpu, "current_group", return_value=group), mock.patch.object(cgroup_cpu.subprocess, "call") as call:
            with self.assertRaisesRegex(ValueError, "bwrap pid namespace"):
                cgroup_cpu.mark_start(str(self.root / "cgroup-start.json"), ["true"])
        call.assert_not_called()
        self.assertFalse((self.root / "cgroup-start.json").exists())

    def test_bench_thumbs_cgroup_start_marker_sees_helpers_bwrap_would_kill(self):
        group = self.fake_group()
        lib = REPO / "tools/flea-bench-thumbs-lib"
        for session, status, lingering in [("exit 0", 0, 0), ("sleep 30 & exit 3", 3, 1)]:
            script = (f"import sys; sys.path.insert(0, {str(lib)!r}); import pathlib, cgroup_cpu\n"
                      f"cgroup_cpu.current_group = lambda: pathlib.Path({str(group)!r}); cgroup_cpu.GRACE_S = 0.2\n"
                      f"sys.exit(cgroup_cpu.mark_start({str(self.root / 'cgroup-start.json')!r}, ['sh', '-c', {session!r}]))")
            prefix = measure.sandbox(self.root)
            result = subprocess.run([*prefix, sys.executable, "-B", "-c", script], timeout=20)
            self.assertEqual(result.returncode, status)
            start = json.loads((self.root / "cgroup-start.json").read_text())
            self.assertEqual((start["cgroup"], start["user_usec"]), (str(group), 200))
            inside = json.loads((self.root / "cgroup-namespace.json").read_text())
            self.assertEqual((inside["status"], len(inside["remaining_pids"])), (status, lingering))

    def test_bench_thumbs_reader_flea_reaches_collector_and_certifies_outputs(self):
        self.run_candidate()

    def test_bench_thumbs_reader_missing_presented_output_invalidates_run(self):
        self.run_candidate(missing=True)

    def test_bench_thumbs_reader_late_bad_receipt_is_a_reason_not_a_crash(self):
        self.run_candidate(late=True)

    def run_candidate(self, missing=False, late=False):
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

        def quit_entrant(*args):
            if late:
                with self.log.open("a") as stream:
                    stream.write("FLEA-B7 " + json.dumps(self.row(0, "preview")) + "\n")
            return "close"

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
             mock.patch.object(measure, "quit_entrant", side_effect=quit_entrant), \
             mock.patch.object(measure, "settled_count", return_value=16), mock.patch.object(measure, "verified_cache", return_value=set(range(16))):
            result = measure.run_one(REPO, self.root, Display(), self.fixture, entrant, 1, 0.01)
        if late:
            self.assertIsNone(result["all_thumbnails_ms"])
            self.assertIsNone(result["user_cpu_s"])
            self.assertIn("duplicate B7 output receipt", result["reasons"])
        elif missing:
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
