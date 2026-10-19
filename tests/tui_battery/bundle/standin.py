"""Stand-in Media seeds so the offline suite can build a bundle without image, video or audio encoders."""
from pathlib import Path

from . import pdf, raster


def seed(root):
    """Writes ROOT/Media with the six seed names; only the native run's real bench seeds count as media proof."""
    media = Path(root) / "Media"
    media.mkdir(parents=True)
    (media / "photo.png").write_bytes(raster.gradient_png(8, 6, [(0, 0, 0), (255, 255, 255)]))
    (media / "report.pdf").write_bytes(pdf.document([[(20, 60, 700, "Visual gate report")]], "", "", 21840, size=(600, 800)))
    for name in ("clip.mp4", "sunset.jpg", "tone.ogg", "bundle.zip"):
        (media / name).write_bytes(b"stand-in " + name.encode() + b"\n")
    return Path(root)


def writable(bundle):
    """Gives a read-only bundle's directories back to its owner so the marked root's own cleanup can remove it."""
    bundle = Path(bundle)
    if bundle.is_absolute() and not bundle.is_symlink() and (bundle / ".flea-test-sandbox").is_file():
        for path in [bundle, *bundle.rglob("*")]:
            if path.is_dir() and not path.is_symlink():
                path.chmod(0o755)
