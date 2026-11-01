"""Drive the shipped plugin examples through their actual line protocol."""
import contextlib
import json
import pathlib
import shutil
import subprocess
import tempfile
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]


@contextlib.contextmanager
def fixture():
    root = pathlib.Path(tempfile.mkdtemp(prefix="flea-plugin-examples-", dir="/tmp"))
    marker = root / ".flea-test-sandbox"
    marker.write_text("plugin example fixture\n")
    try:
        yield root
    finally:
        if (root.is_absolute() and str(root) and root.parent == pathlib.Path("/tmp")
                and root.name.startswith("flea-plugin-examples-") and marker.is_file()):
            shutil.rmtree(root)
        else:
            raise RuntimeError("refused unmarked plugin example cleanup")


# Sample input: {b"info": {b"name": b"fixture", b"length": 1}}.
def encode(value):
    if isinstance(value, int):
        return b"i" + str(value).encode() + b"e"
    if isinstance(value, bytes):
        return str(len(value)).encode() + b":" + value
    if isinstance(value, list):
        return b"l" + b"".join(map(encode, value)) + b"e"
    return b"d" + b"".join(encode(key) + encode(item) for key, item in sorted(value.items())) + b"e"


def exchange(plugin, requests):
    result = subprocess.run(
        ["python3", "-B", str(ROOT / "examples/plugins" / plugin)],
        input="\n".join(map(json.dumps, requests)) + "\n",
        capture_output=True, text=True, timeout=5, check=True,
    )
    return result.stdout.splitlines()


class Examples(unittest.TestCase):
    def test_corrupt_torrent_answers_error_without_ending_the_process(self):
        for data in [b"dle0:e", b"d4:infod5:filesi1eee"]:
            with self.subTest(data=data), fixture() as root:
                torrent = root / "corrupt.torrent"
                torrent.write_bytes(data)
                lines = exchange("torrent-preview/preview.py", [
                    {"op": "hello", "protocolVersion": 1},
                    {"op": "preview", "path": str(torrent)},
                    {"op": "hello", "protocolVersion": 1},
                ])
                self.assertEqual([json.loads(line)["op"] for line in lines], ["ready", "error", "ready"])

    def test_torrent_preview_bounds_serialized_ascii_and_unicode_messages(self):
        for name in ["a" * 400, "海" * 80]:
            with self.subTest(name=name[:4]), fixture() as root:
                torrent = root / "fixture.torrent"
                torrent.write_bytes(encode({b"info": {
                    b"name": b"fixture",
                    b"files": [{b"length": 1, b"path": [(str(i) + name).encode()]} for i in range(200)],
                }}))
                lines = exchange("torrent-preview/preview.py", [
                    {"op": "hello", "protocolVersion": 1},
                    {"op": "preview", "path": str(torrent)},
                ])
                self.assertEqual(len(lines), 2)
                self.assertLessEqual(len(lines[1].encode()), 65536)
                reply = json.loads(lines[1])
                self.assertEqual(reply["op"], "preview")
                self.assertTrue(reply["text"].startswith("fixture\n200 files"))
                self.assertTrue(reply["text"].endswith("\n[Preview truncated]"))

    def test_deleted_virtual_directory_cannot_be_listed(self):
        lines = exchange("virtual-place/bin/run", [
            {"op": "hello", "protocolVersion": 1},
            {"op": "delete", "paths": ["/Photos"]},
            {"op": "list", "path": "/Photos"},
            {"op": "list", "path": "/"},
        ])
        replies = list(map(json.loads, lines))
        self.assertEqual(replies[1], {"op": "done"})
        self.assertEqual(replies[2]["op"], "error")
        self.assertEqual([row["name"] for row in replies[3]["entries"]], ["notes.txt"])

    def test_deleting_parent_and_child_never_recreates_the_parent(self):
        lines = exchange("virtual-place/bin/run", [
            {"op": "hello", "protocolVersion": 1},
            {"op": "delete", "paths": ["/Photos", "/Photos/lake.png"]},
            {"op": "list", "path": "/Photos"},
        ])
        self.assertEqual(json.loads(lines[-1])["op"], "error")


if __name__ == "__main__":
    unittest.main()
