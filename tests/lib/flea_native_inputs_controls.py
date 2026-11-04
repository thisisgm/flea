"""Presentation copies preserve bytes and absence without importing user state."""
from pathlib import Path
import os

from flea_native_inputs import seed


def check_inputs(root):
    source = root / "presentation-source"
    theme = source / ".local/state/omarchy/current/theme"
    theme.mkdir(parents=True)
    (source / ".config/omarchy").mkdir(parents=True)
    for name in ("colors.toml", "shell.toml", "icons.theme"):
        (theme / name).write_bytes((name + "\n").encode())
    (source / ".config/omarchy/shell.toml").write_text("base-size = 16\n")
    (source / ".config/fontconfig/conf.d").mkdir(parents=True)
    (source / ".config/fontconfig/fonts.conf").write_text("session monospace alias\n")
    (source / ".config/fontconfig/conf.d/50-omarchy-input-method.conf").write_text("session CJK fallback\n")
    (source / ".local/state/omarchy/current/theme.name").write_text("fixture\n")
    (source / ".local/state/flea").mkdir()
    (source / ".local/state/flea/ui.json").write_text("operator state")
    (source / ".cache").mkdir()
    (source / ".cache/secret").write_text("operator cache")
    destination = root / "presentation-home"
    config = root / "presentation-config"
    seed(source, destination, config)
    for path in source.rglob("*"):
        if path.is_file() and "omarchy" in path.parts:
            copied = destination / path.relative_to(source)
            assert copied.read_bytes() == path.read_bytes()
            assert not copied.is_symlink() and not copied.samefile(path)
    assert (config / "omarchy/shell.toml").read_text() == "base-size = 16\n"
    for relative in ("fontconfig/fonts.conf", "fontconfig/conf.d/50-omarchy-input-method.conf"):
        assert (config / relative).read_bytes() == (source / ".config" / relative).read_bytes()
        assert (destination / ".config" / relative).read_bytes() == (config / relative).read_bytes()
    assert not (destination / ".local/state/flea").exists()
    assert not (destination / ".cache").exists()
    seed(root / "missing-source", root / "absent-home")
    assert not (root / "absent-home").exists(), "absent inputs must stay absent"
    # A FIFO must refuse immediately instead of holding launch on a presentation read.
    fifo = source / ".local/state/omarchy/current/theme.name"
    assert fifo.is_absolute() and fifo.is_relative_to(root) and (root / ".flea-test-sandbox").is_file()
    fifo.unlink()
    os.mkfifo(fifo)
    try:
        seed(source, root / "fifo-home")
    except ValueError as error:
        assert "not a regular file" in str(error)
    else:
        raise AssertionError("FIFO presentation input was accepted")
    print("TRANSPORT_INPUTS bytes=independent absence=kept state=private fifo=refused")


def check_launch_inputs(root, window_type):
    source = root / "launch-presentation"
    theme = source / ".local/state/omarchy/current/theme"
    theme.mkdir(parents=True)
    (theme / "colors.toml").write_text("foreground = '#123456'\n")
    (source / ".config/fontconfig").mkdir(parents=True)
    (source / ".config/fontconfig/fonts.conf").write_text("owned font alias\n")
    old_home = os.environ.get("HOME")
    old_config = os.environ.get("XDG_CONFIG_HOME")
    descriptor = os.open(source, os.O_DIRECTORY | os.O_RDONLY)
    window = None
    try:
        # The proc alias hides ancestor markers, like an unmarked operator HOME, without leaving this fixture.
        os.environ["HOME"] = f"/proc/{os.getpid()}/fd/{descriptor}"
        os.environ["XDG_CONFIG_HOME"] = "/flea-u01-unmarked-config"
        window = window_type.start(root, parent=root, frontend="gpui", timeout=1)
        copied = window.root / "home/.local/state/omarchy/current/theme/colors.toml"
        assert copied.read_bytes() == (theme / "colors.toml").read_bytes(), "native launch lost the session palette"
        environment = Path(f"/proc/{window.pid}/environ").read_bytes().split(b"\0")
        assert f"HOME={window.root}/home".encode() in environment, "native launch reused operator HOME"
        assert not (window.root / "home/.local/state/flea").exists()
        assert (window.root / "config/fontconfig/fonts.conf").read_text() == "owned font alias\n"
        window.stop()
        window = None
        window = window_type.start(root, parent=root, frontend="qml", timeout=1)
        copied = window.root / "home/.local/state/omarchy/current/theme/colors.toml"
        assert copied.read_bytes() == (theme / "colors.toml").read_bytes(), "QML launch lost the session palette"
        environment = Path(f"/proc/{window.pid}/environ").read_bytes().split(b"\0")
        assert f"HOME={window.root}/home".encode() in environment, "QML launch reused operator HOME"
        window.stop()
        window = None
        print("TRANSPORT_INPUTS launch=qml theme=seeded home=private")
        os.environ["HOME"] = str(source)
        window = window_type.start(root, parent=root, frontend="gpui", timeout=1)
        assert (window.root / "config/fontconfig/fonts.conf").read_text() == "owned font alias\n"
        window.stop()
        window = None
        window = window_type.start(root, parent=root, frontend="qml", timeout=1)
        assert (window.root / "config/fontconfig/fonts.conf").read_text() == "owned font alias\n", "QML launch lost the session fonts"
        print("TRANSPORT_INPUTS launch=seeded home=private qt=unneeded")
    finally:
        if window is not None:
            window.stop()
        os.close(descriptor)
        if old_home is None:
            os.environ.pop("HOME", None)
        else:
            os.environ["HOME"] = old_home
        if old_config is None:
            os.environ.pop("XDG_CONFIG_HOME", None)
        else:
            os.environ["XDG_CONFIG_HOME"] = old_config
