#!/usr/bin/env python3
"""Exercise fixture ownership, protocol doubles and receipts without granting GPUI/native coverage."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest

REPO = Path(__file__).resolve().parents[1]
sys.path[:0] = [str(REPO / "tests/lib"), str(REPO / "tests")]

from flea_uish_fixtures import (FixtureRoot, MARKER, Refused, digest, require_decoder_fixture,
                                smoke_fixture, validate_schedule)
from flea_uish_receipts import ReceiptError, SOURCE_FILES, capabilities, record, source_receipt, verify


def exchange(fixture, spec, requests):
    executable = fixture.install_double("dirsize-backend", spec)
    child = fixture.spawn([str(executable), "--backend"], stdin=subprocess.PIPE,
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
    try:
        output, error = child.communicate("".join(json.dumps(value) + "\n" for value in requests), timeout=7)
        if child.returncode:
            raise AssertionError(f"backend double rc={child.returncode}: {error}")
        return [json.loads(line) for line in output.splitlines()]
    finally:
        fixture.stop(child.pid)


def stale_schedule():
    requests = [{"c": "dirsize", "listing": 1, "rows": [0]}, {"c": "sort"},
                {"c": "dirsize", "listing": 2, "rows": [0]}]
    old = {"t": "dirsized", "row": 0, "bytes": 4097, "partial": False, "ms": 0.0}
    new = dict(old, bytes=4353)
    return requests, {"mode": "backend", "steps": [
        {"expect": requests[0]}, {"expect": requests[1], "delay_ms": 50, "replies": [old]},
        {"expect": requests[2], "delay_ms": 10, "replies": [new]}]}


def assert_stale_premise(replies):
    if [reply.get("bytes") for reply in replies] != [4097, 4353]:
        raise AssertionError("stale/new fixture replies are missing, reordered or indistinguishable")


def negative(name):
    fixture = FixtureRoot.create(REPO)
    try:
        smoke_fixture(fixture)
        if name == "missing-marker":
            marker = fixture.root / MARKER
            backup = fixture.root / "saved-marker"
            marker.rename(backup)
            try:
                fixture.remove(str(fixture.root))
            finally:
                backup.rename(marker)
        elif name == "escaped-symlink":
            fixture.path("escape").symlink_to(REPO, target_is_directory=True)
            fixture.remove(str(fixture.root / "escape"))
        elif name in ("relative-cleanup", "empty-cleanup"):
            fixture.remove("relative" if name == "relative-cleanup" else "")
        elif name == "foreign-pid":
            fixture.stop(os.getppid())
        elif name == "stale-reply":
            requests, spec = stale_schedule()
            spec["steps"][1]["replies"][0]["bytes"] = 4353
            assert_stale_premise(exchange(fixture, spec, requests))
        elif name == "missing-decoder":
            fixture.remove(str(fixture.path("sources/image.png")))
            payload = verify(fixture, record(fixture, name, {"unreachable-product-assertion": False}))
            print(json.dumps({"status": payload["status"], "assertion": payload["assertion"]}))
            return 2 if payload["status"] == "FIXTURE_BLOCKED" else 0
        elif name == "changed-receipt":
            path = record(fixture, name, {"fixture-ready": True})
            data = json.loads(path.read_text())
            data["payload"]["assertion"]["checks"]["fixture-ready"] = False
            fixture.remove(str(path))
            fixture.write(path.relative_to(fixture.root), json.dumps(data))
            verify(fixture, path)
        return 0
    except Refused as error:
        print(f"FIXTURE_REFUSED: {error}")
        return 64
    except ReceiptError as error:
        print(f"RECEIPT_REFUSED: {error}")
        return 4
    except AssertionError as error:
        print(f"ASSERTION_REJECTED: {error}")
        return 1
    finally:
        for pid in list(fixture.children):
            fixture.stop(pid)
        fixture.remove(str(fixture.root))


class Controls(unittest.TestCase):
    def setUp(self):
        self.fixture = FixtureRoot.create(REPO)

    def tearDown(self):
        for pid in list(self.fixture.children):
            self.fixture.stop(pid)
        self.fixture.remove(str(self.fixture.root))

    def test_private_environment_has_no_operator_inputs(self):
        environment = self.fixture.environment()
        for name in ("HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME",
                     "XDG_RUNTIME_DIR", "FLEA_UNDO_DIR", "TMPDIR"):
            self.assertTrue(Path(environment[name]).is_relative_to(self.fixture.root), name)
        self.assertEqual(self.fixture.root.stat().st_mode & 0o777, 0o700)
        self.assertEqual(Path(environment["XDG_RUNTIME_DIR"]).stat().st_mode & 0o777, 0o700)
        self.assertNotIn("DBUS_SESSION_BUS_ADDRESS", environment)
        self.assertNotIn("WAYLAND_DISPLAY", environment)
        child = self.fixture.spawn([sys.executable, "-B", "-c",
                                    "import os,json; print(json.dumps(dict(os.environ)))"], stdout=subprocess.PIPE)
        actual = json.loads(child.communicate(timeout=3)[0])
        self.fixture.stop(child.pid)
        self.assertEqual(actual, environment)

    def test_deterministic_payloads_and_declared_palette(self):
        palette = smoke_fixture(self.fixture)
        self.assertEqual(palette["kind"], "declared-fixture-palette")
        self.assertFalse(palette["operator_theme"])
        names = ("listing/alpha.txt", "listing/zeta.txt", "dirs/aaa/payload", "dirs/zzz/payload", "sources/image.png")
        second = FixtureRoot.create(REPO)
        try:
            smoke_fixture(second)
            self.assertEqual(self.fixture.inventory(names), second.inventory(names))
            self.assertEqual(require_decoder_fixture(self.fixture)["width"], 64)
            self.assertNotEqual(digest(self.fixture.path("dirs/aaa/payload")), digest(self.fixture.path("dirs/zzz/payload")))
        finally:
            second.remove(str(second.root))

    def test_copied_inputs_preserve_bytes_and_absence(self):
        source = self.fixture.directory("input-home")
        names = (".local/state/omarchy/current/theme/colors.toml",
                 ".local/state/omarchy/current/theme/shell.toml",
                 ".local/state/omarchy/current/theme/icons.theme", ".local/state/omarchy/current/theme.name",
                 ".config/omarchy/shell.toml", ".config/fontconfig/fonts.conf",
                 ".config/fontconfig/conf.d/50-omarchy-input-method.conf")
        for index, name in enumerate(names):
            self.fixture.write("input-home/" + name, f"exact input {index}\n")
        self.fixture.presentation(source)
        for name in names:
            copied = self.fixture.path("home/" + name)
            self.assertEqual(copied.read_bytes(), (source / name).read_bytes())
            self.assertFalse(copied.is_symlink())
            self.assertNotEqual(copied.stat().st_ino, (source / name).stat().st_ino)
        with self.assertRaisesRegex(Refused, "new files"):
            self.fixture.presentation(source)
        other = FixtureRoot.create(REPO)
        try:
            other.presentation(other.directory("empty-input-home"))
            self.assertFalse(other.path("home/.local/state/omarchy/current/theme/icons.theme").exists())
        finally:
            other.remove(str(other.root))

    def test_user_thumbnailer_uses_registered_protocol_and_private_cache(self):
        smoke_fixture(self.fixture)
        output = self.fixture.path("c#a?che/thumbnails/large/control.png")
        program = self.fixture.path("bin/drive-url-thumbnail")
        registration = self.fixture.path("data/thumbnailers/url.thumbnailer").read_text()
        self.assertIn(f"Exec={program} %i %o %s", registration)
        child = self.fixture.spawn([str(program), str(self.fixture.path("urls/a.url")), str(output), "256"],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        _, error = child.communicate(timeout=5)
        self.fixture.stop(child.pid)
        self.assertEqual(child.returncode, 0, error)
        self.assertEqual(output.read_bytes(), self.fixture.path("sources/image.png").read_bytes())
        child = self.fixture.spawn([str(program), str(self.fixture.path("urls/a.url")), str(output), "256"],
                                   stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        _, error = child.communicate(timeout=5)
        self.fixture.stop(child.pid)
        self.assertEqual(child.returncode, 0, error)
        self.assertEqual(output.read_bytes(), self.fixture.path("sources/image.png").read_bytes())

    def test_escaped_cleanup_preserves_foreign_marked_root(self):
        foreign = FixtureRoot.create(REPO)
        try:
            sentinel = foreign.write("sentinel", "unchanged\n")
            self.fixture.path("escape").symlink_to(foreign.root, target_is_directory=True)
            with self.assertRaises(Refused):
                self.fixture.remove(str(self.fixture.root / "escape" / "sentinel"))
            self.assertEqual(sentinel.read_text(), "unchanged\n")
        finally:
            foreign.remove(str(foreign.root))

    def test_foreign_workspace_is_refused_before_creation(self):
        with self.assertRaisesRegex(Refused, "own worktree"):
            FixtureRoot.create(self.fixture.root)
        self.assertFalse((self.fixture.root / ".superpowers").exists())

    def test_executable_doubles_preserve_stdout_stderr_status_and_exact_argv(self):
        for name in ("network", "device", "provider", "opener"):
            program = self.fixture.install_double(name, {"mode": "argv", "steps": [
                {"expect": ["owned input"], "stdout": "protocol output\n", "stderr": "owned diagnostic\n", "exit": 7}]})
            child = self.fixture.spawn([str(program), "owned input"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            output, error = child.communicate(timeout=5)
            self.fixture.stop(child.pid)
            self.assertEqual((child.returncode, output, error), (7, b"protocol output\n", b"owned diagnostic\n"))
        child = self.fixture.spawn([str(program), "unexpected"], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        _, error = child.communicate(timeout=5)
        self.fixture.stop(child.pid)
        self.assertEqual(child.returncode, 64)
        self.assertIn(b"unexpected opener request", error)

    def test_delayed_stale_reply_reaches_the_real_executable_pipe(self):
        requests, spec = stale_schedule()
        self.assertGreater(spec["steps"][1]["delay_ms"], 0)
        replies = exchange(self.fixture, spec, requests)
        assert_stale_premise(replies)
        self.assertEqual([reply["t"] for reply in replies], ["dirsized", "dirsized"])

    def test_schedules_refuse_unbounded_work(self):
        for delay in (-1, 251, True):
            with self.subTest(delay=delay), self.assertRaises(Refused):
                validate_schedule({"mode": "backend", "steps": [{"expect": {"c": "dirsize"}, "delay_ms": delay}]})
        with self.assertRaises(Refused):
            validate_schedule({"mode": "backend", "steps": [{"expect": {"c": "dirsize"}, "delay_ms": 250}] * 5})
        with self.assertRaises(Refused):
            validate_schedule({"mode": "argv", "steps": []})
        for value in ([], {"mode": "backend", "steps": [None]}, {"mode": "argv", "steps": "bad"},
                      {"mode": "backend", "steps": [{"expect": {"c": "dirsize"}, "replies": ["bad"]}]}):
            with self.subTest(value=value), self.assertRaises(Refused):
                validate_schedule(value)

    def test_nonzero_refusal_twins(self):
        cases = {"missing-marker": 64, "escaped-symlink": 64, "relative-cleanup": 64,
                 "empty-cleanup": 64, "foreign-pid": 64, "stale-reply": 1,
                 "missing-decoder": 2, "changed-receipt": 4}
        for name, expected in cases.items():
            with self.subTest(control=name):
                child = self.fixture.spawn([sys.executable, "-B", str(Path(__file__).resolve()), "--negative", name],
                                           stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                output, error = child.communicate(timeout=10)
                self.fixture.stop(child.pid)
                self.assertEqual(child.returncode, expected, output + error)
                if name == "missing-decoder":
                    payload = json.loads(output)
                    self.assertEqual(payload["status"], "FIXTURE_BLOCKED")
                    self.assertEqual(payload["assertion"]["status"], "NOT_RUN")
                print(f"CONTROL {name} expected_rc={expected} actual_rc={child.returncode}")

    def test_receipts_separate_source_premise_driver_assertion_and_native(self):
        smoke_fixture(self.fixture)
        evidence = self.fixture.write("evidence/pipe.log", "observed fixture pipe output\n")
        path = record(self.fixture, "fixture-smoke", {"fixture-ready": True}, artifacts=("evidence/pipe.log",))
        payload = verify(self.fixture, path)
        self.assertEqual(payload["status"], "HARNESS_PASS_NATIVE_PENDING")
        self.assertEqual(payload["native_evidence"]["status"], "PENDING")
        self.assertEqual(payload["driver"]["scope"], "fixture-harness")
        for name in ("gpui_window", "app_dispatch", "ipc", "native_input", "wayland_dnd", "pixels", "media_frames", "presented_frames"):
            self.assertFalse(capabilities()[name])
        unsupported = record(self.fixture, "native-required", {"not-run": False}, required=("pixels",))
        self.assertEqual(verify(self.fixture, unsupported)["status"], "DRIVER_UNSUPPORTED")
        failed = record(self.fixture, "assertion-failure", {"distinct-values": False})
        self.assertEqual(verify(self.fixture, failed)["status"], "ASSERTION_FAILED")
        self.fixture.remove(str(evidence))
        self.fixture.write("evidence/pipe.log", "changed output\n")
        with self.assertRaisesRegex(ReceiptError, "artifact hash"):
            verify(self.fixture, path)

    def test_receipts_reject_changed_fixture_and_source(self):
        smoke_fixture(self.fixture)
        path = record(self.fixture, "changed-fixture", {"fixture-ready": True})
        payload_file = self.fixture.path("listing/alpha.txt")
        self.fixture.remove(str(payload_file))
        self.fixture.write("listing/alpha.txt", "changed\n")
        with self.assertRaisesRegex(ReceiptError, "fixture identity"):
            verify(self.fixture, path)
        path = record(self.fixture, "changed-source", {"fixture-ready": True})
        from unittest.mock import patch
        with patch("flea_uish_receipts.source_receipt", return_value={"commit": "other", "files": {}}):
            with self.assertRaisesRegex(ReceiptError, "source identity"):
                verify(self.fixture, path)

    def test_archive_identity_keeps_source_hashes_without_claiming_a_commit(self):
        from unittest.mock import patch
        archive = self.fixture.directory("archive")
        for name in SOURCE_FILES:
            self.fixture.write("archive/" + name, (REPO / name).read_bytes())
        expected = source_receipt(REPO)["files"]
        with patch("flea_uish_receipts.subprocess.run", side_effect=AssertionError("archive must not inspect ancestor Git metadata")):
            identity = source_receipt(archive)
        self.assertEqual(identity, {"kind": "source-files", "commit": None, "files": expected})
        self.fixture.write("archive/.git", "gitdir: missing\n")
        with self.assertRaisesRegex(ReceiptError, "present Git metadata"):
            source_receipt(archive)

    def test_source_identity_matches_the_actual_checkout_or_archive(self):
        identity = source_receipt(REPO)
        self.assertEqual(set(identity["files"]), set(SOURCE_FILES))
        if (REPO / ".git").exists():
            self.assertEqual(identity["kind"], "git-worktree")
            self.assertRegex(identity["commit"], r"^[0-9a-f]{40}$")
        else:
            self.assertEqual(identity["kind"], "source-files")
            self.assertIsNone(identity["commit"])
        print(f"SOURCE_IDENTITY={identity['kind']} files={len(identity['files'])}")

    def test_owned_process_drain_and_cleanup(self):
        child = self.fixture.spawn([sys.executable, "-B", "-c", "import time; time.sleep(30)"])
        with self.assertRaisesRegex(Refused, "drain"):
            self.fixture.remove(str(self.fixture.root))
        self.assertIsInstance(self.fixture.stop(child.pid), int)
        self.assertIsNotNone(child.returncode)
        with self.assertRaisesRegex(Refused, "foreign PID"):
            self.fixture.stop(child.pid)

    def test_ci_execution_contract(self):
        if os.environ.get("FLEA_UISH_CI") == "1":
            with self.assertRaises(OSError):
                os.open("/dev/tty", os.O_RDONLY)
            self.assertEqual(os.readlink("/proc/self/fd/0"), "/dev/null")
            self.assertNotEqual(os.readlink("/proc/self/ns/pid"), os.environ["FLEA_UISH_PARENT_PIDNS"])
            for name in ("HOME", "XDG_CONFIG_HOME", "XDG_CACHE_HOME", "XDG_RUNTIME_DIR", "FLEA_UNDO_DIR", "TMPDIR"):
                self.assertTrue(Path(os.environ[name]).is_relative_to(REPO / ".superpowers" / "m9-uish-03"))
        else:
            self.assertIsNone(os.environ.get("FLEA_UISH_CI"))


def ci_launch():
    fixture = FixtureRoot.create(REPO)
    try:
        environment = fixture.environment()
        environment.update(FLEA_UISH_CI="1", FLEA_UISH_PARENT_PIDNS=os.readlink("/proc/self/ns/pid"))
        command = ["bwrap", "--unshare-pid", "--die-with-parent", "--dev-bind", "/", "/", "--proc", "/proc",
                   "--", "setsid", "-w", "env", "-i"]
        command.extend(f"{key}={value}" for key, value in environment.items())
        command += [sys.executable, "-B", str(Path(__file__).resolve())]
        with open("/dev/null", "rb") as input_stream:
            return subprocess.run(command, stdin=input_stream, cwd=REPO, timeout=90).returncode
    finally:
        fixture.remove(str(fixture.root))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--negative", choices=("missing-marker", "escaped-symlink", "relative-cleanup", "empty-cleanup",
                                               "foreign-pid", "stale-reply", "missing-decoder", "changed-receipt"))
    parser.add_argument("--ci-launch", action="store_true")
    options = parser.parse_args()
    if options.negative:
        raise SystemExit(negative(options.negative))
    if options.ci_launch:
        raise SystemExit(ci_launch())
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(Controls)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    if result.wasSuccessful() and result.testsRun:
        print("CI_ISOLATION=" + ("exercised" if os.environ.get("FLEA_UISH_CI") == "1" else "not-exercised"))
        print(f"PASS ui-uish-fixtures: {result.testsRun} controls; eight known-nonzero twins; native acceptance pending")
    raise SystemExit(0 if result.wasSuccessful() and result.testsRun else 1)
