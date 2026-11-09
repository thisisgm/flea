#!/usr/bin/env python3
"""Exercise the public argv boundary with isolated homes and recording children."""
import base64
import json
import os
from pathlib import Path
import subprocess
import shutil
import sys

import deltas
import sandbox

REPO = Path(__file__).resolve().parents[2]
BINARY = Path(sys.argv[1]).resolve()
GOLDEN = REPO / "tests/compat/cli-v0312.json"
CAPTURE = "--capture" in sys.argv[2:]


def cases(root):
    path = str(root / "files")
    file = path + "/a b.txt"
    rows = [
        ("version", ["--version"], {}),
        ("gui-desktop", ["--gui", path], {"WAYLAND_DISPLAY": "contract"}),
        ("bare-directory", [path], {"WAYLAND_DISPLAY": "contract"}),
        ("bare-file", [file], {"WAYLAND_DISPLAY": "contract"}),
        ("bare-relative", ["files"], {"WAYLAND_DISPLAY": "contract"}),
        ("select-path", ["--select", file], {"WAYLAND_DISPLAY": "contract"}),
        ("select-uri", ["--select", (root / "files/a b.txt").as_uri()], {"WAYLAND_DISPLAY": "contract"}),
        ("select-print", ["--print-target", "--select", file], {}),
        ("gui-no-display", ["--gui"], {}),
        ("gui-conflict", ["--gui", "--tui"], {}),
        ("tui-pipe", ["--tui", path], {}),
        ("cwd-file", ["--tui", "--cwd-file", str(root / "cwd")], {}),
        ("backend", ["--backend"], {}),
        ("open-folder", ["--open", path], {}),
        ("open-file", ["--open", file], {}),
        ("terminal", ["--terminal", path], {}),
        ("pick", ["--pick", str(root / "reply.json")], {"WAYLAND_DISPLAY": "contract", "FLEA_PICKER": '{"kind":"open","multiple":false}'}),
        ("pick-no-request", ["--pick", str(root / "reply.json")], {}),
        ("pick-no-display", ["--pick", str(root / "reply.json")], {"FLEA_PICKER": "{}"}),
        ("unlock", ["--unlock", "/dev/contract"], {"WAYLAND_DISPLAY": "contract"}),
        ("ui-state", ["--ui-state"], {}),
        ("ui-state-write", ["--ui-state", '{"view":"grid"}'], {}),
        ("favourites", ["--favourites", '{"op":"inspect","indices":[]}'], {}),
        ("default", ["--default"], {}),
        ("default-off", ["--default", "off"], {}),
        ("default-alias", ["--youleftmeforstrata"], {}),
        ("picker-claim", ["--picker"], {}),
        ("picker-off", ["--picker", "off"], {}),
        ("prewarm", ["--prewarm", path, "2", str(root / "prewarm")], {}),
        ("launch-warm", ["--launch-warm", "-", "-", "-"], {}),
        ("clip-empty", ["--clip", "set", "copy"], {}),
        ("clip-clear", ["--clip", "clear", ""], {}),
        ("clip-own", ["--clip-own"], {}),
        ("run", ["--run", file, "0", "0", "0", "0", "0"], {}),
        ("run-terminal", ["--run-terminal", file, "0", "0", "0", "0", "0"], {}),
        ("open-handler", ["--open-handler", file, "bad", "0", "0", "0", "0", "0"], {}),
        ("open-text", ["--open-text", file], {}),
        ("flash", ["flash", "choose", path], {}),
        ("update-handoff", ["--update"], {}),
        ("update-check", ["--update", "check"], {}),
        ("clip-get", ["--clip", "get"], {}),
        ("clip-clear-token", ["--clip", "clear", "contract-token"], {}),
        ("figure-store", ["--figure-store"], {}),
        ("unknown", ["--not-a-mode"], {}),
    ]
    malformed = ["version", "prewarm", "launch-warm", "open", "terminal", "unlock", "update", "default", "youleftmeforstrata", "picker", "pick", "clip", "clip-own", "ui-state", "favourites", "run", "run-terminal", "open-text", "open-handler", "select", "print-target", "thumb-worker", "figure-helper", "figure-compile", "figure-store"]
    for flag in malformed:
        args = ["--" + flag]
        if flag in ("version", "update", "default", "youleftmeforstrata", "picker", "clip-own", "ui-state", "thumb-worker", "figure-helper", "figure-compile", "figure-store"):
            args += ["extra", "extra"]
        rows.append(("usage-" + flag, args, {}))
    for verb in ["drag-begin", "size", "forget", "add", "pin", "unpin", "order", "captures", "thumb", "open", "move", "copy", "cancel", "zip", "paths", "places", "choose", "send", "peers", "clear", "restore", "undo", "piles", "toggle", "bind", "bad"]:
        if verb not in ("places", "choose", "send", "peers", "bind", "captures", "cancel", "undo"):
            rows.append(("shelf-" + verb, ["shelf", verb], {}))
    return rows


def encode(data, root):
    data = data.replace(str(root).encode(), b"@ROOT@").replace(str(BINARY).encode(), b"@BIN@").replace(str(REPO).encode(), b"@REPO@")
    return base64.b64encode(data).decode()


def main():
    global BINARY
    assert BINARY.is_file(), f"missing binary: {BINARY}"
    root = sandbox.make("cli-")
    for name in ("home", "config", "state", "data", "cache", "runtime", "bin", "files"):
        (root / name).mkdir(mode=0o700)
    shutil.copy2(BINARY, root / "bin/flea")
    BINARY = root / "bin/flea"
    (root / "files/a b.txt").write_text("contract\n")
    recorder = '''#!/usr/bin/python3
import json, os
print(json.dumps(dict({k: os.environ.get(k) for k in ("FLEA_PATH", "FLEA_SELECT", "FLEA_PICKER", "FLEA_PICKER_REPLY", "FLEA_UNLOCK")}, frontend="qml"), sort_keys=True, separators=(",", ":")))
'''
    # flea-gui takes its start and selection only as arguments, so its record maps them onto the same fields.
    gui_recorder = recorder.replace('frontend="qml"', 'frontend="gpui"').replace("import json, os\n", """import json, os, sys
args = sys.argv[1:]
assert os.environ.get("FLEA_PATH") is None and os.environ.get("FLEA_SELECT") is None, "start or selection leaked through the environment"
os.environ.update({k: args[args.index(f) + 1] for k, f in (("FLEA_PATH", "--"), ("FLEA_SELECT", "--select"), ("FLEA_PICKER_REPLY", "--pick")) if f in args})
""")
    for name, script in {"qs": recorder, "flea-gui": gui_recorder, "gio": "#!/bin/sh\nexit 0\n", "xdg-terminal-exec": "#!/bin/sh\nexit 0\n", "xdg-mime": "#!/bin/sh\nexit 0\n", "hyprctl": "#!/bin/sh\nexit 0\n", "zoxide": "#!/bin/sh\nexit 0\n", "pacman": "#!/bin/sh\nexit 1\n", "omarchy-launch-floating-terminal-with-presentation": "#!/bin/sh\nexit 0\n"}.items():
        p = root / "bin" / name
        p.write_text(script)
        p.chmod(0o700)
    env = {"PATH": str(root / "bin") + ":/usr/bin:/bin", "HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"), "XDG_STATE_HOME": str(root / "state"), "XDG_DATA_HOME": str(root / "data"), "XDG_CACHE_HOME": str(root / "cache"), "XDG_RUNTIME_DIR": str(root / "runtime"), "XDG_DATA_DIRS": str(root / "missing"), "XDG_CONFIG_DIRS": str(root / "missing"), "FLEA_UI": str(REPO / "ui"), "FLEA_BIN": str(BINARY), "QSG_RHI_BACKEND": "opengl", "VK_LOADER_DRIVERS_DISABLE": "contract", "LC_ALL": "C", "TERM": "xterm"}
    if "--gpui" in sys.argv:
        env["FLEA_FRONTEND"] = "gpui"
    result = {}
    expected = {} if CAPTURE else json.loads(GOLDEN.read_text())
    for name, changes in ({} if CAPTURE else deltas.load("cli")).items():
        expected[name] = dict(expected[name], stdout=deltas.apply(name, expected[name]["stdout"], changes))
    usage_changes = [] if CAPTURE else deltas.load("cli-usage").get("actions", [])
    for name, record in expected.items():
        if b"usage: flea " in base64.b64decode(record["stderr"]):
            record["stderr"] = deltas.apply(name, record["stderr"], usage_changes)
    chosen = sys.argv[sys.argv.index("--case") + 1] if "--case" in sys.argv else None
    mismatched = []
    for name, args, extra in cases(root):
        if chosen and name != chosen:
            continue
        run = subprocess.run([str(BINARY), *args], cwd=root, env=env | extra, input=b'{"c":"quit"}\n' if name == "backend" else b"", capture_output=True, timeout=8)
        record = {"code": run.returncode, "stdout": encode(run.stdout, root), "stderr": encode(run.stderr, root)}
        if name == "version":
            version = next(line.split('"')[1] for line in (REPO / "Cargo.toml").read_text().splitlines() if line.startswith("version ="))
            assert run.stdout == (version + "\n").encode() and run.returncode == 0
            record["stdout"] = encode(b"@VERSION@\n", root)
        result[name] = record
        if not CAPTURE and record != expected.get(name):
            mismatched.append(f"{name}: expected {expected.get(name)}, got {record}")
    # --help and -h postdate the 0.3.12 golden, so the built binary is checked against its own usage error: the same lines, on stdout, exit 0.
    if not CAPTURE and chosen is None:
        # Sample stderr: b"flea: unknown flag --not-a-mode\nusage: flea [--tui|--gui] ...\n", exit 2.
        unknown = subprocess.run([str(BINARY), "--not-a-mode"], cwd=root, env=env, capture_output=True, timeout=8)
        lines = unknown.stderr.partition(b"\n")[2]
        if unknown.returncode != 2 or not lines.startswith(b"usage: flea "):
            mismatched.append(f"help: --not-a-mode gave code {unknown.returncode} and stderr {unknown.stderr!r}, no usage to compare")
        for flag in (["--help"], ["-h"], ["--tui", "--help"]):
            run = subprocess.run([str(BINARY), *flag], cwd=root, env=env, capture_output=True, timeout=8)
            if (run.returncode, run.stdout, run.stderr) != (0, lines, b""):
                mismatched.append(f"help {' '.join(flag)}: code {run.returncode}, stdout {run.stdout!r}, stderr {run.stderr!r}")
        print("cli-contract: help rows checked")
    # Every differing row is named, so one run lists the whole break.
    assert not mismatched, "\n".join(mismatched)
    # A run that checked no row must fail, never print a vacuous pass.
    assert result, "no rows ran: refusing a zero-check run"
    if CAPTURE:
        print("CONTRACT_GOLDEN=" + json.dumps(result, sort_keys=True, separators=(",", ":")))
    else:
        assert set(result) == ({chosen} if chosen else set(expected)), "golden row inventory differs"
    print(f"cli-contract: {len(result)} rows passed")
    print("cli-contract: all rows passed")


if __name__ == "__main__":
    main()
