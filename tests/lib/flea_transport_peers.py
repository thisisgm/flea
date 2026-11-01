"""Authenticate app-created GPUI windows within an existing owned launch group."""
from pathlib import Path

from flea_transport import TransportError, Window, endpoint, load, owned


class PeerWindow(Window):
    def __init__(self, root, peer):
        super().__init__(root)
        self.peer = peer

    def record(self):
        parent = load(self.root)
        current = owned(self.peer["pid"], parent)
        if current["state"] == "Z" or current["start"] != self.peer["start"]:
            raise TransportError("app-created window exited or PID was reused")
        return dict(parent, **self.peer)

    def quit(self):
        return self.call("call", "flea", "quit")

    def stop(self, **options):
        raise TransportError("stop the owning launch group instead of its peer")


def peers(window):
    record = window.record()
    if record["frontend"] != "gpui":
        return
    runtime = Path(record.get("runtime_dir") or window.root / "runtime")
    for path in sorted(runtime.rglob("*.sock")):
        if not path.stem.isdecimal() or int(path.stem) == record["pid"]:
            continue
        try:
            pid = int(path.stem)
            current = owned(pid, record)
            if current["state"] == "Z":
                continue
            info = path.lstat()
            peer = PeerWindow(window.root, dict(pid=pid, start=current["start"], socket=str(path),
                                               socket_identity=[info.st_dev, info.st_ino]))
            with endpoint(peer.record()):
                pass
        except (OSError, TransportError):
            continue
        yield peer
