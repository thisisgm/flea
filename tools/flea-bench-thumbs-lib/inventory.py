"""Installed entrant identities, including verified B5 release archives."""

import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time

import measure
import versions as version_probe


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def archive_identity(row, binary):
    if row.get("binary_path") != str(binary) or row.get("binary_sha256") != digest(binary):
        raise ValueError("B5 archive path or binary checksum differs")
    if not row.get("url", "").startswith("https://") or not re.fullmatch(r"[0-9a-f]{64}", row.get("sha256", "")):
        raise ValueError("B5 archive URL/checksum is missing")
    for key in ("source_commit", "tag_commit"):
        if not re.fullmatch(r"[0-9a-f]{40}", row.get(key, "")):
            raise ValueError(f"B5 archive {key} is missing")
    if (binary.parent / "SOURCE_COMMIT").read_text().strip() != row["source_commit"]:
        raise ValueError("B5 archive embedded source commit differs")
    check = row.get("version_check", {})
    if check.get("rc") != 0 or check.get("stdout") != row.get("expected_version"):
        raise ValueError("B5 archive version receipt is not verified")
    if row.get("tag") != "v" + row["expected_version"].removeprefix("strata "):
        raise ValueError("B5 archive tag differs from its executable version")
    return dict(row)


def installed(root, b5_root, ids):
    b5 = Path(b5_root)
    if not b5.is_absolute() or b5.resolve() != b5 or not (b5 / ".flea-test-sandbox").is_file():
        raise ValueError("B5 root must be canonical and marked")
    receipt = b5 / "rival-versions.json"
    receipt_bytes = receipt.read_bytes()
    versions = json.loads(receipt_bytes)
    if not 0 <= time.time() - measure.utc_seconds(versions["date_utc"]) <= 86400:
        raise ValueError("B5 version receipt is older than 24 hours")
    archives = {row["id"]: row for row in versions["archives"]}
    rows = []
    private = Path(tempfile.mkdtemp(prefix="inventory-", dir=root))
    measure.directories(private)
    snapshot = private / "b5-rival-versions.json"
    snapshot.write_bytes(receipt_bytes)
    dependencies = subprocess.check_output(["pacman", "-Q", "kdegraphics-thumbnailers"], text=True, timeout=10).strip()
    for ident in ids:
        if ident == "flea":
            continue
        program = "flea" if ident == "flea-0312" else ident
        override = os.environ.get(ident.upper().replace("-", "_") + "_BIN")
        if ident.startswith("strata") and not override:
            raise ValueError(f"{ident}: explicit B5 archive binary is required")
        name = override or shutil.which(program)
        if not name:
            raise ValueError(f"installed entrant is absent: {ident}")
        binary = Path(name).resolve(strict=True)
        identity = version_probe.probe(ident, binary, measure.sandbox(private), measure.environment(private), private)
        version, output = identity["version"], identity["version_output"]
        if ident == "flea-0312" and version != "0.3.12":
            raise ValueError("FLEA_0312_BIN must name the installed 0.3.12 release")
        row = dict(id=ident, binary=str(binary), sha256=digest(binary), build_sha="unknown-installed",
                   checked_utc=time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), **identity,
                   release_url="", channel="installed, latest not asserted",
                   b5_receipt=dict(path=str(snapshot), original_path=str(receipt), sha256=digest(snapshot), date_utc=versions["date_utc"]))
        if ident.startswith("strata"):
            archive = archive_identity(archives.get(ident, {}), binary)
            if output != archive["expected_version"]:
                raise ValueError(f"actual archive version differs: {ident}")
            nightly = "nightly" in version
            if nightly != (ident == "strata-nightly") or re.search(r"(?:rc|alpha|beta)[.\d-]", version, re.I):
                raise ValueError("stable and nightly require their literal release channels, not an RC")
            row.update(source="official release archive", archive=archive, release_url=archive["url"], channel=ident)
        elif ident != "flea-0312":
            package = versions["packages"].get(ident)
            if package is None:
                raise ValueError(f"B5 package receipt is missing: {ident}")
            if package["realpath"] != str(binary) or package["sha256"] != row["sha256"]:
                raise ValueError(f"installed package differs from B5 receipt: {ident}")
            recorded = package.get("package", {})
            if recorded.get("rc") != 0 or recorded.get("stdout") != output:
                raise ValueError(f"package version differs from B5 receipt: {ident}: {output}")
            row["package"] = package
        if ident == "krusader":
            row["thumbnailer_package"] = dependencies
        rows.append(row)
    return rows
