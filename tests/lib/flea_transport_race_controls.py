"""Drive peer shutdown between shared-runtime discovery and endpoint authentication."""
import subprocess
import sys
from unittest import mock

from flea_transport import REPO, Window, load


def disappearing_peer(root, start, check, windows):
    owner = start()
    peer = load(owner.root)["socket"]
    probe = '''
import os
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[4])
import flea_transport as transport

# Args: /marked/new-root /marked/owner/runtime/flea-ipc/123.sock /marked/owner /repo/tests/lib.
root, peer, owner = map(Path, sys.argv[1:4])
original_stat, original_glob = Path.lstat, Path.rglob
fired = False

def ordered(self, *args, **kwargs):
    entries = list(original_glob(self, *args, **kwargs))
    return iter(sorted(entries, key=lambda path: path != peer))

def vanished(self, *args, **kwargs):
    global fired
    if self == peer and not fired:
        assert peer.is_absolute() and str(peer) and (owner / transport.MARKER).is_file()
        assert peer.is_relative_to(owner / 'runtime')
        fired = True
        os.unlink(peer)
        (root / 'peer-race-injected').touch()
    return original_stat(self, *args, **kwargs)

Path.rglob, Path.lstat = ordered, vanished
transport.serve(root)
'''
    original_popen = subprocess.Popen

    def injected(command, **options):
        if command[-2:-1] == ["_serve"]:
            command = [sys.executable, "-B", "-c", probe, command[-1], peer,
                       str(owner.root), str(REPO / "tests/lib")]
        return original_popen(command, **options)

    with mock.patch("flea_transport.subprocess.Popen", side_effect=injected):
        launched = Window.start(root, parent=root, frontend="gpui", timeout=1, share=owner.root)
    windows.append(launched)
    check((launched.root / "peer-race-injected").is_file(), "shared-peer-shutdown-race-injected")
    check(launched.call("call", "flea", "path").stdout == b"owned\n", "shared-peer-shutdown-keeps-new-launch")
    launched.stop()
    owner.stop()
