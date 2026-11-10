"""Drain only clipboard owners carrying this live-operations run's private state."""
import os
from pathlib import Path
import signal
import sys
import time


def owned(process, root, binary):
    try:
        if process.stat().st_uid != os.getuid():
            return False
        arguments = (process / "cmdline").read_bytes().rstrip(b"\0").split(b"\0")
        if arguments != [os.fsencode(binary), b"--clip-own"]:
            return False
        executable = os.readlink(process / "exe").removesuffix(" (deleted)")
        if executable != str(binary):
            return False
        environment = dict(item.split(b"=", 1) for item in
                           (process / "environ").read_bytes().split(b"\0") if b"=" in item)
        return all(environment.get(name) == os.fsencode(root / suffix) for name, suffix in (
            (b"FLEA_UNDO_DIR", "undo"), (b"XDG_DATA_HOME", "data"),
            (b"FLEA_TEST_WINDOWS_ROOT", "windows")))
    except (FileNotFoundError, ProcessLookupError, PermissionError):
        return False


def drain(root, binary):
    descriptors = []
    try:
        for process in Path("/proc").iterdir():
            if process.name.isdigit() and owned(process, root, binary):
                try:
                    descriptor = os.pidfd_open(int(process.name))
                    if owned(process, root, binary):
                        signal.pidfd_send_signal(descriptor, signal.SIGTERM)
                        descriptors.append((process, descriptor))
                    else:
                        os.close(descriptor)
                except ProcessLookupError:
                    pass
        deadline = time.monotonic() + 5
        while any(owned(process, root, binary) for process, _ in descriptors):
            if time.monotonic() >= deadline:
                raise RuntimeError("live-operations clipboard owner did not drain")
            time.sleep(0.05)
        print(f"LIVE_OPS_CLEANUP clipboard_owners={len(descriptors)} remaining=0")
    finally:
        for _, descriptor in descriptors:
            os.close(descriptor)


if __name__ == "__main__":
    root, binary = map(Path, sys.argv[1:])
    if not root.is_absolute() or not binary.is_absolute() or not (root / ".flea-test-sandbox").is_file():
        raise SystemExit("live-operations cleanup requires an absolute marked run and candidate")
    drain(root, binary)
