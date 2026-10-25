#!/usr/bin/env python3
"""Require the fixture boundary to refuse unsafe parents with Python optimization on."""
from pathlib import Path
import subprocess
import sys
import tempfile

repo = Path(__file__).resolve().parents[3]
parent = Path(sys.argv[1]).resolve()
if not (parent.is_relative_to(repo / ".superpowers/g2-mn2")
        and (parent / ".flea-test-sandbox").is_file()):
    sys.exit(f"mn2-boundary-controls: refusing unmarked root {parent}")
root = Path(tempfile.mkdtemp(prefix="boundary-controls.", dir=parent))
(root / ".flea-test-sandbox").touch()
for label, target in (("unmarked", root / "unmarked"), ("outside", root / "outside")):
    target.mkdir()
    if label == "outside":
        (target / ".flea-test-sandbox").touch()
    stage = root / label / "stage"
    script = repo / "tests/native/g2/mn2-boundary.py"
    if label == "outside":
        stage.mkdir()
        script = stage / "tests/native/g2/mn2-boundary.py"
        script.parent.mkdir(parents=True)
        script.write_text((repo / "tests/native/g2/mn2-boundary.py").read_text())
    before = set(target.iterdir())
    result = subprocess.run(
        [sys.executable, "-B", "-O", str(script), str(target)],
        stdin=subprocess.DEVNULL, capture_output=True, text=True,
        start_new_session=True, timeout=30,
    )
    if (result.returncode == 0 or "mn2-boundary: refusing unmarked root" not in result.stderr
            or set(target.iterdir()) != before):
        sys.exit(f"G2MN2 ROOT CONTROL FAIL {label}: rc={result.returncode}, {result.stderr}")
    print(f"G2MN2 ROOT CONTROL PASS {label}")
