"""Small marked fixtures through the production search session, including pageable raw identities."""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import threading

parent = (Path.cwd() / ".superpowers" / "fb2").resolve()
parent.mkdir(parents=True, exist_ok=True)
root = Path(subprocess.check_output(["mktemp", "-d", str(parent / "search-match.XXXXXX")], text=True).strip())
marker = root / ".flea-test-sandbox"
marker.write_text("flea test sandbox\n")
process = None
try:
    tree = root / "tree"
    tree.mkdir()
    files = ["render", "render_more", "rander", "render_frame.rs", "render12", "render13", "abxd", "abxxefgh", "downloads/helper.txt", "icon.svg", "icon.sxg", "CAFÉ.txt"]
    for name in files:
        path = tree / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(b"")
    raw = os.fsencode(tree) + b"/caf\xff.txt"
    with open(raw, "xb"):
        pass
    env = dict(os.environ)
    for key in ["XDG_CACHE_HOME", "XDG_STATE_HOME", "XDG_CONFIG_HOME", "XDG_RUNTIME_DIR", "XDG_DATA_HOME"]:
        directory = root / key.lower()
        directory.mkdir(mode=0o700)
        env[key] = str(directory)
    process = subprocess.Popen([sys.argv[1], "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, env=env)
    lines = queue.Queue()
    def read():
        for line in process.stdout:
            lines.put(json.loads(line))
        lines.put(None)
    reader = threading.Thread(target=read, daemon=True)
    reader.start()
    def send(**value):
        process.stdin.write(json.dumps(value) + "\n")
        process.stdin.flush()
    def until(predicate):
        while True:
            line = lines.get(timeout=30)
            assert line is not None, "backend ended"
            assert line.get("t") != "error", line
            if predicate(line):
                return line
    send(c="searchopen", path=str(tree), folder=str(tree), hidden=False)
    until(lambda line: line.get("t") == "searchopened")
    generation = 0
    def query(text):
        global generation
        generation += 1
        send(c="searchquery", query=text, gen=generation, count=1, sizes=0)
        done = until(lambda line: line.get("t") == "searched" and line.get("gen") == generation and not line["walking"])
        assert done["partial"] is False, done
        send(c="paths", rows=list(range(done["n"])))
        paths = until(lambda line: line.get("t") == "paths")["paths"]
        assert len(paths) == done["n"], (text, done, paths)
        return [str(Path(path).relative_to(tree)) for path in paths]
    for text in ["redner_frame.rs", "rander_frame.rs", "rendxer_frame.rs", "rener_frame.rs", "xender_frame.rs"]:
        assert "render_frame.rs" in query(text), text
    assert "abxd" not in query("abc")
    assert "abxd" in query("abcd")
    assert "abxxefgh" not in query("abcdefg")
    assert "abxxefgh" in query("abcdefgh")
    assert query("render")[0] == "render"
    ranked = query("render")
    assert "render_more" in ranked and "rander" not in ranked, ranked
    assert "rander" in query("rnader")
    assert "downloads/helper.txt" in query("dwnhelp")
    assert "downloads" in query("downloads")
    assert "render13" not in query("render12")
    assert query(".svg") == ["icon.svg"]
    assert query("café.txt") == ["CAFÉ.txt"]
    assert len(query("caf")) == 2
    send(c="searchclose")
    until(lambda line: line.get("t") == "searchclosed")
    other = root / "other"
    other.mkdir()
    (other / "abce").write_bytes(b"")
    send(c="searchopen", path=str(other), folder=str(other), hidden=False)
    until(lambda line: line.get("t") == "searchopened")
    generation += 1
    send(c="searchquery", query="abc", gen=generation, count=10, sizes=0)
    done = until(lambda line: line.get("t") == "searched" and line.get("gen") == generation and not line["walking"])
    assert done["n"] == 1, done
    send(c="quit")
    process.wait(timeout=10)
    assert process.returncode == 0, process.stderr.read()
    print("SEARCH_CONSERVATIVE_PROTOCOL edits=4 first_letter=ok thresholds=4,8 digits=protected punctuation=literal unicode=ok raw=2 paging=ok scope=ok")
finally:
    if process is not None and process.poll() is None:
        process.kill()
        process.wait(timeout=10)
    assert str(root) and root.is_absolute() and root.parent == parent and root.name.startswith("search-match.")
    assert marker.read_text() == "flea test sandbox\n"
    shutil.rmtree(root)
