"""A window that exits before IPC keeps its own words in a log the caller shares with the transport."""
import os
from pathlib import Path
import subprocess
import sys

MESSAGE = "flea: no graphics driver could open a window; install or repair vulkan-swrast and mesa, or run flea --tui"


def check_shared_log(root):
    repo = Path(__file__).resolve().parents[2]
    target = root / "log-target"
    for name in ("flea", "flea-gui"):
        binary = target / "debug" / name
        binary.parent.mkdir(parents=True, exist_ok=True)
        binary.write_text(f"#!/bin/sh\nprintf '%s\\n' '{MESSAGE}' >&2\nexit 78\n")
        binary.chmod(0o700)
    log = root / "shared-window.log"
    environment = dict(os.environ, CARGO_TARGET_DIR=str(target), FLEA_TEST_WINDOWS_ROOT=str(root), FLEA_PATH=str(root),
                       FLEA_TEST_FRONTEND="gpui", FLEA_BIN=str(target / "debug/flea"))
    environment.pop("FLEA_GUI_BIN", None)
    helper = repo / "tests/lib/flea_transport_cli.py"
    # The same redirection tests/ui-uish/launch.sh's case_renderer gives flea_test_exec.
    script = 'exec "$0" -B "$1" exec -- "$2" --gui "$3" > "$4" 2>&1'
    result = subprocess.run(["sh", "-c", script, sys.executable, str(helper), str(target / "debug/flea"), str(root), str(log)],
                            env=environment, timeout=60)
    text = log.read_text()
    assert result.returncode != 0, text
    assert MESSAGE + "\n" in text, f"the window's line was overwritten: {text!r}"
    assert "flea-test-transport: window exited before IPC" in text, text
    print(f"TRANSPORT_LOG shared-log=verified exit={result.returncode}")


if __name__ == "__main__":
    import tempfile
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from flea_transport import MARKER, REPO
    scratch = REPO / ".superpowers/g010t"
    scratch.mkdir(parents=True, exist_ok=True)
    root = Path(tempfile.mkdtemp(prefix="log-controls.", dir=scratch))
    (root / MARKER).touch()
    check_shared_log(root)
