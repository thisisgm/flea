"""Keep legacy filter sentinels and chooser retries working through the real picker launcher."""
import json
from pathlib import Path
import shutil
import subprocess
import sys

import sandbox

repo = Path(__file__).resolve().parents[2]
failures = []
# flea prefers a flea-gui beside its own executable, so a copy with no sibling is what reaches the stub on PATH.
launcher = sandbox.make("chooser-launcher-") / "flea"
shutil.copy2(sys.argv[1], launcher)


def fixture():
    root = sandbox.make("chooser-replies-")
    for area in ("home", "config", "state", "data", "cache", "runtime", "bin", "chosen"):
        (root / area).mkdir(mode=0o700)
    (root / "chosen/image.iso").write_text("chosen image\n")
    stub = root / "bin/flea-gui"
    stub.write_text('''#!/usr/bin/python3
import json, os, sys
from pathlib import Path
root = Path(os.environ["CHOOSER_ROOT"]).resolve()
assert root.is_absolute() and (root / ".flea-test-sandbox").is_file()
assert sys.argv[1] == "--pick"
reply = Path(sys.argv[2]).resolve()
assert reply.is_absolute() and reply.is_relative_to(root)
chosen = Path(os.environ["CHOOSER_CHOSEN"]).resolve()
assert chosen.is_relative_to(root)
answer = json.dumps({"response": 0, "uris": [chosen.as_uri()]})
if os.environ["CHOOSER_FILTER"] != "absent":
    answer = answer[:-1] + ', "filter":' + os.environ["CHOOSER_FILTER"] + '}'
reply.write_text(answer)
with (root / "invocations").open("a") as log:
    log.write(os.environ["CHOOSER_FILTER"] + "\\n")
''')
    stub.chmod(0o700)
    env = {"PATH": str(root / "bin") + ":/usr/bin:/bin", "HOME": str(root / "home"),
           "XDG_CONFIG_HOME": str(root / "config"), "XDG_STATE_HOME": str(root / "state"),
           "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"),
           "XDG_RUNTIME_DIR": str(root / "runtime"), "XDG_DATA_DIRS": str(root / "missing"),
           "FLEA_UI": str(repo / "ui"), "QSG_RHI_BACKEND": "opengl",
           "VK_DRIVER_FILES": str(root / "missing.json"), "WAYLAND_DISPLAY": "fixture",
           "CHOOSER_ROOT": str(root)}
    return root, env


def invoke(root, env, consumer, index, accepted, label):
    chosen = root / ("chosen/image.iso" if consumer == "flash" else "chosen")
    result = subprocess.run([str(launcher), consumer, "choose"],
                            env=env | {"CHOOSER_FILTER": index, "CHOOSER_CHOSEN": str(chosen)},
                            cwd=root, capture_output=True, timeout=8)
    reply = root / "state/omarchy/flea-shelf/reply.json"
    expected = (str(chosen) + "\n").encode() if accepted else b""
    clean = not reply.exists()
    print(f"{label}: exit={result.returncode}, stdout={result.stdout!r}, reply_absent={clean}, stderr={result.stderr!r}")
    if result.returncode != (0 if accepted else 2) or result.stdout != expected or not clean:
        failures.append(label)
    if not accepted and not result.stderr:
        failures.append(label + "-missing-refusal")


for consumer in ("shelf", "flash"):
    for index in ("absent", "0", "-1"):
        root, env = fixture()
        label = f"{consumer}-filter-{index}"
        invoke(root, env, consumer, index, True, label)
        invoke(root, env, consumer, "absent", True, label + "-second-call")
        calls = (root / "invocations").read_text().splitlines()
        if calls != [index, "absent"]:
            failures.append(label + "-did-not-open-both-choosers")
    for index in ("-2", "0.5", '"0"', "null", "1e309"):
        root, env = fixture()
        label = f"{consumer}-rejected-filter-{index}"
        invoke(root, env, consumer, index, False, label)
        invoke(root, env, consumer, "absent", True, label + "-second-call")
        calls = (root / "invocations").read_text().splitlines()
        if calls != [index, "absent"]:
            failures.append(label + "-did-not-open-second-chooser")

assert not failures, "chooser reply failures: " + ", ".join(failures)
print("chooser-replies: all cases passed")
