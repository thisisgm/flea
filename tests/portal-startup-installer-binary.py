#!/usr/bin/env python3
"""Verify installer binary selection in a marked synthetic repository."""

import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


REPO = Path(__file__).resolve().parent.parent
SCRATCH = REPO / ".superpowers/k123"
SCRATCH.mkdir(parents=True, exist_ok=True)
ROOT = Path(tempfile.mkdtemp(prefix="portal-installer-binary.", dir=SCRATCH)).resolve()
MARKER = ROOT / ".flea-test-sandbox"
MARKER.touch()


try:
    repo = ROOT / "repo"
    for directory in ["tests", "packaging", "target/release", "target/debug"]:
        (repo / directory).mkdir(parents=True)
    script = repo / "tests/portal-startup-installer.py"
    shutil.copyfile(REPO / "tests/portal-startup-installer.py", script)
    shutil.copyfile(REPO / "packaging/flea.portal", repo / "packaging/flea.portal")
    binaries = [repo / "target/release/flea", repo / "target/debug/flea", ROOT / "custom-flea"]
    for binary in binaries:
        binary.write_text(
            f"#!{sys.executable}\nimport os, pathlib, sys\n"
            "pathlib.Path(os.environ['PORTAL_BINARY_CAPTURE']).write_text(sys.argv[0])\n"
            "sys.exit(42)\n")
        binary.chmod(0o700)
    failures = []
    for index, (label, override, expected) in enumerate([
        ("defaults to the release binary built by run-all", None, binaries[0]),
        ("honors an explicit FLEA_BIN", binaries[2], binaries[2]),
        ("refuses a missing binary without a debug or release fallback", ROOT / "absent-flea", None),
    ]):
        capture = ROOT / f"selected-{index}"
        env = dict(os.environ, PORTAL_BINARY_CAPTURE=str(capture))
        env.pop("FLEA_BIN", None)
        if override is not None:
            env["FLEA_BIN"] = str(override)
        result = subprocess.run([sys.executable, str(script)], env=env, stdout=subprocess.PIPE,
                                stderr=subprocess.STDOUT, text=True, timeout=30)
        selected = capture.read_text() if capture.exists() else ""
        passed = selected == str(expected) if expected is not None else (
            not selected and result.returncode != 0 and "installer binary is missing or not executable" in result.stdout)
        print(f"{'ok' if passed else 'FAIL'} installer {label}: exit={result.returncode}, selected={selected}", flush=True)
        if not passed:
            failures.append(label)
    print(f"portal-startup-installer-binary: {len(failures)} failures", flush=True)
    status = int(bool(failures))
finally:
    if not (ROOT.is_absolute() and ROOT.parent == SCRATCH.resolve() and ROOT != SCRATCH and MARKER.is_file()):
        raise RuntimeError("refusing to remove an unmarked binary-selection sandbox")
    shutil.rmtree(ROOT)

sys.exit(status)
