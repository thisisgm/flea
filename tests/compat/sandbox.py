"""One marked scratch root per harness case, removed when the harness exits, pass or fail."""
import atexit
from pathlib import Path
import shutil
import tempfile

REPO = Path(__file__).resolve().parents[2]
SCRATCH = (REPO / ".superpowers/u1").resolve()
MARKER = ".flea-test-sandbox"


def make(prefix):
    SCRATCH.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix=prefix, dir=SCRATCH)).resolve()
    (root / MARKER).touch()
    # atexit is the outermost finally: it runs after a pass, a failed assert and sys.exit alike.
    atexit.register(remove, root)
    return root


def remove(*roots):
    for root in roots:
        root = Path(root)
        # Only an absolute, non-empty, marked root directly inside the scratch dir is ever deleted.
        if str(root) and root.is_absolute() and root.parent == SCRATCH and (root / MARKER).is_file():
            shutil.rmtree(root)
