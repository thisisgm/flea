"""Compare native public material facts with the preserved frozen QML oracle."""
import json
import os
from pathlib import Path
import subprocess
import sys

from flea_transport import Window, built_binary, private

ROOT = private(sys.argv[1])
COLORS = 'background = "#1e2030"\ndark_background = "#2c3045"\nforeground = "#d0d4e0"\naccent = "#7aa2f7"\n'
# Keep the frozen oracle's expected alphas; a native mismatch stays red for controller disposition.
CASES = (("opaque", "opaque", None, 255), ("transparent", "transparent", None, 255),
         ("user-override", "glassy", "user", 87), ("theme-override", "glassy", "theme", 87),
         ("glassy", "glassy", None, 192))


def run_case(name, mode, override, expected):
    case = ROOT / name
    case.mkdir(mode=0o700)
    (case / ".flea-test-sandbox").touch()
    environment = dict(os.environ)
    for key, relative in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"), ("XDG_STATE_HOME", "state"),
                          ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")):
        path = case / relative
        path.mkdir()
        environment[key] = str(path)
    theme = case / "home/.local/state/omarchy/current/theme"
    theme.mkdir(parents=True)
    (theme / "colors.toml").write_text(COLORS)
    if mode != "transparent":
        toggle = case / f"state/omarchy/toggles/hypr/window-{mode}.lua"
        toggle.parent.mkdir(parents=True)
        toggle.write_text("return\n")
    if override:
        config = theme / "shell.toml" if override == "theme" else case / "home/.config/omarchy/shell.toml"
        config.parent.mkdir(parents=True, exist_ok=True)
        config.write_text("[flea]\nbackground-alpha = 0.34\n")
    files = case / "files"
    files.mkdir()
    (files / "a.txt").touch()
    subprocess.run([built_binary("flea"), "--ui-state", '{"updates":{"autoCheck":false}}'],
                   env=environment, check=True, capture_output=True, timeout=10)
    saved = dict(os.environ)
    window = None
    try:
        os.environ.update(environment)
        window = Window.start(files, parent=case, frontend="gpui", timeout=15)
        result = window.call("call", "flea", "glass")
        if (result.returncode, result.stderr) != (0, b""):
            raise RuntimeError(f"native material reader failed: {result}")
        # Sample response: {"clear":true,"mode":"glassy","focused":false,"automatic":true,"alpha":1,"listAlpha":1,"foreground":"#d0d4e0","muted":"#d0d4e0","metadata":"#d0d4e0","window":"window-1","early":false,"material":false,"requestedAlpha":1,"requestedListAlpha":1}
        value = json.loads(result.stdout)
        (case / "glass.json").write_bytes(result.stdout)
        actual = round(value["alpha"] * 255)
        requested = round(value["requestedAlpha"] * 255)
        good = (value["mode"] == mode and value["automatic"] == (override is None) and actual == expected)
        print(f"MATERIAL_PRODUCT {name} alpha={actual} requested={requested} expected={expected} pass={good} reply={result.stdout.decode().strip()}", flush=True)
        window.quit()
        return good
    finally:
        if window is not None:
            window.stop()
        os.environ.clear()
        os.environ.update(saved)


failed = sum(not run_case(*case) for case in CASES)
print(f"MATERIAL_PRODUCT scope=product comparison=frozen-qml-oracle entry=flea--gui floor-call-count=unavailable-public-capability failed={failed}", flush=True)
sys.exit(bool(failed))
