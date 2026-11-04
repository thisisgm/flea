#!/usr/bin/env python3
"""Exercise only the suite setup, including its environment and early cleanup failures."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

suite = Path(__file__).with_name("trashthumb-backend.sh").resolve()
setup = suite.read_text().split("# Suite setup ends here.", 1)[0].split("for module in ", 1)[0]
fixture = Path(sys.argv[1]).resolve()
assert fixture.is_absolute() and fixture.is_dir()
assert os.geteuid() != 0, "the marker-write control requires the suite's ordinary user"
real_mktemp = shutil.which("mktemp")
real_realpath = shutil.which("realpath")
assert real_mktemp and real_realpath

mktemp_wrapper = r'''#!/usr/bin/python3
import os, pathlib, subprocess, sys
root = pathlib.Path(os.environ['FLEA_SETUP_ROOT'])
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
box = pathlib.Path(subprocess.check_output([os.environ['FLEA_REAL_MKTEMP'], *sys.argv[1:]], text=True).strip())
assert box.is_absolute() and box.parent == root
(root / 'created-box').write_text(str(box))
if os.environ['FLEA_SETUP_MODE'] == 'nonempty':
    (box / 'keep').write_text('preserve this unmarked file')
if os.environ['FLEA_SETUP_MODE'] in ['readonly', 'nonempty']:
    box.chmod(0o500)
print(box)
'''
realpath_wrapper = r'''#!/usr/bin/python3
import os, pathlib, sys
root = pathlib.Path(os.environ['FLEA_SETUP_ROOT'])
assert root.is_absolute() and (root / '.flea-test-sandbox').is_file()
record = root / 'created-box'
once = root / 'guard-refused'
if os.environ['FLEA_SETUP_MODE'] == 'guard' and record.exists():
    box = pathlib.Path(record.read_text())
    if str(box) in sys.argv[1:] and (box / '.flea-test-sandbox').is_file() and not once.exists():
        once.write_text('refused the first marked-box check')
        raise SystemExit(1)
os.execv(os.environ['FLEA_REAL_REALPATH'], [os.environ['FLEA_REAL_REALPATH'], *sys.argv[1:]])
'''
probe = "\npython3 -c 'import json,os; print(json.dumps({key:os.environ.get(key) for key in [\"CARGO_HOME\",\"CARGO_TARGET_DIR\",\"TMPDIR\"]}))'\n"
errors = []
for label, mode in [("F2 inherited CARGO_HOME", "normal"), ("F6 failed marker write", "readonly"),
                    ("F6 refused marked box", "guard"), ("F6 nonempty unmarked box preserved", "nonempty")]:
    root = Path(tempfile.mkdtemp(prefix="trashthumb-setup-", dir=fixture))
    root.joinpath(".flea-test-sandbox").write_text("suite setup control\n")
    try:
        tools = root / "tools"
        tools.mkdir()
        for name, source in [("mktemp", mktemp_wrapper), ("realpath", realpath_wrapper)]:
            path = tools / name
            path.write_text(source)
            path.chmod(0o755)
        cargo_home = root / "registered-cargo-home"
        cargo_home.mkdir()
        env = dict(os.environ, FLEA_FIXTURE_ROOT=str(root), FLEA_SETUP_ROOT=str(root),
                   FLEA_SETUP_MODE=mode, FLEA_REAL_MKTEMP=real_mktemp, FLEA_REAL_REALPATH=real_realpath,
                   CARGO_HOME=str(cargo_home), PATH=str(tools) + os.pathsep + os.environ["PATH"])
        result = subprocess.run(["/bin/bash", "-c", setup + probe, str(suite)],
                                env=env, text=True, capture_output=True)
        box = Path(root.joinpath("created-box").read_text())
        assert box.is_absolute() and box.parent == root
        if mode == "normal":
            assert result.returncode == 0, result.stdout + result.stderr
            values = json.loads(result.stdout.strip().splitlines()[-1])
            assert values["CARGO_HOME"] == str(cargo_home), values
            assert values["CARGO_TARGET_DIR"] == str(box / "target"), values
            assert values["TMPDIR"] == str(box / "tmp"), values
            assert not box.exists(), "a marked completed box remains"
        elif mode == "nonempty":
            assert result.returncode != 0
            assert (box / "keep").read_text() == "preserve this unmarked file"
            assert not (box / ".flea-test-sandbox").exists()
        else:
            assert result.returncode != 0, "the injected setup failure must refuse the suite"
            assert not box.exists(), "an early setup failure left its empty or marked box"
            if mode == "guard":
                assert root.joinpath("guard-refused").is_file(), "the real guard was not exercised"
        print("ok " + label)
    except (AssertionError, OSError, ValueError) as error:
        errors.append(label)
        print("FAIL " + label + ": " + str(error))
    finally:
        record = root / "created-box"
        if record.exists():
            box = Path(record.read_text())
            assert box.is_absolute() and box.parent == root and root.joinpath(".flea-test-sandbox").is_file()
            if box.is_dir():
                box.chmod(0o700)
        assert str(root) and root.is_absolute() and root.parent == fixture and root.joinpath(".flea-test-sandbox").is_file()
        shutil.rmtree(root)
if errors:
    raise SystemExit(1)
print("trashthumb-backend setup: all four environment and cleanup controls passed")
