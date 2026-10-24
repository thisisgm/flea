"""Still-wallpaper and diagnostic guards reject their deliberately failing twins."""
import json
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

import flea_native_wallpaper as wallpaper
from flea_native_log import check, unexpected


def check_envelope(root):
    proc = root / "wallpaper-proc"
    process = proc / "101"
    process.mkdir(parents=True)
    image = root / "wallpaper.png"
    image.write_bytes(b"fixture image")
    (process / "comm").write_text("swaybg\n")
    (process / "cmdline").write_bytes(b"swaybg\0-i\0" + str(image).encode() + b"\0")
    state = dict(mime="image/png", frames="1\n", pid=101)

    def output(command, **_):
        if command[0] == "hyprctl":
            return json.dumps({"monitor": {"levels": {"0": [{"pid": state["pid"]}]}}})
        if command[0] == "file":
            return state["mime"]
        if command[:2] == ["magick", "identify"]:
            return state["frames"]
        raise AssertionError(command)

    with patch.object(wallpaper.subprocess, "check_output", output):
        observed = wallpaper.inspect(proc)
        assert observed["still"] and observed["images"][0]["path"] == str(image)
        for name, changes in (
            ("animated", {"frames": "2\n2\n"}),
            ("video", {"mime": "video/mp4"}),
            ("foreign-layer", {"pid": 102}),
        ):
            original = state.copy()
            state.update(changes)
            try:
                wallpaper.inspect(proc)
            except RuntimeError:
                pass
            else:
                raise AssertionError(f"wallpaper {name} was accepted")
            state.update(original)
        (process / "comm").write_text("mpvpaper\n")
        try:
            wallpaper.inspect(proc)
        except RuntimeError as error:
            assert "video/animated" in str(error)
        else:
            raise AssertionError("animated renderer was accepted")
    assert unexpected("INFO window mapped\nWARN deliberate\n", ["WARN deliberate"]) == []
    assert check("flea-gui: WARN deliberate\n", ["WARN deliberate"]) == []
    assert check("WARN deliberate\nWARN deliberate\n", ["WARN deliberate"])
    assert check("INFO mapped\n", ["WARN deliberate"])
    assert check("ERROR backend panicked while handling WARN deliberate", ["WARN deliberate"])
    for line in ("WARN unexpected", "ERROR VAAPITextureConverter: No rhi", "flea-gui: backend failed",
                 "FLEA_IPC_NOT_PORTED flea.tokens", "thread panicked", "flea-gui: backend disconnected"):
        assert unexpected(line, ["WARN deliberate"]) == [line], line
    expected = root / "native-expected-log"
    log = root / "native-twin.log"
    expected.write_text("WARN deliberate\n")
    for content, status in (("flea-gui: WARN deliberate\n", 0),
                            ("ERROR backend panicked while handling WARN deliberate\n", 1)):
        log.write_text(content)
        result = subprocess.run([sys.executable, "-B", str(Path(__file__).with_name("flea_native_log.py")),
                                 str(expected), str(log)], capture_output=True, timeout=3)
        assert result.returncode == status, result
    print(f"TRANSPORT_ENVELOPE still=verified animation=refused video=refused foreign-layer=refused native-log=verified twin_exit={result.returncode}")
