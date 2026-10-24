"""Content-bound UISH harness receipts with separate premises, assertions and native debt."""
import hashlib
import json
from pathlib import Path
import subprocess

from flea_uish_fixtures import Refused, digest, regular_bytes, require_decoder_fixture, FixturePremiseError

SOURCE_FILES = ("tests/lib/flea_uish_fixtures.py", "tests/lib/flea_uish_receipts.py",
                "tests/uish-fixture-controls.py", "tests/ui-uish-fixtures.sh",
                "tools/flea-sandbox-guard", "tests/lib/flea_native_inputs.py", "tests/tui_battery/png.py",
                "tests/ui.sh", "tests/ui-dirsortstale.sh", "tests/ui-preview-policy.sh")


class ReceiptError(RuntimeError):
    """The receipt or one of the inputs/artifacts it names has changed."""


def content_hash(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":")).encode()).hexdigest()


def source_receipt(workspace):
    workspace = Path(workspace)
    identity = {"kind": "source-files", "commit": None,
                "files": {name: digest(workspace / name) for name in SOURCE_FILES}}
    metadata = workspace / ".git"
    if metadata.exists() or metadata.is_symlink():
        try:
            command = subprocess.run(["git", "-C", str(workspace), "rev-parse", "HEAD"],
                                     check=True, capture_output=True, text=True, timeout=5)
        except (OSError, subprocess.SubprocessError) as error:
            raise ReceiptError("present Git metadata cannot supply source identity") from error
        identity.update(kind="git-worktree", commit=command.stdout.strip())
    return identity


def capabilities():
    return {"fixture_files": True, "executable_doubles": True, "backend_schedule": True,
            "gpui_window": False, "app_dispatch": False, "ipc": False, "native_input": False,
            "wayland_dnd": False, "pixels": False, "media_frames": False, "presented_frames": False}


def fixture_manifest(fixture):
    names = []
    for directory in ("listing", "dirs", "urls", "sources", "bin", "doubles", "home", "data/thumbnailers"):
        path = fixture.path(directory)
        if path.exists():
            for entry in path.rglob("*"):
                if entry.is_symlink():
                    raise Refused("fixture manifest cannot follow a symlink")
                if entry.is_file():
                    names.append(str(entry.relative_to(fixture.root)))
    return fixture.inventory(names)


def record(fixture, case, assertions, required=("fixture_files",), decoder=True, artifacts=()):
    """A passing fixture receipt is never an application or native acceptance receipt."""
    if not case or not all(c.isalnum() or c in "-_" for c in case):
        raise ReceiptError("invalid receipt case name")
    if not assertions or any(not isinstance(key, str) or type(value) is not bool for key, value in assertions.items()):
        raise ReceiptError("receipt requires named boolean assertions")
    premise = {"status": "PASS"}
    if decoder:
        try:
            premise["decoder"] = require_decoder_fixture(fixture)
        except FixturePremiseError as error:
            premise = {"status": "BLOCKED", "reason": str(error)}
    supported = capabilities()
    missing = [name for name in required if not supported.get(name, False)]
    status = "HARNESS_PASS_NATIVE_PENDING"
    assertion = {"status": "PASS" if all(assertions.values()) else "FAIL", "checks": assertions}
    if premise["status"] != "PASS":
        status, assertion = "FIXTURE_BLOCKED", {"status": "NOT_RUN", "checks": {}}
    elif missing:
        status, assertion = "DRIVER_UNSUPPORTED", {"status": "NOT_RUN", "checks": {}}
    elif not all(assertions.values()):
        status = "ASSERTION_FAILED"
    payload = {"schema": 1, "case": case, "status": status,
               "source_receipt": source_receipt(fixture.workspace),
               "fixture": {"root": str(fixture.root), "manifest": fixture_manifest(fixture), "premise": premise,
                           "presentation": json.loads(regular_bytes(fixture.path("presentation.json")))},
               "driver": {"scope": "fixture-harness", "capabilities": supported, "required": list(required), "missing": missing},
               "assertion": assertion, "artifacts": fixture.inventory(artifacts),
               "native_evidence": {"status": "PENDING", "owner": "release-controller"}}
    envelope = {"payload": payload, "sha256": content_hash(payload)}
    path = fixture.write(f"evidence/{case}.receipt.json", json.dumps(envelope, indent=2, sort_keys=True) + "\n")
    return path


def verify(fixture, receipt):
    try:
        path = fixture.path(Path(receipt).relative_to(fixture.root))
        envelope = json.loads(regular_bytes(path, 1024 * 1024))
        payload = envelope["payload"]
        if envelope["sha256"] != content_hash(payload):
            raise ReceiptError("changed receipt hash")
        if payload["schema"] != 1 or payload["source_receipt"] != source_receipt(fixture.workspace):
            raise ReceiptError("changed source identity")
        if payload["fixture"]["root"] != str(fixture.root) or payload["fixture"]["manifest"] != fixture_manifest(fixture):
            raise ReceiptError("changed fixture identity")
        if payload["fixture"]["presentation"] != json.loads(regular_bytes(fixture.path("presentation.json"))):
            raise ReceiptError("changed presentation identity")
        if payload["artifacts"] != fixture.inventory(payload["artifacts"]):
            raise ReceiptError("changed evidence artifact hash")
        if payload["native_evidence"] != {"status": "PENDING", "owner": "release-controller"}:
            raise ReceiptError("fixture receipt cannot grant native acceptance")
        return payload
    except (OSError, ValueError, KeyError, TypeError, Refused) as error:
        raise ReceiptError(f"unreadable or escaped receipt: {error}") from error
