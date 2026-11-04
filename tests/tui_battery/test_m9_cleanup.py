"""Exercise the owned driver's real exceptional cleanup without native or host input."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from . import calendar_native, candidate, driver, native
from .evidence import Blocked, MARKER


class CleanupIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-cleanup.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned process cleanup regression\n")
        self.children = []
        self.addCleanup(self.reap_owned)

    def reap_owned(self):
        for child in self.children:
            if child.poll() is None:
                child.kill()
            child.wait(5)

    def child(self):
        child = subprocess.Popen([sys.executable, "-B", "-c", "import time; time.sleep(30)"], start_new_session=True)
        self.children.append(child)
        return child

    def test_actual_driver_reaps_owned_group_after_close_failure(self):
        child, stranger = self.child(), self.child()
        (self.root / "launch-start.json").write_text(json.dumps({"pid": child.pid, "start_ns": time.time_ns()}))
        class Terminal:
            def close(self):
                raise Blocked("deliberate close failure")
        with patch.object(os, "killpg", side_effect=AssertionError("unanchored group signal")), \
                patch.object(os, "kill", side_effect=AssertionError("numeric process signal")), \
                patch.object(calendar_native, "TERM_GRACE", .5), patch.object(calendar_native, "KILL_GRACE", .5):
            with self.assertRaises(Blocked):
                driver.close_terminal(Terminal(), self.root)
        child.wait(5)
        self.assertIsNone(stranger.poll(), "unowned group must remain alive")
        receipt = json.loads((self.root / "cleanup.json").read_text())
        self.assertFalse(receipt["live"], receipt)
        self.assertEqual(receipt["pid"], child.pid)
        self.assertTrue(receipt["signals"], receipt)

    def test_wrong_start_identity_never_signals_named_process(self):
        stranger = self.child()
        (self.root / "launch-start.json").write_text(json.dumps({"pid": stranger.pid, "start_ns": 1}))
        class Terminal:
            def close(self):
                return {"exit_code": 0}
        with patch.object(os, "killpg", side_effect=AssertionError("numeric group signal")), \
                patch.object(signal, "pidfd_send_signal", side_effect=AssertionError("unowned pidfd signal")):
            with self.assertRaisesRegex(Blocked, "outlived close"):
                driver.close_terminal(Terminal(), self.root)
        self.assertIsNone(stranger.poll())
        receipt = json.loads((self.root / "cleanup.json").read_text())
        self.assertEqual(receipt["signals"], [])
        self.assertTrue(receipt["live"])

    def test_native_close_exception_has_no_numeric_kill_fallback(self):
        stranger = self.child()
        (self.root / "launch-start.json").write_text(json.dumps({"pid": stranger.pid, "start_ns": 1}))
        class Adapter:
            def close(self, commands):
                self.closed = True
        adapter = Adapter()
        terminal = native.Terminal(None, adapter, self.root, None, .1)
        with patch.object(native, "wait_for", side_effect=Blocked("close timeout")), \
                patch.object(os, "killpg", side_effect=AssertionError("numeric fallback reached")):
            with self.assertRaisesRegex(Blocked, "close timeout"):
                terminal.close()
        self.assertTrue(adapter.closed)
        self.assertIsNone(stranger.poll())

    def test_lifecycle_and_hooks_are_sealed_and_tampering_blocks(self):
        hashes = {}
        self.assertEqual(set(candidate.seal_extra(self.root, hashes)), set(candidate.LIFECYCLE))
        for name in candidate.LIFECYCLE + candidate.HOOKS:
            (self.root / name).write_text('{"observed":true}\n')
        # The catalog hook is only a receipt when it names the candidate's inputs and its private artifacts.
        inputs = {"files": {"keys.toml": "bound"}}
        artifact = self.root / "fx/data/catalog.json"
        artifact.parent.mkdir(parents=True)
        artifact.write_text('{"language":"ja"}\n')
        (self.root / "candidate-identity.json").write_text(json.dumps({"candidate_inputs": inputs}))
        (self.root / "catalog-case.json").write_text(json.dumps(
            {"candidate_inputs": inputs, "artifacts": {"data/catalog.json": candidate.digest(artifact)}}))
        self.assertEqual(candidate.seal_extra(self.root, hashes), [])
        candidate.verify_extra(self.root, hashes)
        (self.root / "disabled-state.json").write_text('{"observed":false}\n')
        with self.assertRaisesRegex(Blocked, "hook artifact changed"):
            candidate.verify_extra(self.root, hashes)


if __name__ == "__main__":
    unittest.main()
