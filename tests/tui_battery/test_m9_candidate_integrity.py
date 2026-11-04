"""Real retained-file and private receiver controls around the existing driver boundary."""
import json
import os
from pathlib import Path
import tempfile
import signal
import socket
import subprocess
import sys
import time
import threading
import unittest
from unittest.mock import patch

from . import candidate, driver, reconcile, review_support
from .bundle import cups
from .evidence import Blocked, MARKER, Owned, digest


class CandidateIntegrityTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-integrity.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned retained identity and cleanup controls\n")

    def catalog(self, identity):
        fx = self.root / "fx"
        (fx / "data").mkdir(parents=True)
        path = fx / "data/catalog.json"
        path.write_text('{"language":"ja"}\n')
        receipt = {"language": "ja", "keyboard_group": "us", "artifacts": {"data/catalog.json": digest(path)}}
        if "candidate_inputs" in identity:
            receipt["candidate_inputs"] = identity["candidate_inputs"]
        def execute(*args, fixture_setup, **kwargs):
            return fixture_setup(fx)
        with patch.object(driver, "execute_pair", side_effect=execute):
            result = candidate.catalog_case(args=None, root=None, display=None, terminal="foot", state={}, variant={},
                colors={}, identity=identity, profiles={}, reviews={}, overrides={}, seams={}, language="ja", keyboard_group="us",
                prepare=lambda fixture: receipt)
        return path, result

    def test_catalog_case_requires_nonempty_candidate_inputs(self):
        with self.assertRaises(Blocked):
            self.catalog({"elf_sha256": "candidate"})

    def test_retained_catalog_bytes_are_sealed_and_tampering_blocks_reconcile(self):
        identity = {"candidate_inputs": {"files": {"keys.toml": "bound", "i18n/ja/flea.json": "bound"}}}
        path, receipt = self.catalog(identity)
        (self.root / "catalog-case.json").write_text(json.dumps(receipt))
        (self.root / "candidate-identity.json").write_text(json.dumps(identity))
        for name in candidate.LIFECYCLE:
            (self.root / name).write_text("{}")
        hashes = {}
        self.assertEqual(candidate.seal_extra(self.root, hashes), [])
        self.assertEqual(hashes["fx/data/catalog.json"], receipt["artifacts"]["data/catalog.json"])
        candidate.verify_extra(self.root, hashes)
        path.write_text('{"language":"ar"}\n')
        with self.assertRaisesRegex(Blocked, "catalog input.*changed"):
            candidate.verify_extra(self.root, hashes)
        with self.assertRaisesRegex(Blocked, "catalog input.*changed"):
            candidate.seal_extra(self.root, {})

    def test_retention_and_sealing_failure_cannot_become_reviewed_pass(self):
        from .test_review_r2 import Review2Tests
        helper = Review2Tests()
        helper.root = self.root
        source, case, identity, row, config = helper.retained_case()
        path = case / "verdict.json"
        original = json.loads(path.read_text())
        for failure in ("fixture_retain_reason", "sealing_reason"):
            path.write_text(json.dumps(dict(original, **{failure: "partial retained fixture"})))
            with patch.object(reconcile, "visual_review", return_value={"status": "PASS", "review": {}}):
                result = reconcile.retained(source, identity, [row], self.root, {},
                           {"foot/tui-list-text/primary": config})[0]
            self.assertEqual(result["status"], "BLOCKED")
            self.assertIn("permanent", result["reason"])

    def test_sealing_exception_is_recorded_and_cannot_be_smoke_pass(self):
        verdict = {"status": "BLOCKED", "visual": {"reason": driver.REVIEW_PENDING},
                   "behavior": {"status": "PASS"}, "cells": {"geometry": {"status": "PASS"}, "color": {"status": "PASS"}},
                   "media": {"status": "PASS"}, "edges": {"status": "PASS"}}
        self.assertTrue(driver.review_pending(verdict))
        with patch.object(driver, "seal", side_effect=Blocked("owned evidence sealing failure")):
            driver.seal_case(self.root, verdict, None)
        self.assertEqual(verdict["sealing_reason"], "owned evidence sealing failure")
        self.assertFalse(driver.review_pending(verdict))

    def test_modern_control_archive_requires_every_sealed_source_file(self):
        case = self.root / "captures/0"
        case.mkdir(parents=True)
        identity = {"candidate_inputs": {"files": {"keys.toml": "bound"}}}
        (case / "candidate-identity.json").write_text(json.dumps(identity))
        for name in candidate.LIFECYCLE:
            (case / name).write_text("{}")
        output = case / "fx/runtime/picture-native/source.dat"
        output.parent.mkdir(parents=True)
        output.write_bytes(b"owned integrity-only fixture\n")
        hashes = {}
        self.assertEqual(candidate.seal_extra(case, hashes), [])
        (case / "verdict.json").write_text(json.dumps({"artifact_sha256": hashes}))
        (self.root / "checks.json").write_text(json.dumps({"archive-integrity-only": [{"capture": "captures/0"}]}))
        candidate.validate_control_inputs(self.root, identity)
        output.write_bytes(b"changed retained source\n")
        with self.assertRaisesRegex(Blocked, "hook artifact changed"):
            candidate.validate_control_inputs(self.root, identity)

    def test_interrupted_terminal_close_still_stops_real_private_receiver_and_retains_fixture(self):
        created = []
        original = cups.PrivateSink
        def sink(*args, **kwargs):
            value = original(*args, **kwargs)
            created.append(value)
            return value
        with patch.object(cups, "PrivateSink", side_effect=sink), \
                patch.object(driver, "close_terminal", side_effect=KeyboardInterrupt):
            with self.assertRaises(KeyboardInterrupt):
                review_support.pair(self.root, seams={"tui-list-text": {"private_cups": True}})
        self.assertEqual(len(created), 1)
        self.assertFalse(created[0].thread.is_alive())
        case = self.root / "foot/tui-list-text/primary"
        self.assertTrue(json.loads((case / "cups-traffic.json").read_text())["thread_joined"])
        self.assertTrue((case / "fx/state/flea/ui.json").is_file())
        self.assertEqual(json.loads((case / "verdict.json").read_text())["status"], "FAIL")

    def test_real_process_sigint_waits_until_the_inflight_receiver_is_joined(self):
        root = Owned(self.root).directory("signal-child")
        (root / MARKER).write_text("owned child signal and IPC control\n")
        script = r'''
import json, pathlib, time
from tui_battery.bundle import cups
from tui_battery.calendar_native import shielded
from tui_battery.evidence import Owned
import sys
root = Owned(pathlib.Path(sys.argv[1])).path
fx = root / "fx"
fx.mkdir()
original = cups.http_request
def receiving(*args, **kwargs):
    (root / "receiving").write_text("actual receiver reading request")
    return original(*args, **kwargs)
cups.http_request = receiving
sink = cups.PrivateSink(root, fx).start()
(root / "socket-ready").write_text("ready")
try:
    with shielded():
        deadline = time.monotonic() + 5
        while not (root / "receiving").exists():
            if time.monotonic() > deadline: raise RuntimeError("receiver never accepted client")
            time.sleep(.01)
        (root / "main-blocked").write_text("inside cleanup shield")
        sink.close()
        (root / "closed").write_text("receiver joined before cancellation")
except KeyboardInterrupt:
    (root / "interrupted").write_text("actual SIGINT delivered")
assert (root / "closed").is_file() and (root / "interrupted").is_file() and not sink.thread.is_alive()
print(json.dumps({"joined_before_interrupt": True}))
'''
        child = subprocess.Popen([sys.executable, "-B", "-c", script, str(root)], stdout=subprocess.PIPE,
                                 stderr=subprocess.PIPE, start_new_session=True)
        descriptor = os.pidfd_open(child.pid)
        client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        client.settimeout(5)
        def wait(name):
            deadline = time.monotonic() + 5
            while not (root / name).is_file():
                if child.poll() is not None or time.monotonic() > deadline:
                    self.fail("owned child never reached " + name)
                time.sleep(.01)
        try:
            wait("socket-ready")
            client.connect(str(root / "fx/runtime/cups.sock"))
            client.sendall(b"POST /printers/flea-private HTTP/1.1\r\nContent-Type: application/ipp\r\nContent-Length: 128\r\n\r\npartial")
            wait("main-blocked")
            signal.pidfd_send_signal(descriptor, signal.SIGINT)
            stdout, stderr = child.communicate(timeout=10)
            self.assertEqual(child.returncode, 0, stderr.decode())
            self.assertTrue(json.loads(stdout)["joined_before_interrupt"])
        finally:
            client.close()
            if child.poll() is None:
                signal.pidfd_send_signal(descriptor, signal.SIGKILL)
                child.wait(5)
            os.close(descriptor)

    def test_startup_pending_sigint_joins_receiver_and_preserves_driver_ownership(self):
        created = []
        original_sink, original_start = cups.PrivateSink, threading.Thread.start
        def sink(*args, **kwargs):
            value = original_sink(*args, **kwargs)
            created.append(value)
            return value
        def start(thread):
            original_start(thread)
            signal.raise_signal(signal.SIGINT)
        with patch.object(cups, "PrivateSink", side_effect=sink), \
                patch.object(threading.Thread, "start", new=start):
            with self.assertRaises(KeyboardInterrupt):
                review_support.pair(self.root, seams={"tui-list-text": {"private_cups": True}})
        self.assertEqual(len(created), 1)
        self.assertFalse(created[0].thread.is_alive())
        case = self.root / "foot/tui-list-text/primary"
        failure = json.loads((case / "cups-start-failure.json").read_text())
        self.assertTrue(failure["thread_started"])
        self.assertTrue(failure["thread_joined"])
        traffic = json.loads((case / "cups-traffic.json").read_text())
        self.assertTrue(traffic["thread_joined"])
        self.assertTrue(traffic["errors"])
        self.assertTrue((case / "fx").is_dir())
        self.assertEqual(json.loads((case / "verdict.json").read_text())["status"], "FAIL")


if __name__ == "__main__":
    unittest.main()
