import atexit
import binascii
import collections
import hashlib
import json
import os
from pathlib import Path
import queue
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import zipfile
import zlib

binary = Path(sys.argv[1]).resolve(strict=True)
assert binary.is_file(), f"required current backend executable is missing: {binary}"
parent = Path(sys.argv[3]).resolve(strict=True)
root = Path(tempfile.mkdtemp(prefix="model3d-protocol.", dir=parent))
assert root.is_absolute() and root.is_relative_to(parent)
marker = root / ".flea-test-sandbox"
marker.write_text("3D protocol integration sandbox\n")
REPLY_TIMEOUT_S = 30
STDERR_LINES_KEPT = 200
THUMB_TABLE = "flea-3d-v2"


# Every exit path removes the sandbox, checked absolute, marked and inside the parent it was made in.
def remove_root():
    assert str(root) and root.is_absolute() and root.is_relative_to(parent) and root != parent and marker.is_file()
    shutil.rmtree(root)


atexit.register(remove_root)


def owned(name):
    path = root / name
    assert marker.is_file() and path.is_absolute() and path.is_relative_to(root) and ".." not in path.parts
    return path


# The thumbnail cache names an entry by the MD5 of the file's URI, under fail/flea or large.
def cache_entry(kind, path):
    return owned("cache/thumbnails") / kind / (hashlib.md5(path.as_uri().encode()).hexdigest() + ".png")


def png(width, height, stream=None):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", binascii.crc32(kind + data))
    if stream is None:
        stream = zlib.compress((b"\0" + bytes(width * 4)) * height)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)) + chunk(b"IDAT", stream) + chunk(b"IEND", b"")


# Sample input: b'FL3D\x01\0\0\0', u32le JSON length, JSON summary, then the model records.
def summary(path):
    data = path.read_bytes()
    assert data[:8] == b"FL3D\x01\0\0\0"
    count = struct.unpack_from("<I", data, 8)[0]
    return json.loads(data[12:12 + count])


def sandbox_available():
    if not shutil.which("bwrap") or not shutil.which("prlimit"):
        return False
    argv = ["bwrap", "--unshare-all", "--die-with-parent", "--ro-bind", "/usr", "/usr", "--symlink", "usr/lib", "/lib", "--symlink", "usr/lib64", "/lib64", "--symlink", "usr/bin", "/bin", "--proc", "/proc", "--dev", "/dev", "--", "/usr/bin/true"]
    result = subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE, timeout=10)
    return result.returncode == 0


class Session:
    def __init__(self, path=None):
        home = owned("home")
        home.mkdir(exist_ok=True)
        environment = {"PATH": path if path is not None else os.defpath, "HOME": str(home), "XDG_CACHE_HOME": str(owned("cache")), "XDG_CONFIG_HOME": str(owned("config")), "XDG_DATA_HOME": str(owned("data"))}
        self.child = subprocess.Popen([str(binary), "--backend"], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        self.lines = queue.Queue()
        self.errors = collections.deque(maxlen=STDERR_LINES_KEPT)
        threading.Thread(target=self.read, daemon=True).start()
        threading.Thread(target=self.drain, daemon=True).start()

    def read(self):
        for line in self.child.stdout:
            self.lines.put(json.loads(line))

    # Backend stderr is read as it comes, so a full pipe never blocks the backend; the tail explains a missing reply.
    def drain(self):
        for line in self.child.stderr:
            self.errors.append(line)

    def ask(self, command, kind):
        self.child.stdin.write(json.dumps(command) + "\n")
        self.child.stdin.flush()
        while True:
            try:
                result = self.lines.get(timeout=REPLY_TIMEOUT_S)
            except queue.Empty:
                raise AssertionError(f"no {kind} reply within {REPLY_TIMEOUT_S} s; backend stderr tail:\n{''.join(self.errors)}") from None
            if result.get("t") == kind:
                return result

    def close(self):
        self.child.stdin.write('{"c":"quit"}\n')
        self.child.stdin.flush()
        try:
            self.child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.child.kill()
            self.child.wait()


if sys.argv[2] == "missing-sandbox":
    empty = owned("empty-path")
    empty.mkdir()
    path = owned("part.obj")
    path.write_text("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n")
    session = Session(str(empty))
    try:
        result = session.ask({"c": "model3d", "slot": "look", "token": 1, "path": str(path)}, "model3d")
        assert result["state"] == "failed", result
        assert "bwrap and prlimit" in result["reason"], result
        assert not list(owned("cache/thumbnails/flea-3d").glob("*.3d"))
    finally:
        session.close()
else:
    if not sandbox_available():
        print("SKIP model3d_protocol_real_sandbox_model_facts_and_existing_thumbnail_cache: bwrap/prlimit cannot run here", file=sys.stderr)
        sys.exit(0)
    decoder = owned("decoder")
    decoder.write_bytes(Path(__file__).with_name("model3d_thumbnailer.py").read_bytes())
    decoder.chmod(0o700)
    specs = owned("data/thumbnailers")
    specs.mkdir(parents=True)
    owned("data/thumbnailers/model3d-test.thumbnailer").write_text(f"[Thumbnailer Entry]\nExec={json.dumps(str(decoder))} %i %o %s\nMimeType=image/png;\n")
    path = owned("kit.3mf")
    model = "<model><resources><object id='1'><mesh><vertices><vertex x='0' y='0' z='0'/><vertex x='1' y='0' z='0'/><vertex x='0' y='1' z='0'/><vertex x='0' y='0' z='1'/></vertices><triangles><triangle v1='0' v2='2' v3='1'/><triangle v1='0' v2='1' v3='3'/><triangle v1='0' v2='3' v3='2'/><triangle v1='1' v2='2' v3='3'/></triangles></mesh></object></resources><build><item objectid='1'/><item objectid='1'/></build></model>"
    plates = "<config><plate><metadata key='plater_id' value='1'/><model_instance><metadata key='object_id' value='1'/><metadata key='instance_id' value='0'/></model_instance></plate><plate><metadata key='plater_id' value='2'/><model_instance><metadata key='object_id' value='1'/><metadata key='instance_id' value='1'/></model_instance></plate></config>"
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("3D/3dmodel.model", model)
        archive.writestr("Metadata/model_settings.config", plates)
        archive.writestr("Metadata/thumbnail.png", png(512, 512))
    bare = owned("bare.3mf")
    with zipfile.ZipFile(bare, "w") as archive:
        archive.writestr("3D/3dmodel.model", model)
    broken = owned("broken.3mf")
    with zipfile.ZipFile(broken, "w") as archive:
        archive.writestr("3D/3dmodel.model", model)
        archive.writestr("Metadata/thumbnail.png", png(64, 64, stream=b"not zlib"))
    session = Session()
    try:
        rows = session.ask({"c": "list", "path": str(root), "first": 100}, "rows")["rows"]
        row = next(i for i, item in enumerate(rows) if item["n"] == path.name)
        assert rows[row]["t"] is True
        assert session.ask({"c": "thumb", "rows": [row], "cacheOnly": True}, "thumbed")["file"] == ""
        ready = session.ask({"c": "model3d", "slot": "look", "token": 7, "path": str(path)}, "model3d")
        assert ready["state"] == "ready" and ready["token"] == 7, ready
        file = Path(ready["file"])
        assert file.is_relative_to(root)
        facts = summary(file)
        assert facts["facts"]["objects"] == 2 and facts["facts"]["plates"] == 2, facts
        assert facts["facts"]["open_edges"] == 0, facts
        assert Path(ready["thumbnail"]).is_relative_to(root)
        thumb = session.ask({"c": "thumb", "rows": [row]}, "thumbed")
        image = Path(thumb["file"])
        assert image.is_file() and image.is_relative_to(root), thumb
        data = image.read_bytes()
        assert data[:8] == b"\x89PNG\r\n\x1a\n"
        assert struct.unpack_from(">II", data, 16) == (256, 256)
        assert b"Model3D-Test-Decoder\0sandboxed" in data
        assert b"Thumb::URI\0" + path.as_uri().encode() in data
        before = image.stat()
        cached = session.ask({"c": "thumb", "rows": [row]}, "thumbed")
        assert cached["file"] == str(image), cached
        after = image.stat()
        assert (after.st_ino, after.st_mtime_ns) == (before.st_ino, before.st_mtime_ns)
        assert session.ask({"c": "model3d", "slot": "look", "token": 8, "path": str(path)}, "model3d")["file"] == str(file)
        path.write_bytes(b"broken")
        failed = session.ask({"c": "model3d", "slot": "look", "token": 9, "path": str(path)}, "model3d")
        assert failed["state"] == "failed" and failed["message"] == "This file cannot be previewed."
        assert session.ask({"c": "model3d", "slot": "look", "token": 10, "cancel": True}, "model3d")["state"] == "cancelled"
        # A mesh with no embedded image gets no thumbnail but a failure record under the 3D table, which a repeat request reads.
        bare_row = next(i for i, item in enumerate(rows) if item["n"] == bare.name)
        assert rows[bare_row]["t"] is True
        assert session.ask({"c": "thumb", "rows": [bare_row]}, "thumbed")["file"] == ""
        record = cache_entry("fail/flea", bare).read_bytes()
        assert f"X-Flea-Table\0{THUMB_TABLE}".encode() in record, record
        assert f"Thumb::MTime\0{bare.stat().st_mtime_ns // 10**9}".encode() in record, record
        assert session.ask({"c": "thumb", "rows": [bare_row], "cacheOnly": True}, "thumbed")["file"] == ""
        # An embedded PNG whose pixel stream does not decode is refused by the sandboxed decoder and publishes nothing.
        broken_row = next(i for i, item in enumerate(rows) if item["n"] == broken.name)
        assert session.ask({"c": "thumb", "rows": [broken_row]}, "thumbed")["file"] == ""
        assert not cache_entry("large", broken).exists()
        assert session.ask({"c": "thumb", "rows": [broken_row], "cacheOnly": True}, "thumbed")["file"] == ""
    finally:
        session.close()
