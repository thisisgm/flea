"""Run the complete offline suite with pinned references and private test roots."""
import hashlib
import json
import os
from pathlib import Path
import sys
import tarfile
import unittest

from .adapters import TERMINALS
from .driver import main as battery
from .evidence import Blocked, Owned, digest

HERE = Path(__file__).resolve().parent


def references(owner):
    fixture = HERE / "fixtures"
    manifest = json.loads((fixture / "references.json").read_text())
    archive = fixture / "references.tar.xz"
    if manifest.get("schema") != 1 or digest(archive) != manifest.get("archive_sha256"):
        raise Blocked("offline reference archive identity mismatch")
    records = manifest["files"]
    if not records or sum(row["size"] for row in records.values()) > 32 * 1024 * 1024:
        raise Blocked("offline reference snapshot exceeds its byte budget")
    root = owner.directory("references")
    with tarfile.open(archive, "r:xz") as source:
        seen = set()
        for member in source:
            path = Path(member.name)
            if (not member.isfile() or member.name not in records or member.name in seen
                    or path.is_absolute() or ".." in path.parts or path.parts[0] != "boards"):
                raise Blocked("offline reference archive contains an unexpected entry")
            entry = records[member.name]
            if member.size != entry["size"]:
                raise Blocked("offline reference member size mismatch")
            data = source.extractfile(member).read(member.size + 1)
            if len(data) != member.size or hashlib.sha256(data).hexdigest() != entry["sha256"]:
                raise Blocked("offline reference member identity mismatch")
            target = owner.child("references/" + member.name)
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("xb") as output:
                output.write(data)
            seen.add(member.name)
    if seen != set(records):
        raise Blocked("offline reference snapshot is incomplete")
    return root


def main(scratch):
    owner = Owned(Path(scratch))
    if os.isatty(0):
        raise Blocked("offline suite stdin must not be a terminal")
    try:
        descriptor = os.open("/dev/tty", os.O_RDONLY | os.O_NOCTTY)
    except OSError:
        pass
    else:
        os.close(descriptor)
        raise Blocked("offline suite must have no controlling terminal")
    boards = references(owner)
    print("tui-battery: private environment, no controlling tty, pinned references, native capture disabled", flush=True)
    if (os.environ.get("FLEA_BATTERY_BOARDS") != str(boards)
            or os.environ.get("FLEA_BATTERY_TEST_ROOT") != str(owner.child("tests"))):
        raise Blocked("offline inputs must use the wrapper's marked scratch root")
    suite = unittest.defaultTestLoader.discover(str(HERE), pattern="test_*.py", top_level_dir=str(HERE.parent))
    count = suite.countTestCases()
    if not count:
        raise Blocked("offline regression discovery returned no tests")
    result = unittest.TextTestRunner(verbosity=1).run(suite)
    if not result.wasSuccessful() or result.skipped or result.testsRun != count:
        return 1
    output = owner.child("diagnostic")
    code = battery(["--boards", str(boards), "--output", str(output), "--dry-run"])
    if code:
        return code
    manifest = json.loads((output / "manifest.json").read_text())
    records = manifest["records"]
    if (manifest["status"] != "DRY-RUN-PASS" or manifest["native_captures"] != 0
            or manifest["native_acceptance"] is not False or len(records) != len(TERMINALS)
            or {row["adapter"] for row in records} != set(TERMINALS)
            or any(row["status"] != "ADAPTER-PASS" or row["exit_code"] != 0 or row["fake"] is not True for row in records)):
        raise Blocked("offline fake-adapter diagnostics did not complete")
    print(f"PASS tui-battery: {result.testsRun} regressions, {len(records)} fake adapters")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv[1]))
    except (Blocked, OSError, ValueError, KeyError, tarfile.TarError) as error:
        print("FAIL tui-battery: " + str(error), file=sys.stderr)
        sys.exit(1)
