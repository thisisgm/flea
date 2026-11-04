#!/usr/bin/env python3
"""Drive tools/flea-bench-gate report through valid and deliberately broken B2 evidence."""
import copy
import csv
import hashlib
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import unittest

repo, scratch, gate = map(Path, sys.argv[1:4])
if not (scratch / ".flea-test-sandbox").is_file():
    raise SystemExit("bench-gate-provenance: scratch root has no sandbox marker")

HEADER = ("id,run,mapped_ms,settled_ms,pss_kb,cpu_s,child_pss_kb,thumbs_n,pss_anon_kb,uss_kb,drm_kb,thumbs_by_dir,"
          "cpu_tree_s,thumbs_by_format,ranked,preview_ms,cpu_reaped_s,cpu_sys_s,dirty_wb_kb,pgpgout_pre_kb,"
          "pgpgout_launch_kb,write_clean,rerun,first_rows_ms,cpu_main_s,cold_pss_kb,cold_drm_kb,cold_child_pss_kb,"
          "cold_sample_after_map_ms,launch_pos").split(",")
ROSTER = ("flea", "flea-anchor", "dolphin", "nautilus", "thunar", "pcmanfm", "nemo", "strata-stable", "strata-nightly")
PACKAGED = ROSTER[2:7]
CANDIDATE_COMMIT = "1" * 40
ANCHOR_COMMIT = "f21789e56cc4b0877caa3156fff7bce52a8dd9c2"
CPU_METHOD = ("complete run cgroup plus an external helper, identical to cpu_tree_s; "
              "cpu_main_s retains the main-process reading")
WORKLOADS = {
    "field_a": ("/home/flea-sandbox/flea-bench-btrfs", 100000, "field", "yes", 0),
    "field_b": ("/home/flea-sandbox/flea-media-btrfs", 2000, "field", "yes", 1800),
    "engine": ("/home/flea-sandbox/flea-bench-btrfs", 100000, "engine", "no", 0),
}
CELL_KEYS = [f"B2:{label}:{metric}" for label, metrics in (
    ("field-a", ("mapped_ms", "settled_ms", "memory_mib", "cpu_s")),
    ("field-b", ("mapped_ms", "settled_ms", "memory_mib", "cpu_s")),
    ("engine", ("cold_mapped_ms", "cold_memory_mib", "cold_gpu_mib"))) for metric in metrics]


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


def binary(name, version=None):
    return {"path": f"/opt/bench/{name}", "sha256": digest(name), "version": version}


def context(workload):
    fixture, _, _, drop, _ = WORKLOADS[workload]
    rivals = {name: {"binary": binary(name), "package": f"{name} 1.0-1"} for name in PACKAGED}
    for name, version in (("strata-stable", "strata 1.1.0"), ("strata-nightly", "strata 1.0.1-nightly.20261018")):
        record = binary(name, version)
        rivals[name] = {"binary": record, "archive": {
            "id": name, "expected_version": version, "binary_path": record["path"], "binary_sha256": record["sha256"],
            "version_check": {"rc": 0, "stdout": version}, "tag": f"v-{name}", "url": f"https://example.invalid/{name}",
            "sha256": digest(name + "archive"), "source_commit": "2" * 40, "tag_commit": "3" * 40}}
    anchor = binary("anchor-flea", "0.3.12")
    return {
        "host": "minipc", "kernel": "6.17.1-arch1-1",
        "session": {"boot_id": "boot-1", "hyprland_instance": "hypr-1", "wayland_display": "wayland-1",
                    "monitors": [{"name": "DP-1", "width": 2560, "height": 1440, "scale": 1}],
                    "gpu": ["00:02.0 VGA compatible controller [0300]: Intel Corporation [UHD Graphics 630]"]},
        "storage": {"source": "/dev/nvme0n1p2", "fstype": "btrfs", "target": fixture, "fsroot": f"/@{Path(fixture).name}",
                    "uuid": "uuid-1", "maj:min": "0:40"},
        "builds": {"flea": {"commit": CANDIDATE_COMMIT, "launcher": binary("flea", "0.4.0"), "gui": binary("flea-gui")},
                   "flea-anchor": {"commit": ANCHOR_COMMIT, "launcher": anchor,
                                   "package": f"{anchor['path']} is owned by flea 0.3.12-1",
                                   "ui_sha256": digest("anchor-ui")}},
        "rivals": rivals, "drop_thumbs": drop,
    }


def rows(workload):
    entrants = ROSTER[:2] if workload == "engine" else ROSTER
    result = []
    for run in (1, 2, 3):
        for place, entrant in enumerate(entrants):
            factor = 1 if entrant == "flea" else 2 + place / 10
            cpu = f"{0.5 * factor:.2f}"
            row = dict.fromkeys(HEADER, "0")
            row.update(id=entrant, run=str(run), mapped_ms=str(int(100 * factor)), settled_ms=str(int(300 * factor)),
                       pss_kb=str(int(50000 * factor)), child_pss_kb=str(int(20000 * factor)), cpu_s=cpu,
                       cpu_tree_s=cpu, cpu_main_s=f"{0.4 * factor:.2f}", ranked="yes", write_clean="1",
                       thumbs_n="unmeasurable", thumbs_by_format="unmeasurable", thumbs_by_dir="-",
                       cold_pss_kb=str(int(40000 * factor)), cold_child_pss_kb=str(int(10000 * factor)),
                       cold_drm_kb=str(int(30000 * factor)), cold_sample_after_map_ms="3050",
                       launch_pos=str(place + 1))
            if workload == "field_b":
                row.update(thumbs_n="40", thumbs_by_format="jpg=30;png=10;unknown=0")
            result.append(row)
    return result


def manifest(workload, csv_text, record, closed=True, assertion=None, strata=None, identities=None, dirty="clean",
             cpu_method=CPU_METHOD):
    fixture, count, profile, _, denominator = WORKLOADS[workload]
    builds = record["builds"]
    lines = ["# Field bench manifest", "", "## Run", "", "- date: 2026-10-25T10:00:00+00:00",
             f"- host: {record['host']}, kernel {record['kernel']}", f"- fixture: {fixture}", "- fixture kind: btrfs",
             f"- fixture payload: {count} visible entries, filesystem btrfs", f"- front end: gpui; profile: {profile}",
             f"- candidate source commit: {builds['flea']['commit']}",
             f"- gate context: {json.dumps(record, sort_keys=True)}", f"- cpu_s: {cpu_method}"]
    if profile == "engine":
        lines += ["- engine method: cold cache, 3 s pause, Hyprland openwindow event, PSS and resident DRM at map "
                  "plus 3 s; three interleaved runs per build",
                  "- engine PSS: cold_pss_kb is the GUI alone; cold_child_pss_kb is the backend; "
                  "the gate compares their sum"]
    for name in strata if strata is not None else ("strata-stable", "strata-nightly"):
        item = record["rivals"][name]["binary"]
        lines.append(f"- Strata {name.removeprefix('strata-')}: {item['path']}; exact version: {item['version']}; "
                     f"sha256: {item['sha256']}")
    for sha in identities or (builds["flea"]["launcher"]["sha256"], builds["flea"]["gui"]["sha256"],
                              builds["flea-anchor"]["launcher"]["sha256"]):
        lines.append(f"- Flea binary identity: /opt/bench/some-flea sha256 {sha}")
    lines += [f"- fixture payload assertion: {assertion or f'PASSED, {count} visible entries against the expected {count}'}",
              f"- thumbnailable denominator: {denominator}, from {count} visible entries less the 0 above",
              "", "## Entrants", "", "```",
              f"flea       gui  source {CANDIDATE_COMMIT[:9]} untagged ({dirty}), release binary built 2026-10-25, 1 bytes",
              f"flea-anchor gui source {ANCHOR_COMMIT[:9]} v0.3.12 (clean), release binary built 2026-10-06, 1 bytes"]
    lines += [f"{name}    gui  {record['rivals'][name]['package']}" for name in PACKAGED] + ["```", ""]
    if closed:
        lines += ["## Run close", "", "- ended: 2026-10-25T11:00:00+00:00", "## Rank refusals", "",
                  f"- closed CSV sha256: {digest(csv_text)}"]
        # A close killed between its hash line and its last line is partial, and never approves numbers.
        if closed != "partial":
            lines.append("- close complete: yes")
    return "\n".join(lines) + "\n"


class Evidence:
    """One comparison's three CSVs and manifests, with every input editable before it is written."""

    counter = 0

    def __init__(self):
        Evidence.counter += 1
        self.root = scratch / f"case-{Evidence.counter:02d}"
        self.root.mkdir()
        self.rows = {workload: rows(workload) for workload in WORKLOADS}
        self.contexts = {workload: context(workload) for workload in WORKLOADS}
        self.options = {workload: {} for workload in WORKLOADS}

    def path(self, workload):
        return self.root / f"{workload}.csv"

    def write(self):
        for workload in WORKLOADS:
            with self.path(workload).open("w", newline="") as stream:
                writer = csv.DictWriter(stream, fieldnames=HEADER, lineterminator="\n")
                writer.writeheader()
                writer.writerows(self.rows[workload])
            text = self.path(workload).read_text()
            self.path(workload).with_suffix(".manifest.md").write_text(
                manifest(workload, text, self.contexts[workload], **self.options[workload]))
        return self

    def run(self, a=None, b=None, engine=None, output=None):
        output = output or self.root / "receipt.json"
        result = subprocess.run(
            [sys.executable, "-B", str(gate), "report", "--a", str(a or self.path("field_a")),
             "--b", str(b or self.path("field_b")), "--engine", str(engine or self.path("engine")),
             "--output", str(output)], capture_output=True, text=True, timeout=60, stdin=subprocess.DEVNULL)
        receipt = json.loads(output.read_text()) if output.is_file() else None
        return result, receipt


def cells(receipt):
    return {cell["key"]: cell for cell in receipt["cells"]}


class GateProvenance(unittest.TestCase):
    def refused(self, evidence, pattern, **paths):
        result, receipt = evidence.run(**paths)
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertRegex(result.stderr, pattern)
        self.assertFalse(receipt["release_pass"])
        self.assertEqual([cell["key"] for cell in receipt["cells"]], CELL_KEYS)
        self.assertEqual({cell["state"] for cell in receipt["cells"]}, {"BLOCKED"})
        return receipt

    def test_separate_valid_workloads_pass_with_identity_bound_cells(self):
        evidence = Evidence().write()
        result, receipt = evidence.run()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(receipt["release_pass"] and receipt["first_everywhere"])
        self.assertEqual([cell["key"] for cell in receipt["cells"]], CELL_KEYS)
        for cell in receipt["cells"]:
            self.assertEqual(cell["state"], "FIRST")
            path = evidence.path(cell["workload"])
            self.assertEqual(cell["evidence"]["csv_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
            self.assertEqual(cell["evidence"]["manifest_sha256"],
                             hashlib.sha256(path.with_suffix(".manifest.md").read_bytes()).hexdigest())
        self.assertTrue(all(row["eligible"] for audited in receipt["rows"].values() for row in audited))
        self.assertEqual(len(receipt["rows"]["field_a"]), 27)
        self.assertEqual(len(receipt["rows"]["engine"]), 6)

    def test_one_csv_in_three_roles_without_manifests(self):
        evidence = Evidence()
        lone = evidence.root / "one.csv"
        with lone.open("w", newline="") as stream:
            writer = csv.DictWriter(stream, fieldnames=HEADER)
            writer.writeheader()
            writer.writerows(rows("field_a"))
        self.refused(evidence, "manifest|distinct", a=lone, b=lone, engine=lone)

    def test_one_csv_in_three_roles_with_its_manifest(self):
        evidence = Evidence().write()
        lone = evidence.path("field_a")
        self.refused(evidence, "distinct CSVs and manifests", a=lone, b=lone, engine=lone)

    def test_copied_workload_in_another_role(self):
        evidence = Evidence().write()
        for suffix in (".csv", ".manifest.md"):
            shutil.copyfile(evidence.path("field_a").with_suffix(suffix), evidence.root / f"copy{suffix}")
        self.refused(evidence, "engine: manifest fixture", engine=evidence.root / "copy.csv")

    def test_swapped_manifests(self):
        evidence = Evidence().write()
        a, b = (evidence.path(name).with_suffix(".manifest.md") for name in ("field_a", "field_b"))
        text_a, text_b = a.read_text(), b.read_text()
        a.write_text(text_b)
        b.write_text(text_a)
        self.refused(evidence, "CSV hash mismatch")

    def test_swapped_workload_roles(self):
        evidence = Evidence().write()
        self.refused(evidence, "field_a: manifest fixture", a=evidence.path("field_b"), b=evidence.path("field_a"))

    def test_open_and_failed_manifests(self):
        evidence = Evidence()
        evidence.options["field_b"]["closed"] = False
        self.refused(evidence.write(), "missing complete close")
        evidence = Evidence()
        evidence.options["field_b"]["closed"] = "partial"
        self.refused(evidence.write(), "field_b.manifest.md: missing complete close")
        evidence = Evidence()
        evidence.options["field_a"]["assertion"] = "**FAILED**, 99990 visible entries against the expected 100000"
        self.refused(evidence.write(), "fixture, storage, profile")

    def test_wrong_storage(self):
        evidence = Evidence()
        evidence.contexts["field_b"]["storage"]["uuid"] = "uuid-2"
        self.refused(evidence.write(), "fixture storage differs")
        evidence = Evidence()
        for workload in WORKLOADS:
            evidence.contexts[workload]["storage"]["fstype"] = "ext4"
        self.refused(evidence.write(), "btrfs storage identity")

    def test_wrong_cache_method_host_and_session(self):
        evidence = Evidence()
        evidence.contexts["field_a"]["drop_thumbs"] = "no"
        self.refused(evidence.write(), "cold-cache method")
        evidence = Evidence()
        evidence.contexts["engine"]["session"]["boot_id"] = "boot-2"
        self.refused(evidence.write(), "session differs")
        evidence = Evidence()
        for workload in WORKLOADS:
            evidence.contexts[workload]["session"]["monitors"][0]["scale"] = 1.25
        self.refused(evidence.write(), "display session")

    def test_missing_child_cpu_and_mixed_accounting(self):
        evidence = Evidence()
        evidence.rows["field_a"][0].update(cpu_s="0.10", cpu_tree_s="0.10", cpu_main_s="0.40")
        evidence.rows["field_b"][0].update(cpu_s="0.40")
        evidence.rows["engine"][0].update(cpu_tree_s="-")
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertFalse(receipt["release_pass"])
        states = cells(receipt)
        for key in CELL_KEYS:
            self.assertEqual(states[key]["state"], "UNKNOWN", key)
            self.assertEqual(states[key]["incomplete_entrants"], ["flea"])
        reasons = {workload: audited[0]["reasons"] for workload, audited in receipt["rows"].items()}
        self.assertEqual(reasons["field_a"], ["whole-cgroup CPU is below the main process, so child CPU is missing"])
        self.assertEqual(reasons["field_b"], ["mixed CPU accounting: cpu_s is not the whole-cgroup cpu_tree_s"])
        self.assertEqual(reasons["engine"], ["missing whole-cgroup or main-process CPU"])
        self.assertIn("INELIGIBLE field_a line 2 flea run 1", result.stdout)
        evidence = Evidence()
        evidence.options["field_b"]["cpu_method"] = "main process only"
        self.refused(evidence.write(), "whole-cgroup method")

    def test_stale_binary(self):
        evidence = Evidence()
        evidence.contexts["engine"]["builds"]["flea"]["launcher"] = binary("older-flea", "0.4.0")
        self.refused(evidence.write(), "engine: builds differs")
        evidence = Evidence()
        evidence.options["field_a"]["identities"] = (digest("older-flea"), digest("flea-gui"), digest("anchor-flea"))
        self.refused(evidence.write(), "binary is stale")
        evidence = Evidence()
        for workload in WORKLOADS:
            evidence.contexts[workload]["builds"]["flea"]["launcher"]["version"] = "0.3.12"
        self.refused(evidence.write(), "binary identity/version")
        evidence = Evidence()
        evidence.contexts["engine"]["rivals"]["dolphin"]["package"] = "dolphin 0.9-1"
        self.refused(evidence.write(), "dolphin: installed package differs")
        evidence = Evidence()
        evidence.contexts["field_b"]["rivals"]["strata-nightly"]["binary"]["sha256"] = digest("old nightly")
        self.refused(evidence.write(), "strata-nightly")
        # A self-consistent but different nightly build on the B fixture is caught by the cross-fixture check alone.
        evidence = Evidence()
        nightly = evidence.contexts["field_b"]["rivals"]["strata-nightly"]
        nightly["binary"]["sha256"] = nightly["archive"]["binary_sha256"] = digest("newer nightly")
        self.refused(evidence.write(), "strata-nightly: rival version/build differs between fixtures")

    def test_candidate_and_anchor_identity(self):
        evidence = Evidence()
        evidence.options["field_a"]["dirty"] = "DIRTY"
        self.refused(evidence.write(), "flea: manifest source tree is not one clean checkout")
        evidence = Evidence()
        for workload in WORKLOADS:
            evidence.contexts[workload]["builds"]["flea-anchor"]["package"] = None
        self.refused(evidence.write(), "installed flea 0.3.12 package")
        evidence = Evidence()
        for workload in WORKLOADS:
            evidence.contexts[workload]["builds"]["flea-anchor"]["commit"] = "4" * 40
        self.refused(evidence.write(), "installed 0.3.12 build")

    def test_duplicate_row_ids_and_unknown_entrants(self):
        evidence = Evidence()
        evidence.rows["field_b"].append(copy.deepcopy(evidence.rows["field_b"][3]))
        self.refused(evidence.write(), "line 29 repeats or lacks the row ID nautilus run 1 rerun 0")
        evidence = Evidence()
        evidence.rows["field_a"][7]["id"] = "strata"
        self.refused(evidence.write(), "names the retired id 'strata'")

    def test_rows_outside_a_roster_are_listed_and_never_ranked(self):
        # A default producer run carries TUI rows, and its engine profile every GUI rival as well.
        evidence = Evidence()
        for run in ("1", "2", "3"):
            for extra in ("flea-tui", "yazi", "strata-nightly-channel"):
                evidence.rows["field_a"].append(dict(evidence.rows["field_a"][0], id=extra, run=run, mapped_ms="1"))
            evidence.rows["engine"].append(dict(evidence.rows["field_a"][2], run=run, mapped_ms="1"))
        channel = evidence.contexts["field_a"]["rivals"]
        channel["strata-nightly-channel"] = {"binary": binary("strata-nightly-channel", "strata 1.0.2-nightly"),
                                             "archive": None}
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(receipt["release_pass"])
        outside = {(row["id"], tuple(row["reasons"])) for audited in receipt["rows"].values()
                   for row in audited if not row["eligible"]}
        self.assertEqual(outside, {("flea-tui", ("outside the field_a roster, so it is listed and never ranked",)),
                                   ("yazi", ("outside the field_a roster, so it is listed and never ranked",)),
                                   ("strata-nightly-channel",
                                    ("outside the field_a roster, so it is listed and never ranked",)),
                                   ("dolphin", ("outside the engine roster, so it is listed and never ranked",))})

    def test_write_guard_rerun_rows(self):
        # The producer writes a contaminated launch and its clean rerun under one run number.
        evidence = Evidence()
        dirty = next(row for row in evidence.rows["field_b"] if (row["id"], row["run"]) == ("thunar", "2"))
        dirty.update(write_clean="0", ranked="unranked: writes in flight", rerun="0")
        evidence.rows["field_b"].append(dict(dirty, write_clean="1", ranked="yes", rerun="1"))
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual({cell["state"] for cell in receipt["cells"]}, {"FIRST"})
        thunar = [(row["eligible"], row["rerun"]) for row in receipt["rows"]["field_b"]
                  if (row["id"], row["run"]) == ("thunar", "2")]
        self.assertEqual(thunar, [(False, "0"), (True, "1")])
        evidence = Evidence()
        clean = next(row for row in evidence.rows["field_b"] if (row["id"], row["run"]) == ("thunar", "2"))
        evidence.rows["field_b"].append(dict(clean, rerun="1"))
        self.refused(evidence.write(), "two clean rows for thunar run 2")

    def test_installed_anchor_package_names(self):
        evidence = Evidence()
        for workload in WORKLOADS:
            anchor = evidence.contexts[workload]["builds"]["flea-anchor"]
            anchor["package"] = f"{anchor['launcher']['path']} is owned by flea-bin 0.3.12-1"
        result, _ = evidence.write().run()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        evidence = Evidence()
        for workload in WORKLOADS:
            anchor = evidence.contexts[workload]["builds"]["flea-anchor"]
            anchor["package"] = f"{anchor['launcher']['path']} is owned by flea-bin 0.3.11-1"
        self.refused(evidence.write(), "installed flea 0.3.12 package")

    def test_engine_sample_outside_its_window_is_listed(self):
        evidence = Evidence()
        evidence.rows["engine"][0]["cold_sample_after_map_ms"] = "3140"
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(receipt["rows"]["engine"][0]["reasons"],
                         ["cold sample 3140 ms after map, outside 3000 to 3100"])
        self.assertIn("INELIGIBLE engine line 2 flea run 1", result.stdout)

    def test_unexpected_failures_refuse_and_never_hold(self):
        evidence = Evidence()
        # A cell past the csv module's field limit raises csv.Error, which no named handler expects.
        evidence.rows["field_a"][3]["thumbs_by_dir"] = "d" * 200000
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)
        self.assertEqual({cell["state"] for cell in receipt["cells"]}, {"BLOCKED"})
        evidence = Evidence()
        evidence.options["field_b"]["closed"] = False
        result, _ = evidence.write().run(output=evidence.root / "no-such-dir" / "receipt.json")
        self.assertEqual(result.returncode, 2, result.stdout + result.stderr)

    def test_missing_stable_or_nightly(self):
        for name in ("strata-stable", "strata-nightly"):
            evidence = Evidence()
            evidence.options["field_a"]["strata"] = [item for item in ("strata-stable", "strata-nightly") if item != name]
            self.refused(evidence.write(), f"Strata {name.removeprefix('strata-')} exactly once")
            evidence = Evidence()
            evidence.options["field_b"]["strata"] = [item for item in ("strata-stable", "strata-nightly") if item != name]
            evidence.contexts["field_b"]["rivals"][name] = {"binary": None, "archive": None}
            self.refused(evidence.write(), "binary identity")
        evidence = Evidence()
        evidence.rows["field_b"] = [row for row in evidence.rows["field_b"] if row["id"] != "strata-nightly"]
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        for key in CELL_KEYS[4:8]:
            self.assertEqual(cells(receipt)[key]["state"], "MISSING")
            self.assertEqual(cells(receipt)[key]["missing_entrants"], ["strata-nightly"])
        self.assertEqual(cells(receipt)[CELL_KEYS[0]]["state"], "FIRST")

    def test_incomplete_eleven_cell_coverage(self):
        evidence = Evidence()
        evidence.rows["engine"] = [row for row in evidence.rows["engine"]
                                   if (row["id"], row["run"]) != ("flea-anchor", "3")]
        evidence.rows["field_a"][4].update(write_clean="0", ranked="unranked: writes in flight")
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        states = {key: cell["state"] for key, cell in cells(receipt).items()}
        self.assertEqual(len(states), 11)
        self.assertEqual({states[key] for key in CELL_KEYS[8:]}, {"UNKNOWN"})
        self.assertEqual({states[key] for key in CELL_KEYS[:4]}, {"UNKNOWN"})
        self.assertEqual({states[key] for key in CELL_KEYS[4:8]}, {"FIRST"})
        dirty = receipt["rows"]["field_a"][4]
        self.assertEqual((dirty["id"], dirty["eligible"]), ("thunar", False))
        self.assertEqual(dirty["reasons"], ["dirty writes during the run (write_clean=0)",
                                            "producer refused to rank it (unranked: writes in flight)"])

    def test_zero_media_work_never_ranks(self):
        evidence = Evidence()
        evidence.rows["field_b"][8].update(thumbs_n="0", thumbs_by_format="jpg=0;unknown=0")
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        self.assertEqual(receipt["rows"]["field_b"][8]["reasons"], ["no verified media work"])
        self.assertEqual(cells(receipt)["B2:field-b:mapped_ms"]["incomplete_entrants"], ["strata-nightly"])

    def test_loss_and_ceiling_tie_are_distinct(self):
        evidence = Evidence()
        for row in evidence.rows["field_a"]:
            if row["id"] == "nemo":
                row["settled_ms"] = "200"
            row.update(cpu_s="0.00", cpu_tree_s="0.00", cpu_main_s="0.00")
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
        states = cells(receipt)
        self.assertEqual(states["B2:field-a:settled_ms"]["state"], "LOSS")
        self.assertEqual(states["B2:field-a:settled_ms"]["ahead_or_tied"], ["nemo"])
        self.assertEqual(states["B2:field-a:cpu_s"]["state"], "TIE_CEILING")
        evidence = Evidence()
        for row in evidence.rows["field_a"]:
            row.update(cpu_s="0.00", cpu_tree_s="0.00", cpu_main_s="0.00")
        result, receipt = evidence.write().run()
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(receipt["release_pass"])
        self.assertFalse(receipt["first_everywhere"])

    def test_receipt_is_immutable(self):
        evidence = Evidence().write()
        output = evidence.root / "receipt.json"
        self.assertEqual(evidence.run(output=output)[0].returncode, 0)
        before = output.read_bytes()
        result, _ = evidence.run(output=output)
        self.assertEqual(result.returncode, 2)
        self.assertRegex(result.stderr, "never overwritten")
        self.assertEqual(output.read_bytes(), before)
        csv_bytes = evidence.path("field_a").read_bytes()
        result = subprocess.run(
            [sys.executable, "-B", str(gate), "report", "--a", str(evidence.path("field_a")),
             "--b", str(evidence.path("field_b")), "--engine", str(evidence.path("engine")),
             "--output", str(evidence.path("field_a"))], capture_output=True, text=True, timeout=60,
            stdin=subprocess.DEVNULL)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(evidence.path("field_a").read_bytes(), csv_bytes)


if __name__ == "__main__":
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(GateProvenance)
    outcome = unittest.TextTestRunner(stream=sys.stdout, verbosity=2).run(suite)
    raise SystemExit(0 if outcome.wasSuccessful() else 1)
