"""Offline execution of the real early-lane driver with native commands stubbed."""

import datetime
import hashlib
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import tempfile
import types
import unittest
from unittest import mock

REPO = Path(__file__).resolve().parent.parent
loader = importlib.machinery.SourceFileLoader("bench_thumbs_native", str(REPO / "tools/flea-bench-thumbs"))
spec = importlib.util.spec_from_loader(loader.name, loader)
bench = importlib.util.module_from_spec(spec)
loader.exec_module(bench)
ROOT = Path(os.environ["FLEA_B7_TEST_ROOT"])


class NativeLane(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="native-stubs-", dir=ROOT))
        (self.root / ".flea-test-sandbox").touch()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.runtime = self.root / "session"
        self.runtime.mkdir()
        descriptor = os.open(self.runtime, os.O_RDONLY | os.O_DIRECTORY)
        self.listener = socket.socket(socket.AF_UNIX)
        self.listener.bind(f"/proc/self/fd/{descriptor}/wayland-test")
        os.close(descriptor)
        self.addCleanup(self.listener.close)
        source = '#!/usr/bin/python3\n' + (REPO / "tests/bench-thumbs-stub.py").read_text()
        for name in ("bwrap", "hyprctl", "grim", "wtype", "perf", "dbus-run-session", "pacman"):
            path = self.bin / name
            path.write_text(source)
            path.chmod(0o700)
        self.b5 = self.root / "b5"
        self.b5.mkdir()
        (self.b5 / ".flea-test-sandbox").touch()
        binaries = {}
        archives = []
        packages = {}
        self.paths = {}
        for ident in bench.IDS:
            if ident == "flea":
                continue
            version = "0.3.12" if ident == "flea-0312" else "1.0.0" if ident == "strata-stable" else "1.0.1-nightly.20261018" if ident == "strata-nightly" else "2.9.0"
            path = self.b5 / "entrants" / ident / "strata" if ident.startswith("strata") else self.bin / ("flea" if ident == "flea-0312" else ident)
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(source)
            path.chmod(0o700)
            output = ("strata" if ident.startswith("strata") else "flea" if ident == "flea-0312" else ident) + " " + version
            self.paths[ident] = path
            binaries[str(path)] = dict(id=ident, version_output=output)
            if ident.startswith("strata"):
                embedded, tag = "1" * 40, "2" * 40
                (path.parent / "SOURCE_COMMIT").write_text(embedded + "\n")
                archives.append(dict(id=ident, binary_path=str(path), binary_sha256=bench.digest(path), sha256="a" * 64,
                                     url=f"https://example.invalid/{ident}.tar.gz", tag="v" + version, expected_version=output,
                                     source_commit=embedded, tag_commit=tag, version_check=dict(rc=0, stdout=output)))
            elif ident != "flea-0312":
                packages[ident] = dict(realpath=str(path), sha256=bench.digest(path), package=dict(rc=0, stdout=ident + " " + version + "-2"))
        (self.root / ".b7-offline.json").write_text(json.dumps(dict(binaries=binaries, driver=str(REPO / "tools/flea-bench-thumbs"))))
        (self.b5 / "rival-versions.json").write_text(json.dumps(dict(date_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(), archives=archives, packages=packages)))
        env = dict(PATH=str(self.bin) + ":/usr/bin:/bin", XDG_RUNTIME_DIR=str(self.runtime), WAYLAND_DISPLAY="wayland-test",
                   HYPRLAND_INSTANCE_SIGNATURE="stub-instance", FLEA_0312_BIN=str(self.paths["flea-0312"]),
                   STRATA_STABLE_BIN=str(self.paths["strata-stable"]), STRATA_NIGHTLY_BIN=str(self.paths["strata-nightly"]))
        self.environment = mock.patch.dict(os.environ, env)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.runtime_patch = mock.patch.object(bench.wayland, "NATIVE_RUNTIME", self.runtime)
        self.runtime_patch.start()
        self.addCleanup(self.runtime_patch.stop)

    def roster(self):
        rows = bench.inventory.installed(self.root, self.b5, bench.IDS)
        path = self.root / "roster.json"
        path.write_text(json.dumps(rows))
        return rows, path

    def display_failure(self, ids):
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        failure = "nautilus-application-Message: Failed to initialize display server connection: Unsupported or missing session type"
        for row in meta["binaries"].values():
            if row["id"] in ids:
                row["version_failure"] = failure
        path.write_text(json.dumps(meta))

    def test_recorded_nautilus_display_failure_uses_package_inventory(self):
        self.display_failure({"nautilus"})
        rows, _ = self.roster()
        row = next(row for row in rows if row["id"] == "nautilus")
        self.assertEqual(row["version_method"], "package")
        self.assertEqual(row["version_output"], "nautilus 2.9.0-2")
        commands = [json.loads(line) for line in (self.root / "stub-commands.jsonl").read_text().splitlines()]
        self.assertFalse(any(row["kind"] == "version-call" and row["id"] == "nautilus" for row in commands))

    def test_package_epoch_pkgrel_and_receipt_disagreement_are_checked(self):
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        meta["binaries"][str(self.paths["nautilus"])]["package_version"] = "1:49.1-3"
        path.write_text(json.dumps(meta))
        receipt = self.b5 / "rival-versions.json"
        data = json.loads(receipt.read_text())
        data["packages"]["nautilus"]["package"]["stdout"] = "nautilus 1:49.1-3"
        receipt.write_text(json.dumps(data))
        rows, _ = self.roster()
        row = next(row for row in rows if row["id"] == "nautilus")
        self.assertEqual(row["version"], "49.1")
        self.assertEqual(row["package_version"], "1:49.1-3")
        meta["binaries"][str(self.paths["nautilus"])]["package_version"] = "1:49.1-4"
        path.write_text(json.dumps(meta))
        with self.assertRaisesRegex(ValueError, "package version differs"):
            bench.inventory.installed(self.root, self.b5, bench.IDS)
        with self.assertRaisesRegex(ValueError, "package version differs"):
            bench.measure.run_one(REPO, self.root, None, self.root / "unused-fixture", row, 1, 2)

    def test_real_samples_and_archive_receipts_without_an_x_server(self):
        packaged = set(bench.IDS) - {"flea", "flea-0312", "strata-stable", "strata-nightly"}
        self.display_failure(packaged)
        rows, path = self.roster()
        self.assertEqual(len(rows), 9)
        self.assertEqual(bench.roster(path, installed=True), rows)
        for row in rows:
            self.assertEqual(json.loads(Path(row["b5_receipt"]["path"]).read_text()), json.loads((self.b5 / "rival-versions.json").read_text()))
            if row["id"].startswith("strata"):
                self.assertNotEqual(row["archive"]["source_commit"], row["archive"]["tag_commit"])
                self.assertEqual(row["source"], "official release archive")
        fixture = self.root / "photo-jpeg"
        fixture.mkdir()
        for index in range(16):
            (fixture / f"sample-{index:02d}.jpg").write_bytes(b"offline source placeholder")
        evidence = []
        with bench.wayland.Display(self.root) as display:
            self.assertEqual(display.metadata()["transport"], "Hyprland/Wayland")
            for ident in (ident for ident in bench.IDS if ident != "flea"):
                row = next(row for row in rows if row["id"] == ident)
                result = bench.measure.run_one(REPO, self.root, display, fixture, row, 1, 2)
                self.assertEqual(result["version_identity"]["version_method"], "package" if ident in packaged else "cli")
                self.assertEqual(result["version_identity"]["version"], row["version"])
                self.assertEqual(result["verified"], 0 if ident in ("krusader", "dolphin", "thunar", "pcmanfm") else 16, result)
                self.assertEqual([reason for reason in result["reasons"] if "exit status" in reason or "Hyprland" in reason or "focus" in reason], [])
                self.assertEqual(result["user_cpu_s_diagnostic"], 1.234)
                self.assertIsNone(result["preview_800_ms"])
                self.assertEqual([reason for reason in result["reasons"] if "did not quit" in reason or "no source-equivalent" in reason], [])
                self.assertEqual(result["quit_method"], "chord" if ident in ("krusader", "dolphin", "nautilus") else "close", ident)
                self.assertEqual(result["all_thumbnails_settle_ms"] is None, ident in ("krusader", "dolphin", "thunar", "pcmanfm"), ident)
                self.assertEqual(sum(result["cache_written"].values()), 16, ident)
                if ident in ("krusader", "dolphin", "thunar", "pcmanfm", "nemo", "nautilus"):
                    self.assertEqual(result["cache_written"], {"large/256": 16} if ident in ("nemo", "nautilus") else {"normal/128": 16}, ident)
                    self.assertTrue(result["thumbnail_method"].startswith("cache-settle"), ident)
                    self.assertIsNone(result["all_thumbnails_ms"])
                    self.assertIsNone(result["declined"], ident)
                if ident in ("krusader", "dolphin"):
                    self.assertTrue((Path(result["evidence"]) / "cache/thumbnails").is_dir(), ident)
                config = Path(result["evidence"]) / "config"
                self.assertFalse((config / "kdeglobals").exists() or (config / "libfm").exists(), ident)
                if ident == "flea-0312":
                    self.assertTrue(result["thumbnail_method"].startswith("flea thumb trace"))
                    self.assertGreater(result["all_thumbnails_ms"], 0)
                    self.assertGreater(result["per_thumbnail_ms"], 0)
                    trace = (Path(result["evidence"]) / "trace.log").read_text().splitlines()
                    self.assertEqual(len(trace), 16)
                    self.assertTrue(all(line.startswith("flea: trace row=") for line in trace))
                if ident.startswith("strata"):
                    self.assertEqual(result["per_thumbnail_ms"], 12)
                    self.assertTrue((Path(result["evidence"]) / "preview-15.png").is_file())
                if ident == "krusader":
                    self.assertIn("--left", result["argv"])
                    self.assertIn("--right", result["argv"])
                    self.assertIn("ShowPreviews=true", (Path(result["evidence"]) / "config/krusaderrc").read_text())
                evidence.append(dict(id=ident, **result))
        commands = [json.loads(line) for line in (self.root / "stub-commands.jsonl").read_text().splitlines()]
        self.assertTrue(any(row["kind"] == "grim" for row in commands))
        dispatches = [row["argv"] for row in commands if row["kind"] == "hyprctl" and row["argv"][:1] == ["dispatch"]]
        focus = 'hl.dsp.focus({ window = "address:0xb700" })'
        close = 'hl.dsp.window.close({ window = "address:0xb700" })'
        self.assertEqual(sorted(argv[1] for argv in dispatches), sorted([focus] * 9 + [close] * 6), dispatches)
        self.assertTrue(all(len(argv) == 2 for argv in dispatches), dispatches)
        self.assertTrue(any(row["kind"] == "wtype" for row in commands))
        self.assertFalse(any(row["kind"] in ("Xvfb", "xdotool", "xwd") for row in commands))
        for ident in (ident for ident in bench.IDS if ident != "flea"):
            window = [json.loads(path.read_text()) for path in self.root.glob(f"photo-jpeg-{ident}-1-*/window.json")]
            self.assertEqual([(row["address"], row["token"].startswith(f"photo-jpeg-{ident}-1-")) for row in window], [("0xb700", True)], ident)
        homes = [row["environment"]["HOME"] for row in commands if row["kind"] == "app-environment"]
        self.assertEqual(len(set(homes)), 9)
        cli_versions = {row["id"] for row in commands if row["kind"] == "version-call"}
        self.assertEqual(cli_versions, {"flea-0312", "strata-stable", "strata-nightly"})
        for ident in packaged:
            queries = [row for row in commands if row["kind"] == "pacman" and row["argv"] == ["-Q", "--", ident]]
            self.assertEqual(len(queries), 2, ident)
        (self.root / "OFFLINE-ONLY.json").write_text(json.dumps(dict(simulated=True, rows=evidence), indent=2))
        print("offline sample evidence:", self.root / "OFFLINE-ONLY.json")
        proof = dict(simulated=True, private_homes=len(set(homes)), samples=[
            dict(id=row["id"], verified=row["verified"], version=row["version_identity"]["version"]) for row in evidence])
        print("OFFLINE_B7_IDENTITIES=" + json.dumps(proof, sort_keys=True))

    def one_sample(self, ident):
        rows, _ = self.roster()
        fixture = self.root / "photo-jpeg"
        fixture.mkdir(exist_ok=True)
        for index in range(16):
            (fixture / f"sample-{index:02d}.jpg").write_bytes(b"offline source placeholder")
        row = next(row for row in rows if row["id"] == ident)
        with bench.wayland.Display(self.root) as display:
            return bench.measure.run_one(REPO, self.root, display, fixture, row, 1, 4)

    def big_fixture(self, name, size):
        fixture = self.root / name
        fixture.mkdir()
        for index in range(16):
            (fixture / f"sample-{index:02d}.jpg").write_bytes(b"x" * size)
        return fixture

    def sample_on(self, ident, fixture, timeout=4):
        rows, _ = self.roster()
        row = next(row for row in rows if row["id"] == ident)
        with bench.wayland.Display(self.root) as display:
            return bench.measure.run_one(REPO, self.root, display, fixture, row, 1, timeout)

    def test_a_kio_entrant_saves_nothing_without_a_thumbnail_root_and_the_lane_provides_it(self):
        # Red evidence for the FIX4 run: Krusader showed previews but wrote 0 of 16 because cache/thumbnails did not exist.
        fixture = self.big_fixture("kio-fixture", 10)
        for ident in ("krusader", "dolphin"):
            with self.subTest(ident=ident):
                result = self.sample_on(ident, fixture)
                self.assertEqual(result["verified"], 0, result["reasons"])
                self.assertEqual(result["cache_written"], {"normal/128": 16})
        with mock.patch.object(bench.measure, "prepare_rival_state", lambda root, ident: None):
            result = self.sample_on("krusader", fixture)
        self.assertEqual(result["verified"], 0)
        self.assertEqual(result["cache_written"], {})
        self.assertTrue(any(reason.startswith("0/16 valid fixture cache entries") for reason in result["reasons"]))
        self.assertIsNone(result["declined"])

    def test_the_lane_writes_no_setting_for_any_rival(self):
        fixture = self.big_fixture("settings-fixture", 10)
        for ident in ("krusader", "dolphin", "pcmanfm", "thunar", "nemo"):
            result = self.sample_on(ident, fixture)
            config = Path(result["evidence"]) / "config"
            self.assertFalse((config / "kdeglobals").exists(), ident)
            self.assertFalse((config / "libfm").exists(), ident)
            self.assertFalse((config / "dconf").exists() and any((config / "dconf").iterdir()), ident)

    def test_nemo_over_its_default_thumbnail_limit_is_recorded_as_declined_not_failed(self):
        fixture = self.big_fixture("big-jpeg", 1048576 + 1)
        result = self.sample_on("nemo", fixture)
        self.assertEqual(result["verified"], 0)
        self.assertEqual(result["declined"]["limit_bytes"], 1048576)
        self.assertEqual(result["declined"]["smallest_fixture_bytes"], 1048577)
        self.assertIn("libnemo-private/org.nemo.gschema.xml:267-268", result["declined"]["source"])
        self.assertTrue(any(reason.startswith("declined: org.nemo.preferences thumbnail-limit default 1048576 bytes") for reason in result["reasons"]), result["reasons"])
        self.assertEqual([reason for reason in result["reasons"] if "valid fixture cache entries" in reason or "app.log tail" in reason], [])
        self.assertEqual(result["quit_method"], "close")

    def test_nemo_at_or_under_its_limit_is_measured_and_a_non_nemo_zero_stays_a_failure(self):
        small = self.big_fixture("small-jpeg", 1048576)
        result = self.sample_on("nemo", small)
        self.assertEqual((result["verified"], result["declined"]), (16, None))
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        meta["silent"] = ["thunar"]
        path.write_text(json.dumps(meta))
        result = self.sample_on("thunar", self.big_fixture("big-for-thunar", 1048576 + 1))
        self.assertEqual(result["verified"], 0)
        self.assertIsNone(result["declined"])
        self.assertTrue(any("0/16 valid fixture cache entries" in reason for reason in result["reasons"]))

    def test_a_declined_rival_stays_incomplete_and_blocks_first_place(self):
        names = ["nemo", "strata-stable"]
        declined = dict(setting="org.nemo.preferences thumbnail-limit", limit_bytes=1048576, smallest_fixture_bytes=2871481, source="x")
        rows = []
        for round_number in range(1, 7):
            rows.append(dict(id="nemo", fixture="photo-jpeg", round=round_number, all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None,
                             all_thumbnails_settle_ms=None, user_cpu_s=None, declined=declined))
            rows.append(dict(id="strata-stable", fixture="photo-jpeg", round=round_number, verified=16, all_thumbnails_ms=300, per_thumbnail_ms=20, preview_800_ms=None,
                             all_thumbnails_settle_ms=250.0, user_cpu_s=None, declined=None))
        with mock.patch.object(bench, "IDS", names), mock.patch.object(bench, "SPECS", {"photo-jpeg": bench.SPECS["photo-jpeg"]}):
            result = bench.summary(rows)
        by_id = {row["id"]: row for row in result}
        self.assertEqual(by_id["nemo"]["declined"]["limit_bytes"], 1048576)
        self.assertIsNone(by_id["strata-stable"]["declined"])
        self.assertTrue(by_id["strata-stable"]["metrics"]["all_thumbnails_ms"]["measured"])
        self.assertFalse(by_id["strata-stable"]["metrics"]["all_thumbnails_ms"]["first"])
        self.assertFalse(by_id["nemo"]["metrics"]["all_thumbnails_ms"]["measured"])
        rows_no_decline = [dict(row, declined=None) for row in rows]
        with mock.patch.object(bench, "IDS", names), mock.patch.object(bench, "SPECS", {"photo-jpeg": bench.SPECS["photo-jpeg"]}):
            self.assertFalse({row["id"]: row for row in bench.summary(rows_no_decline)}["strata-stable"]["metrics"]["all_thumbnails_ms"]["first"])

    def test_a_partly_small_fixture_is_never_predicted_declined(self):
        mixed = self.big_fixture("mixed", 1048576 + 1)
        (mixed / "sample-00.jpg").write_bytes(b"x")
        self.assertIsNone(bench.measure.declined_by_default("nemo", mixed))
        self.assertIsNone(bench.measure.declined_by_default("thunar", self.big_fixture("any-thunar", 9999999)))

    def test_an_entrant_that_ignores_both_quit_actions_is_reported_as_not_quit(self):
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        meta["close_ignored"] = ["thunar"]
        path.write_text(json.dumps(meta))
        with mock.patch.object(bench.measure, "QUIT_WAIT", 0.3):
            result = self.one_sample("thunar")
        self.assertTrue(any("entrant did not quit within 0.6 s (close+chord)" in reason for reason in result["reasons"]), result["reasons"])
        self.assertIsNone(result["quit_method"])
        self.assertEqual(result["verified"], 0)

    def test_a_close_that_is_ignored_escalates_to_the_chord_for_a_chord_entrant(self):
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        meta["close_ignored"] = ["nemo"]
        path.write_text(json.dumps(meta))
        with mock.patch.object(bench.measure, "QUIT_WAIT", 0.2):
            result = self.one_sample("nemo")
        self.assertIsNone(result["quit_method"])
        closes = (self.root / "closes.jsonl").read_text().splitlines()
        self.assertEqual(len(closes), 1)
        keys = [json.loads(line)["key"] for line in (self.root / "keys.jsonl").read_text().splitlines()]
        self.assertIn("q", keys)

    def test_the_trace_seam_sends_only_the_backend_stderr_to_the_trace_file(self):
        binary = self.root / "fake flea $(touch escaped)"
        binary.write_text('#!/bin/sh\necho "out:$*"\necho "err:$*" >&2\n')
        binary.chmod(0o700)
        seam = bench.measure.write_trace_seam(self.root, str(binary))
        backend = subprocess.run([str(seam), "--backend"], capture_output=True, text=True, timeout=5)
        other = subprocess.run([str(seam), "--gui", "x"], capture_output=True, text=True, timeout=5)
        self.assertEqual((backend.stdout, backend.stderr), ("out:--backend\n", ""))
        self.assertEqual((self.root / "trace.log").read_text(), "err:--backend\n")
        self.assertEqual((other.stdout, other.stderr), ("out:--gui x\n", "err:--gui x\n"))

    def test_a_rival_normal_flavor_thumbnail_counts_at_its_native_size_and_a_wrong_uri_does_not(self):
        fixture = self.root / "fx"
        fixture.mkdir()
        cache = self.root / "cache-probe"
        (cache / "thumbnails/normal").mkdir(parents=True)
        stub = (REPO / "tests/bench-thumbs-stub.py").read_text()
        namespace = {}
        exec("import struct, zlib\n" + stub[stub.index("def png("):stub.index("# Entrants")], namespace)
        for index in range(16):
            source = fixture / f"s{index}.jpg"
            source.write_bytes(b"x")
            name = hashlib.md5(source.as_uri().encode()).hexdigest()
            uri = source.as_uri() if index != 3 else "file:///elsewhere.jpg"
            (cache / "thumbnails/normal" / f"{name}.png").write_bytes(namespace["png"](128, 96, uri))
        self.assertEqual(len(bench.measure.verified_cache(cache, fixture, None)), 15)
        self.assertEqual(len(bench.measure.verified_cache(cache, fixture, 256)), 0)
        self.assertEqual(bench.measure.settled_count(cache, bench.measure.expected_stems(fixture)), 16)

    def test_trace_metrics_need_sixteen_distinct_jobs_and_sixteen_verified(self):
        jobs = [dict(row=n, queued=1.0, total=21.0, seen_ms=1000.0 + n * 5) for n in range(16)]
        metrics = bench.measure.trace_metrics(jobs, 16)
        self.assertEqual(metrics["per_thumbnail_ms"], 20.0)
        self.assertEqual(metrics["all_thumbnails_ms"], 1075.0 - (1000.0 - 21.0))
        for bad_jobs, verified in ((jobs[:15], 16), (jobs, 15), ([*jobs[:15], dict(jobs[0])], 16)):
            self.assertIsNone(bench.measure.trace_metrics(bad_jobs, verified)["all_thumbnails_ms"])

    def stub(self, *arguments):
        return subprocess.run([str(self.bin / arguments[0]), *arguments[1:]], capture_output=True, text=True, timeout=10)

    def test_fake_compositor_rejects_the_legacy_dispatchers_with_exit_7(self):
        for legacy in (("focuswindow", "address:0xb700"), ("sendshortcut", "CTRL,3,address:0xb700"), ("tagwindow", "+t address:0xb700")):
            result = self.stub("hyprctl", "dispatch", *legacy)
            self.assertEqual(result.returncode, 7, legacy)
            self.assertIn("')' expected near", result.stderr)
        miss = self.stub("hyprctl", "dispatch", 'hl.dsp.focus({ window = "address:0xb700" })')
        self.assertEqual((miss.returncode, miss.stdout.strip()), (0, "warning: =[C]:-1: hl.focus: window not found"))

    def test_stale_clients_beside_the_owned_window_are_ignored(self):
        (self.root / "stale-clients.json").write_text(json.dumps([
            dict(address="0x564ba3c0d8f0", pid=os.getpid(), xwayland=False, **{"class": "stale-one"}),
            dict(address="0xb700", pid=os.getppid(), xwayland=False, **{"class": "stale-two"})]))
        rows, _ = self.roster()
        fixture = self.root / "photo-jpeg"
        fixture.mkdir()
        for index in range(16):
            (fixture / f"sample-{index:02d}.jpg").write_bytes(b"offline source placeholder")
        row = next(row for row in rows if row["id"] == "nautilus")
        with bench.wayland.Display(self.root) as display:
            result = bench.measure.run_one(REPO, self.root, display, fixture, row, 1, 2)
        self.assertEqual(result["verified"], 16, result)
        self.assertEqual([reason for reason in result["reasons"] if "exit status" in reason or "Hyprland" in reason or "focus" in reason], [])
        self.assertEqual(result["user_cpu_s_diagnostic"], 1.234)
        window = json.loads((Path(result["evidence"]) / "window.json").read_text())
        self.assertEqual((window["address"], window["class"]), ("0xb700", "nautilus"))
        self.assertNotEqual(window["pid"], os.getppid())
        commands = [json.loads(line) for line in (self.root / "stub-commands.jsonl").read_text().splitlines()]
        self.assertFalse(any(row["kind"] == "hyprctl" and row["argv"][:1] == ["dispatch"] and "stale" in " ".join(row["argv"]) for row in commands))

    def test_display_lock_and_foreign_focus_refuse_input(self):
        with bench.wayland.Display(self.root) as display:
            with self.assertRaises(BlockingIOError):
                with bench.wayland.Display(self.root):
                    self.fail("second display lane acquired the lock")
            with self.assertRaises(ValueError):
                display.key(dict(address="0xdead"), "foreign-token", "ctrl+q")
        commands = (self.root / "stub-commands.jsonl").read_text()
        self.assertNotIn('"kind": "wtype"', commands)

    def test_listed_cli_runs_the_early_schedule_offline(self):
        field = self.bin / "flea-field-bench"
        field.write_text('#!/usr/bin/python3\n' + (REPO / "tests/bench-thumbs-stub.py").read_text())
        field.chmod(0o700)
        roster = self.root / "cli-roster.json"
        for command in ("doctor", "fixtures", "verify", "installed-roster", "run"):
            arguments = [str(field), "--thumbs", command]
            if command != "doctor":
                arguments += ["--root", str(self.root)]
            if command == "installed-roster":
                arguments += ["--b5-root", str(self.b5)]
            if command == "run":
                arguments += ["--installed", "--roster", str(roster)]
            result = subprocess.run(arguments, capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 1 if command == "run" else 0, result.stderr)
            if command == "installed-roster":
                roster.write_text(result.stdout)
        rows = json.loads(roster.read_text())
        self.assertEqual({row["id"] for row in rows}, set(bench.IDS) - {"flea"})
        run = next(self.root.glob("run-*"))
        self.assertEqual(len((run / "runs.jsonl").read_text().splitlines()), 189)
        commands = [json.loads(line) for line in (self.root / "stub-commands.jsonl").read_text().splitlines()]
        phases = [row["argv"][1] for row in commands if row["kind"] == "flea-field-bench"]
        self.assertEqual(phases, ["doctor", "fixtures", "verify", "installed-roster", "run"])

    def test_l_b7_missing_child_cpu_cannot_be_presented_as_complete(self):
        result = self.one_sample("strata-stable")
        commands = [json.loads(line) for line in (self.root / "stub-commands.jsonl").read_text().splitlines()]
        perf = next(row["argv"] for row in commands if row["kind"] == "perf" and "-o" in row["argv"])
        self.assertNotIn("--no-inherit", perf)
        self.assertNotIn("-i", perf)
        self.assertEqual(perf[perf.index("--") + 1:perf.index("--") + 3], ["dbus-run-session", "--"])
        self.assertEqual(result["user_cpu_s_diagnostic"], 1.234)
        self.assertIsNone(result["user_cpu_s"])

    def test_l_b7_unequal_edge_refuses_thumbnail_completion(self):
        result = self.one_sample("thunar")
        self.assertEqual(result["cache_written"], {"normal/128": 16})
        self.assertEqual(result["verified"], 0)
        self.assertIsNone(result["all_thumbnails_ms"])
        self.assertIsNone(result["all_thumbnails_settle_ms"])

    def test_l_b7_swapped_version_is_refused_before_sample_launch(self):
        rows, _ = self.roster()
        row = next(row for row in rows if row["id"] == "strata-stable")
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        meta["binaries"][str(self.paths["strata-stable"])]["version_output"] = "strata 2.0.0"
        path.write_text(json.dumps(meta))
        with bench.wayland.Display(self.root) as display:
            with self.assertRaisesRegex(ValueError, "inventoried version"):
                bench.measure.run_one(REPO, self.root, display, self.root, row, 1, 2)

    def test_l_b7_swapped_binary_is_refused_before_sample_launch(self):
        rows, _ = self.roster()
        row = next(row for row in rows if row["id"] == "strata-stable")
        self.paths["strata-stable"].write_text("#!/bin/sh\necho strata 1.0.0\n")
        with mock.patch.object(bench.measure.subprocess, "Popen") as launch:
            with self.assertRaisesRegex(ValueError, "checksum differs"):
                bench.measure.run_one(REPO, self.root, None, self.root, row, 1, 2)
        launch.assert_not_called()

    def test_l_b7_crashed_quit_cannot_keep_completion_metrics(self):
        path = self.root / ".b7-offline.json"
        meta = json.loads(path.read_text())
        meta["exit_status"] = {"strata-stable": 139}
        path.write_text(json.dumps(meta))
        result = self.one_sample("strata-stable")
        self.assertEqual(result["verified"], 16)
        self.assertTrue(any("status 139" in reason for reason in result["reasons"]), result["reasons"])
        self.assertIsNone(result["all_thumbnails_ms"])
        self.assertIsNone(result["per_thumbnail_ms"])
        self.assertIsNone(result["all_thumbnails_settle_ms"])

    def test_l_b7_fail_named_ancestor_does_not_hide_valid_outputs(self):
        fixture = self.root / "ancestor-fixture"
        fixture.mkdir()
        cache = self.root / "fail" / "cache"
        (cache / "thumbnails/large").mkdir(parents=True)
        stub = (REPO / "tests/bench-thumbs-stub.py").read_text()
        namespace = {}
        exec("import struct, zlib\n" + stub[stub.index("def png("):stub.index("# Entrants")], namespace)
        for index in range(16):
            source = fixture / f"s{index}.jpg"
            source.write_bytes(b"stub fixture")
            name = hashlib.md5(source.as_uri().encode()).hexdigest()
            (cache / "thumbnails/large" / f"{name}.png").write_bytes(namespace["png"](256, 192, source.as_uri()))
        self.assertEqual(len(bench.measure.verified_cache(cache, fixture)), 16)

    def test_full_early_schedule_manifest_and_candidate_stays_unmeasured(self):
        rows, path = self.roster()
        args = types.SimpleNamespace(roster=path, installed=True, only=None, timeout=15, passes=7)
        calls = []
        def sample(repo, parent, display, fixture, entrant, round_number, timeout):
            calls.append((fixture.name, entrant["id"], round_number))
            return dict(all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None, user_cpu_s=None, simulated=True)
        with mock.patch.object(bench, "verify_fixtures", return_value={"simulated": True}), mock.patch.object(bench.measure, "run_one", side_effect=sample), mock.patch("builtins.print"):
            self.assertEqual(bench.run(args, self.root), 1)
        self.assertEqual(len(calls), 189)
        self.assertNotIn("flea", {call[1] for call in calls})
        self.assertEqual([call[1] for call in calls[:9]], [row["id"] for row in rows])
        self.assertEqual([call[1] for call in calls[27:36]], list(reversed([row["id"] for row in rows])))
        run = next(self.root.glob("run-*"))
        manifest = json.loads((run / "manifest.json").read_text())
        self.assertEqual(manifest["entrants"], rows)
        self.assertEqual(manifest["session"]["display_lock"], str(self.runtime / "flea-display.lock"))
        summary = json.loads((run / "summary.json").read_text())
        self.assertTrue(all(not value["measured"] for row in summary if row["id"] == "flea" for value in row["metrics"].values()))

    def test_a_focused_single_pass_runs_one_round_and_cannot_rank(self):
        rows, path = self.roster()
        args = types.SimpleNamespace(roster=path, installed=True, only="thunar,nemo", timeout=15, passes=1)
        calls = []
        def sample(repo, parent, display, fixture, entrant, round_number, timeout):
            calls.append((fixture.name, entrant["id"], round_number))
            return dict(all_thumbnails_ms=None, per_thumbnail_ms=None, preview_800_ms=None, user_cpu_s=None, simulated=True)
        with mock.patch.object(bench, "verify_fixtures", return_value={"simulated": True}), mock.patch.object(bench.measure, "run_one", side_effect=sample), mock.patch("builtins.print"):
            self.assertEqual(bench.run(args, self.root), 1)
        self.assertEqual(len(calls), 6)
        self.assertEqual({call[2] for call in calls}, {0})
        self.assertEqual({call[1] for call in calls}, {"thunar", "nemo"})

    def test_cli_exposes_the_focused_run_controls(self):
        result = subprocess.run([str(REPO / "tools/flea-field-bench"), "--thumbs", "run", "--help"],
                                capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("--only", result.stdout)
        self.assertIn("--passes 1-7", result.stdout)

    def test_source_fallback_and_changed_archive_are_refused(self):
        with mock.patch.dict(os.environ, {"STRATA_STABLE_BIN": ""}):
            with self.assertRaisesRegex(ValueError, "explicit B5 archive"):
                bench.inventory.installed(self.root, self.b5, bench.IDS)
        archive = json.loads((self.b5 / "rival-versions.json").read_text())["archives"][0]
        (self.paths["strata-stable"].parent / "SOURCE_COMMIT").write_text("bad source")
        with self.assertRaisesRegex(ValueError, "embedded source commit"):
            bench.inventory.archive_identity(archive, self.paths["strata-stable"])

    def test_raw_verifier_is_mandatory_and_checks_both_real_set_locations(self):
        repo = self.root / "raw-owner"
        (repo / "tools").mkdir(parents=True)
        corpus = self.root / "camera-corpus"
        (corpus / "twelve").mkdir(parents=True)
        (corpus / "hundred").mkdir()
        with self.assertRaisesRegex(ValueError, "official tools/flea-raw-corpus"):
            bench.raw_corpus.verify(repo, self.root, corpus)
        tool = repo / "tools/flea-raw-corpus"
        tool.write_text("offline verifier stub, never camera data")
        (repo / "tools/flea-raw-corpus.tsv").write_text("offline verifier manifest stub")
        with mock.patch.object(bench.raw_corpus.subprocess, "run", return_value=types.SimpleNamespace(returncode=0)) as run:
            result = bench.raw_corpus.verify(repo, self.root, corpus)
            self.assertEqual([call.args[0] for call in run.call_args_list],
                             [[str(tool), "verify", str(corpus), "--set", "twelve"], [str(tool), "verify", str(corpus), "--set", "hundred"]])
            self.assertEqual(set(result["sets"]), {"twelve", "hundred"})
            self.assertEqual(result["sets"]["twelve"]["directory"], str(corpus / "twelve"))
            self.assertEqual(result["sets"]["hundred"]["directory"], str(corpus / "hundred"))
        with mock.patch.object(bench.raw_corpus.subprocess, "run", return_value=types.SimpleNamespace(returncode=3)) as run:
            with self.assertRaisesRegex(ValueError, "verification failed"):
                bench.raw_corpus.verify(repo, self.root, corpus)
            self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    result = unittest.main(exit=False)
    print(f"bench-thumbs-native: {result.result.testsRun} offline contracts, {len(result.result.failures) + len(result.result.errors)} failed")
    raise SystemExit(not result.result.wasSuccessful())
