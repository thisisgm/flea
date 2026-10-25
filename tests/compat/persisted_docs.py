"""Exercise each persistent JSON document through a real launch and its first writer."""
import json
from pathlib import Path
import subprocess
import tempfile


def check_documents(parent, binary, inherited, _file, _run):
    repo = Path(__file__).resolve().parents[2]
    plugin_version = json.loads((repo / "shelf/manifest.json").read_text())["version"]
    documents = {
        "ui": ("state/flea/ui.json", {"stateVersion": 999, "view": "grid"}),
        "shelf": ("state/omarchy/flea-shelf/shelf.json", {"schemaVersion": 999, "items": []}),
        "piles": ("state/omarchy/flea-shelf/piles.json", {"schemaVersion": 999, "piles": []}),
        "summon": ("state/omarchy/flea-shelf/summon.json", {"schemaVersion": 999, "summon": 0}),
        "drags": ("state/omarchy/flea-shelf/drags.json", {"schemaVersion": 999, "drags": []}),
        "dests": ("state/omarchy/flea-shelf/dests.json", {"schemaVersion": 999, "dests": []}),
        "undo": ("state/omarchy/flea-shelf/undo.json", {"schemaVersion": 999, "at": 1, "moves": []}),
        "reply": ("state/omarchy/flea-shelf/reply.json", {"schemaVersion": 999, "response": 0, "uris": []}),
        "plugin": ("config/omarchy/plugins/io.github.thisisgm.flea-shelf/manifest.json",
                   {"schemaVersion": 999, "id": "io.github.thisisgm.flea-shelf", "version": plugin_version}),
    }
    for name, (relative, newer) in documents.items():
        for kind, original in (("foreign", b'{"foreign":{"opaque":[1,2,3]}}\n'),
                               ("newer", json.dumps(newer).encode()),
                               ("unparsable", b'{"truncated":'), ("nonobject", b'[1,2,3]\n')):
            root = Path(tempfile.mkdtemp(prefix=f"preserve-{name}-{kind}-", dir=parent)).resolve()
            (root / ".flea-test-sandbox").touch()
            for area in ("home", "config", "state", "data", "cache", "runtime", "files", "dest", "bin"):
                (root / area).mkdir(mode=0o700)
            env = inherited | {"HOME": str(root / "home"), "XDG_CONFIG_HOME": str(root / "config"),
                "XDG_STATE_HOME": str(root / "state"), "XDG_DATA_HOME": str(root / "data"),
                "XDG_CACHE_HOME": str(root / "cache"), "XDG_RUNTIME_DIR": str(root / "runtime"),
                "PATH": str(root / "bin") + ":/usr/bin:/bin", "FLEA_FRONTEND": "gpui",
                "WAYLAND_DISPLAY": "fixture", "QSG_RHI_BACKEND": "opengl",
                "VK_DRIVER_FILES": str(root / "missing.json"), "FLEA_SHELF": str(repo / "shelf")}
            for command in ("omarchy", "omarchy-shell", "hyprctl", "zoxide"):
                stub = root / "bin" / command
                stub.write_text("#!/bin/sh\nexit 0\n")
                stub.chmod(0o700)
            source = root / "files/a.txt"
            source.write_text("kept file\n")
            ui = root / "state/flea/ui.json"
            ui.parent.mkdir(parents=True, exist_ok=True)
            ui.write_text('{"shelf":{"enabled":true}}' if name == "plugin" else '{"view":"list"}')
            shelf = root / "state/omarchy/flea-shelf/shelf.json"
            shelf.parent.mkdir(parents=True, exist_ok=True)
            shelf.write_text(json.dumps({"items": [{"path": str(source), "folder": False}]}))
            target = root / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(original)
            def invoke(args):
                return subprocess.run([str(binary), *args], cwd=root, env=env, capture_output=True, timeout=8)
            launched = invoke(["--gui"])
            assert launched.returncode == 0, (name, kind, launched.stderr)
            assert target.read_bytes() == original, (name, kind, "launch replaced unknown bytes")
            actions = {
                "ui": ["--ui-state", '{"hidden":true}'],
                "shelf": ["shelf", "add", str(source)],
                "piles": ["shelf", "clear"], "summon": ["shelf", "toggle"],
                "drags": ["shelf", "drag-begin", "copy", str(source)],
                "dests": ["shelf", "copy", str(root / "dest"), str(source)],
                "undo": ["shelf", "move", str(root / "dest"), str(source)],
                "reply": ["shelf", "choose"],
                "plugin": ["--ui-state", '{"shelf":{"enabled":true}}'],
            }
            if name == "plugin":
                disabled = invoke(["--ui-state", '{"shelf":{"enabled":false}}'])
                assert disabled.returncode == 0, (name, kind, "disabling the shelf failed", disabled.stderr)
            attempted = invoke(actions[name])
            if name == "ui" and kind in ("unparsable", "nonobject"):
                # A ui.json that is not an object is kept as ui.json.broken and replaced, as 0.3.12 did.
                assert attempted.returncode == 0, (name, kind, attempted.stderr)
                assert (target.parent / "ui.json.broken").read_bytes() == original, (name, kind, "backup lost the bytes")
                assert json.loads(target.read_text())["hidden"] is True, (name, kind, "patch did not land")
                continue
            assert target.read_bytes() == original, (name, kind, "first writer replaced unknown bytes", attempted.stderr)
            assert b"unchanged" in attempted.stderr, (name, kind, "writer did not explain refusal", attempted.stderr)
        print(f"state-compat: {name} foreign/newer/unparsable/nonobject survive launch and first writer")
