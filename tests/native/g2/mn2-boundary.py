#!/usr/bin/env python3
"""Check both native adapter arms against the real fixture guard without a display."""
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile

repo = Path(__file__).resolve().parents[3]
parent = Path(sys.argv[1]).resolve()
if not (parent.is_relative_to(repo / ".superpowers/g2-mn2")
        and (parent / ".flea-test-sandbox").is_file()):
    sys.exit(f"mn2-boundary: refusing unmarked root {parent}")
root = Path(tempfile.mkdtemp(prefix="native-boundary.", dir=parent))
(root / ".flea-test-sandbox").touch()
names = ("fixture_root", "thumb_fixture", "hash_fixture", "stale_fixture")
assignments = []
source = (repo / "tests/ui.sh").read_text()
for name in names:
    lines = re.findall(rf"^{name}=.*$", source, re.MULTILINE)
    assert len(lines) == 1, f"tests/ui.sh must define {name} once"
    assignments.extend(lines)
probe = "\n".join([
    "#!/bin/bash",
    "set -euo pipefail",
    ". " + shlex.quote(str(repo / "tools/flea-sandbox-guard")),
    *assignments,
    'expected=$(realpath -m -- "$HOME/../fixtures")',
    'test -f "$HOME/../../.flea-test-sandbox"',
    "status=0",
    'for path in "$fixture_root" "$thumb_fixture" "$hash_fixture" "$stale_fixture"; do',
    '    sandbox_require "$path"',
    '    if [[ $SANDBOX_ROOT == "$expected" && $SANDBOX_PATH == "$expected/"* ]]; then',
    '        printf "containment: INSIDE %s\\n" "$SANDBOX_PATH"',
    "    else",
    '        printf "containment: OUTSIDE %s\\n" "$SANDBOX_PATH"',
    "        status=1",
    "    fi",
    "done",
    'exit "$status"',
    "",
])
failures = []
for inherited in (False, True):
    label = "inherited" if inherited else "unset"
    stage = root / label
    (stage / "tests/native/g2").mkdir(parents=True)
    (stage / ".flea-test-sandbox").touch()
    shutil.copyfile(repo / "tests/native/g2/mn2.sh", stage / "tests/native/g2/mn2.sh")
    (stage / "tests/ui.sh").write_text(probe)
    env = {
        "PATH": "/usr/bin:/bin",
        "HOME": str(stage / "operator-home"),
        "W": str(stage),
        "Q_BIN": "/usr/bin/true",
        "G_BIN": "/usr/bin/true",
        "XDG_RUNTIME_DIR": str(stage / "runtime"),
    }
    override = root / "inherited-fixtures"
    if inherited:
        env["FLEA_FIXTURE_ROOT"] = str(override)
    result = subprocess.run(
        ["/bin/bash", "tests/native/g2/mn2.sh"], cwd=stage, env=env,
        stdin=subprocess.DEVNULL, capture_output=True, text=True,
        start_new_session=True, timeout=30,
    )
    (stage / "adapter.log").write_text(result.stdout + result.stderr)
    assert result.returncode == 2, result
    assert "BLOCKED: the GPUI scene CLI tools/flea-ci-visual is absent" in result.stderr
    adapters = list((stage / ".superpowers/g2-mn2").glob("native.*"))
    assert len(adapters) == 1
    adapter = adapters[0]
    assert (adapter / ".flea-test-sandbox").is_file()
    for arm in ("qml", "gpui"):
        output = (adapter / arm / "ui.log").read_text()
        passed = output.count("containment: INSIDE ") == len(names)
        print(f"{label}/{arm}: {'PASS' if passed else 'FAIL'}", flush=True)
        print(output, end="", flush=True)
        if not passed:
            failures.append(f"{label}/{arm}")
    assert not override.exists(), "an inherited fixture path was written"
    assert not list(stage.rglob("flea-ui-*-*")), "read-only guard probe created fixtures"
print(f"evidence: {root}", flush=True)
if failures:
    sys.exit("G2MN2 BOUNDARY FAIL: " + ", ".join(failures))
print("G2MN2 BOUNDARY PASS 4 arm/environment checks, 16 guarded paths")
