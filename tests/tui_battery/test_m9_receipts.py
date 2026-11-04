"""Supplement acceptance must execute the accepted producer validator."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from . import receipts, control_native
from .evidence import Blocked, MARKER

class ReceiptIntegrationTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp(prefix="m9-receipts.", dir=os.environ["FLEA_BATTERY_TEST_ROOT"]))
        (self.root / MARKER).write_text("owned receipt regression\n")

    def test_control_validator_cannot_be_bypassed_by_passing_assertions(self):
        identity = {key: "identity" for key in ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked")}
        control = {"id": "query-native", "checks": ["native-positive"]}
        (self.root / "manifest.json").write_text(json.dumps({"identity": identity, "records": [
            {"terminal": "foot", "kind": "control", "id": "query-native", "directory": "foot/query-native"}]}))
        with patch.object(control_native, "validate_record", side_effect=Blocked("validator negative")) as validate:
            result = receipts.supplement(self.root, identity, ["foot"], [control], {"states": []})
        validate.assert_called_once()
        self.assertEqual(result[0]["status"], "BLOCKED")
        self.assertIn("validator negative", result[0]["reason"])

    def test_supplement_rejects_stale_untracked_identity_and_unexpected_records(self):
        identity = {key: "identity" for key in ("commit", "dirty_patch_sha256", "elf_sha256", "references", "untracked")}
        for altered in ({"identity": dict(identity, untracked="changed"), "records": []},
                        {"identity": identity, "records": [{"terminal": "foot", "kind": "surface", "id": "unknown"}]}):
            (self.root / "manifest.json").write_text(json.dumps(altered))
            with self.assertRaises(Blocked):
                receipts.supplement(self.root, identity, ["foot"], [], {"states": []})


