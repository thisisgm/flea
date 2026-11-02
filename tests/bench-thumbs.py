"""Negative controls for worker timing, provenance, cold caches, and incomplete ranking."""

import datetime
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

REPO = Path(__file__).resolve().parent.parent
loader = importlib.machinery.SourceFileLoader("bench_thumbs", str(REPO / "tools/flea-bench-thumbs"))
spec = importlib.util.spec_from_loader(loader.name, loader)
bench = importlib.util.module_from_spec(spec)
loader.exec_module(bench)
measure = bench.measure
ROOT = Path(os.environ["FLEA_B7_TEST_ROOT"])


class Contracts(unittest.TestCase):
    def test_queue_and_concurrent_workers_are_not_divided_wall_time(self):
        events = []
        for index in range(16):
            finish = 0.113 + (0.470 - 0.113) * index / 15
            timestamp = datetime.datetime.fromtimestamp(1_700_000_000 + finish, datetime.timezone.utc).isoformat().replace("+00:00", "Z")
            events.extend(measure.worker_events(f"{timestamp} DEBUG browser worker completed operation=Image queue_ms=0 elapsed_ms=113"))
        result = measure.worker_metrics(events, 16)
        self.assertAlmostEqual(result["all_thumbnails_ms"], 470, places=2)
        self.assertEqual(result["per_thumbnail_ms"], 113)
        self.assertNotEqual(result["per_thumbnail_ms"], result["all_thumbnails_ms"] / 16)

    def test_queue_is_included_in_batch_but_not_worker_latency(self):
        event = measure.worker_events("2026-10-20T08:00:00.100Z browser worker completed operation=Image queue_ms=80 elapsed_ms=20")[0]
        self.assertEqual(event["end_ms"] - event["start_ms"], 100)
        self.assertEqual(event["elapsed_ms"], 20)

    def test_metadata_jobs_and_unstamped_lines_never_become_thumbnails(self):
        self.assertEqual(measure.worker_events("2026-10-20T08:00:00Z browser worker completed operation=ImageMetadata queue_ms=0 elapsed_ms=4"), [])
        self.assertEqual(measure.worker_events("browser worker completed operation=Image queue_ms=0 elapsed_ms=4"), [])

    def test_empty_output_and_duplicate_work_cannot_win(self):
        event = dict(kind="Image", end_ms=100, start_ms=0, elapsed_ms=100)
        for count, verified in ((16, 15), (15, 16), (17, 16), (0, 0)):
            result = measure.worker_metrics([event] * count, verified)
            self.assertIsNone(result["all_thumbnails_ms"])
            self.assertIsNone(result["per_thumbnail_ms"])

    def test_preview_log_alone_cannot_prove_success_or_800px(self):
        event = dict(kind="PreviewImage", end_ms=100, start_ms=0, elapsed_ms=134)
        result = measure.worker_metrics([event] * 16, 16)
        self.assertIsNone(result["preview_800_ms"])
        self.assertEqual(result["preview_worker_ms_unverified"], 134)

    def test_user_cpu_never_counts_system_time_or_unsupported_perf(self):
        self.assertEqual(measure.perf_user("2810.000,msec,task-clock:u,1,100.00,,"), 2.81)
        self.assertIsNone(measure.perf_user("<not supported>,msec,task-clock:u,0,0,,"))
        self.assertIsNone(measure.perf_user("4000,msec,task-clock,1,100,,"))
        self.assertIsNone(measure.perf_user("4,seconds,task-clock:u,1,100,,"))

    def rows(self):
        return [dict(id=ident, fixture=name, round=round_number, verified=16, **{metric: 1.0 for metric in bench.AFTER[name]})
                for name in bench.SPECS for ident in bench.IDS for round_number in range(7)]

    def test_warmup_excluded_and_equal_medians_never_first(self):
        rows = self.rows()
        for row in rows:
            if row["round"] == 0:
                row["all_thumbnails_ms"] = 10000
        result = bench.summary(rows)[0]["metrics"]["all_thumbnails_ms"]
        self.assertEqual(result["median"], 1)
        self.assertEqual(result["minimum"], 1)
        self.assertFalse(result["first"])

    def test_missing_round_or_rival_and_zero_cannot_rank(self):
        rows = self.rows()
        rows = [r for r in rows if not (r["id"] == "strata-nightly" and r["round"] == 6)]
        for row in rows:
            if row["id"] == "flea":
                row["all_thumbnails_ms"] = 0.5
                row["user_cpu_s"] = 0
        flea = bench.summary(rows)[0]["metrics"]
        self.assertFalse(flea["all_thumbnails_ms"]["first"])
        self.assertTrue(flea["all_thumbnails_ms"]["beats_after"])
        self.assertFalse(flea["user_cpu_s"]["measured"])

    def test_duplicate_rounds_booleans_and_infinity_cannot_rank(self):
        for value in (True, float("inf")):
            rows = self.rows()
            for row in rows:
                row["all_thumbnails_ms"] = value
            self.assertFalse(bench.summary(rows)[0]["metrics"]["all_thumbnails_ms"]["measured"])
        rows = self.rows()
        for row in rows:
            if row["round"] == 6:
                row["round"] = 5
        self.assertFalse(bench.summary(rows)[0]["metrics"]["all_thumbnails_ms"]["measured"])

    def test_bench_thumbs_reader_delay_mutants_cannot_rank(self):
        rows = self.rows()
        self.assertEqual(bench.summary(rows)[0]["id"], "flea")
        self.assertTrue(all(metric["measured"] for metric in bench.summary(rows)[0]["metrics"].values()))
        for row in rows:
            if row["id"] == "flea":
                row["mutant"] = True
        self.assertTrue(all(not metric["measured"] for metric in bench.summary(rows)[0]["metrics"].values()))

    def test_marker_and_symlink_are_required_to_guard_writes(self):
        path = Path(tempfile.mkdtemp(dir=ROOT))
        with self.assertRaises(ValueError):
            bench.marked_root(str(path))
        (path / ".flea-test-sandbox").touch()
        self.assertEqual(bench.marked_root(str(path)), path)
        alias = ROOT / "alias"
        alias.symlink_to(path, target_is_directory=True)
        with self.assertRaises(ValueError):
            bench.marked_root(str(alias))
        with self.assertRaises(ValueError):
            bench.marked_root(".")

    def test_fixture_pixels_are_reproducible_and_png_crc_is_real(self):
        import struct
        import zlib
        first = bench.pixels(32, 16, 7001, True)
        self.assertEqual(first, bench.pixels(32, 16, 7001, True))
        self.assertNotEqual(first, bench.pixels(32, 16, 7002, True))
        chunk = bench.png_chunk(b"IDAT", zlib.compress(first))
        self.assertEqual(struct.unpack(">I", chunk[:4])[0], len(chunk) - 12)
        self.assertEqual(struct.unpack(">I", chunk[-4:])[0], zlib.crc32(chunk[4:-4]))
        self.assertEqual(zlib.decompress(chunk[8:-4]), first)

    def test_cache_guard_rejects_failure_markers_wrong_size_and_bad_png(self):
        fixture = ROOT / "cache-fixture"
        fixture.mkdir()
        (fixture / "one.jpg").touch()
        cache = ROOT / "cache"
        (cache / "thumbnails/fail").mkdir(parents=True)
        (cache / "thumbnails/large").mkdir()
        import hashlib
        import struct
        import zlib
        key = hashlib.md5((fixture / "one.jpg").as_uri().encode()).hexdigest()
        def png(edge):
            header = bench.png_chunk(b"IHDR", struct.pack(">IIBBBBB", edge, 1, 8, 2, 0, 0, 0))
            uri = bench.png_chunk(b"tEXt", b"Thumb::URI\0" + (fixture / "one.jpg").as_uri().encode())
            data = bench.png_chunk(b"IDAT", zlib.compress(b"\0" + b"\x11\x22\x33" * edge))
            return b"\x89PNG\r\n\x1a\n" + header + uri + data + bench.png_chunk(b"IEND", b"")
        large = cache / f"thumbnails/large/{key}.png"
        large.write_bytes(png(256))
        self.assertEqual(measure.verified_cache(cache, fixture), {key})
        large.unlink()
        (cache / f"thumbnails/fail/{key}.png").write_bytes(png(256))
        self.assertEqual(measure.verified_cache(cache, fixture), set())
        large.write_bytes(png(128))
        self.assertEqual(measure.verified_cache(cache, fixture), set())
        large.write_bytes(b"not a png")
        self.assertEqual(measure.verified_cache(cache, fixture), set())

    def test_complete_roster_and_baseline_are_mandatory(self):
        rows = bench.roster_template()
        self.assertEqual({r["id"] for r in rows}, set(bench.IDS))
        path = ROOT / "roster.json"
        path.write_text(json.dumps(rows[:-1]))
        with self.assertRaises(ValueError):
            bench.roster(path)

    def test_installed_roster_cannot_select_the_candidate(self):
        path = ROOT / "installed-roster.json"
        path.write_text(json.dumps([dict(id="flea", binary="/never-run-this")]))
        with self.assertRaises(ValueError):
            bench.roster(path, installed=True)

    def test_candidate_requires_its_inventoried_gpui_sibling_before_launch(self):
        from unittest import mock
        from inventory import digest
        path = Path(tempfile.mkdtemp(dir=ROOT))
        binary = path / "flea"
        binary.write_text("stub binary")
        (path / "flea-gui").write_text("stub gui")
        entrant = dict(id="flea", binary=str(binary), sha256=digest(binary), gui_sha256=digest(binary), version="0.4.0")
        with mock.patch.object(measure.subprocess, "Popen") as launch, mock.patch.object(measure.version_probe, "probe") as probe:
            with self.assertRaisesRegex(ValueError, "GPUI sibling checksum"):
                measure.run_one(REPO, ROOT, None, ROOT, entrant, 1, 15)
        probe.assert_not_called()
        launch.assert_not_called()

    def test_existing_field_entry_dispatches_before_shared_cache_setup(self):
        result = subprocess.run([str(REPO / "tools/flea-field-bench"), "--thumbs", "roster"], env={"PATH": os.environ["PATH"]}, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(json.loads(result.stdout)), len(bench.IDS))

    def test_real_fixture_sizes_decodes_and_tamper_refusal(self):
        root = Path(tempfile.mkdtemp(dir=ROOT))
        (root / ".flea-test-sandbox").touch()
        made = bench.fixtures(root)
        checked = bench.verify_fixtures(root)
        self.assertEqual(checked, made)
        self.assertEqual(sum(len(v["files"]) for v in checked["fixtures"].values()), 48)
        file = root / "fixtures/photo-jpeg/sample-00.jpg"
        with file.open("ab") as stream:
            stream.write(b"changed")
        with self.assertRaises(ValueError):
            bench.verify_fixtures(root)

    def test_l_b7_false_completion_and_missing_output_cannot_rank(self):
        for verified in (None, 0, 15, 17):
            rows = self.rows()
            for row in rows:
                row["verified"] = verified
            metrics = bench.summary(rows)[0]["metrics"]
            self.assertEqual(set(metrics), set(bench.AFTER["photo-jpeg"]))
            self.assertTrue(all(not value["measured"] and not value["first"] for value in metrics.values()))
        rows = self.rows()
        for row in rows:
            row["simulated"] = True
        self.assertFalse(bench.summary(rows)[0]["metrics"]["all_thumbnails_ms"]["measured"])

    def test_l_b7_template_exposes_package_identity_fields(self):
        rows = {row["id"]: row for row in bench.roster_template()}
        for ident in bench.inventory.version_probe.PACKAGED:
            self.assertEqual(rows[ident]["package_name"], ident)
            self.assertIn("package_version", rows[ident])

    def test_l_b7_background_suite_output_obeys_run_all(self):
        import re
        result = subprocess.run([sys.executable, "-B", str(REPO / "tests/bench-thumbs-background.py"), "-q"],
                                capture_output=True, text=True, timeout=15)
        self.assertEqual(result.returncode, 0, result.stderr)
        # Sample input: 'FAIL x' and 'suite: FAIL x'; the run-all failure sentinel.
        failure_line = re.compile(r"(^|[: ])FAIL[: ]", re.MULTILINE)
        self.assertNotRegex(result.stdout + result.stderr, failure_line)

    def test_l_b7_tmp_fixture_remains_visible_in_sandbox(self):
        import shutil
        parent = Path(tempfile.mkdtemp(prefix="mount-", dir=ROOT))
        root = parent / "sample"
        root.mkdir()
        measure.directories(root)
        fixture = parent / "fixture.jpg"
        fixture.write_bytes(b"stub fixture")
        command = measure.sandbox(root)
        binds = [command[index + 1:index + 3] for index, value in enumerate(command) if value == "--bind"]
        self.assertNotIn([str(root / "tmp"), "/tmp"], binds)
        if not shutil.which("bwrap"):
            self.skipTest("bubblewrap unavailable after mount-contract assertion")
        result = subprocess.run(command + ["/usr/bin/test", "-f", str(fixture)], capture_output=True, text=True, timeout=10)
        if "Operation not permitted" in result.stderr:
            self.skipTest("host disables unprivileged namespaces after mount-contract assertion")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_l_b7_fixture_writes_cannot_escape_through_an_existing_link(self):
        root = Path(tempfile.mkdtemp(prefix="guard-", dir=ROOT))
        outside = Path(tempfile.mkdtemp(prefix="guard-target-", dir=ROOT))
        (root / ".flea-test-sandbox").touch()
        sentinel = outside / "sentinel"
        sentinel.write_text("unchanged")
        (root / "fixtures").symlink_to(outside, target_is_directory=True)
        from unittest import mock
        with mock.patch.object(bench, "SPECS", {"tiny": (2, 2, 100, "png")}):
            with self.assertRaises((OSError, ValueError)):
                bench.fixtures(root)
        self.assertEqual([path.name for path in outside.iterdir()], ["sentinel"])
        self.assertEqual(sentinel.read_text(), "unchanged")

    def test_fresh_run_directories_start_with_an_empty_cache(self):
        first = Path(tempfile.mkdtemp(dir=ROOT))
        second = Path(tempfile.mkdtemp(dir=ROOT))
        measure.directories(first)
        measure.directories(second)
        (first / "cache/already-rendered").touch()
        self.assertEqual(list((second / "cache").iterdir()), [])


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--cleanup-root":
        import shutil
        target = Path(sys.argv[2])
        marker = target / ".flea-test-sandbox"
        if (target != ROOT or not target.is_absolute() or target.resolve() != target or target.is_symlink()
                or target.parent != Path("/tmp") or not target.name.startswith("flea-b7-contracts.")
                or marker.is_symlink() or not marker.is_file()):
            raise SystemExit("refusing cleanup outside the marked private contracts root")
        shutil.rmtree(target)
        print(f"bench-thumbs: removed sandbox {target}")
        raise SystemExit(0)
    result = unittest.main(exit=False)
    print(f"bench-thumbs: {result.result.testsRun} contracts, {len(result.result.failures) + len(result.result.errors)} failed")
    raise SystemExit(not result.result.wasSuccessful())
