"""Copy only Omarchy presentation inputs into an owned native window's private home."""
import os
from pathlib import Path
import stat
import sys


def copy_input(source, destination):
    try:
        descriptor = os.open(source, os.O_RDONLY | os.O_NONBLOCK | os.O_CLOEXEC)
    except FileNotFoundError:
        return
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise ValueError(f"native presentation input is not a regular file: {source}")
        content = stream.read(1024 * 1024 + 1)
    if len(content) > 1024 * 1024:
        raise ValueError(f"native presentation input exceeds 1 MiB: {source}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(content)


def seed(source_home, private_home, private_config=None):
    source = Path(source_home)
    if not source.is_absolute():
        raise ValueError("native presentation source HOME must be absolute")
    for relative in ("theme/colors.toml", "theme/shell.toml", "theme/icons.theme", "theme.name"):
        path = Path(".local/state/omarchy/current") / relative
        copy_input(source / path, private_home / path)
    user = Path(".config/omarchy/shell.toml")
    copy_input(source / user, private_home / user)
    if private_config is not None:
        copy_input(source / user, private_config / "omarchy/shell.toml")
    seed_fonts(source, private_home, private_config)


def seed_fonts(source_home, private_home=None, private_config=None):
    source = Path(source_home)
    if not source.is_absolute():
        raise ValueError("native font source HOME must be absolute")
    for relative in ("fontconfig/fonts.conf", "fontconfig/conf.d/50-omarchy-input-method.conf"):
        user = Path(".config") / relative
        if private_home is not None:
            copy_input(source / user, private_home / user)
        if private_config is not None:
            copy_input(source / user, private_config / relative)


if __name__ == "__main__":
    seed_fonts(Path(sys.argv[1]), Path(sys.argv[2]))
