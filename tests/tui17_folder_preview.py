import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import zlib

binary, mode = Path(sys.argv[1]), sys.argv[2]
root = Path(tempfile.mkdtemp(prefix="flea-test-tui17-folder-"))
assert root.is_absolute() and root.parent == Path(tempfile.gettempdir())
(root / ".flea-test-sandbox").write_text("TUI17 folder preview fixture\n")

def cleanup():
    assert root.is_absolute() and str(root) and (root / ".flea-test-sandbox").is_file()
    shutil.rmtree(root)

try:
    for name in ("home", "data", "config", "cache", "state", "runtime", "work/A/empty", "work/A/full"):
        (root / name).mkdir(parents=True, exist_ok=True)
    os.chmod(root / "runtime", 0o700)
    for name in ("alpha.txt", "beta.md", "gamma.txt"):
        (root / "work/A" / name).write_text(f"top file {name}\n")
    for index in range(1, 6):
        (root / "work/A/full" / f"f{index}.txt").write_text(f"full file {index}\n")
    for name in ("Empty", "Locked", "Many", "Media"):
        (root / "work" / name).mkdir()
    (root / "work/Media/letter.odt").write_bytes(b"PK not really an office document\n")
    (root / "work/Media/blob.dat").write_bytes(b"\x00\x01\x02flea visual gate blob\x03")
    (root / "work/notes.txt").write_text("notes\n")
    (root / "work/todo.md").write_text("todo\n")
    (root / "work/Code").mkdir()
    (root / "work/Docs").mkdir()
    for path in (root / "work").rglob("*"):
        os.utime(path, (1749981600, 1749981600))
    env = dict(PATH="/usr/bin:/bin", HOME=str(root / "home"), LANG="C.UTF-8", TERM="xterm-kitty", COLORTERM="truecolor", TZ="UTC")
    for variable, name in (("XDG_DATA_HOME", "data"), ("XDG_CONFIG_HOME", "config"), ("XDG_CACHE_HOME", "cache"), ("XDG_STATE_HOME", "state"), ("XDG_RUNTIME_DIR", "runtime")):
        env[variable] = str(root / name)
    if mode == "parity_protocol":
        child = subprocess.Popen([str(binary), "--backend"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        pending = bytearray()
        def request(value):
            child.stdin.write((json.dumps(value) + "\n").encode())
            child.stdin.flush()
        def answer(kind):
            deadline = time.monotonic() + 20
            while time.monotonic() < deadline:
                while b"\n" in pending:
                    line, _, rest = pending.partition(b"\n")
                    pending[:] = rest
                    value = json.loads(line)
                    if value.get("t") == kind:
                        return value
                if select.select([child.stdout], [], [], .05)[0]:
                    chunk = os.read(child.stdout.fileno(), 65536)
                    assert chunk, "backend stopped before its reply"
                    pending.extend(chunk)
            raise AssertionError(f"backend did not return {kind}")
        try:
            request(dict(c="list", path=str(root / "work/A"), first=31))
            answer("listed")
            listing = answer("rows")["listing"]
            request(dict(c="filter", traverse=True, query="^alpha", id=101))
            found = answer("found")
            assert found["listing"] == listing and found["id"] == 101 and len(found["rows"]) == 1, found
            request(dict(c="paths", rows=found["rows"], listing=listing, hold=True, id=102))
            held = answer("paths")
            assert held["id"] == 102 and held["listing"] == listing and held["paths"] == [str(root / "work/A/alpha.txt")], held
            assert len(held["held"]) == 1 and isinstance(held["held"][0]["ino"], str), held
            request(dict(c="quit"))
            child.communicate(timeout=10)
            assert child.returncode == 0
            print("PASS real backend: smart find and held Copy are single stamped JSON lines")
        finally:
            if child.poll() is None:
                child.kill()
                child.communicate(timeout=10)
    elif mode == "protocol":
        requests = [dict(c="peek", path=str(root / "work/A"), first=31, details=True), dict(c="quit")]
        output = subprocess.run([str(binary), "--backend"], input="".join(json.dumps(r)+"\n" for r in requests), text=True, capture_output=True, env=env, timeout=30)
        assert output.returncode == 0, output.stderr
        reply = next(json.loads(line) for line in output.stdout.splitlines() if json.loads(line).get("t") == "peeked")
        rows = {row["n"]: row for row in reply["rows"]}
        for name, size in (("alpha.txt", 19), ("beta.md", 17), ("gamma.txt", 19)):
            assert rows[name].get("s") == size, (name, rows[name], "missing child size")
            assert rows[name].get("p", 0) & 0o170000 == 0o100000, (name, rows[name], "missing child mode")
        print("PASS real backend peek: alpha.txt=19 B beta.md=17 B gamma.txt=19 B")
    else:
        from tui_battery.cells import Screen, text
        env["KITTY_WINDOW_ID"] = "tui17-pty-fixture"
        palette = root / "home/.local/state/omarchy/current/theme/colors.toml"
        palette.parent.mkdir(parents=True)
        palette.write_text('background = "#1c120d"\nforeground = "#c6b395"\naccent = "#d28a41"\ncolor1 = "#d65a4f"\ncolor2 = "#79ab55"\ncolor4 = "#509467"\ncolor8 = "#8b7b74"\n')
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 31, 130, 1092, 682))
        def session():
            os.setsid()
            fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
        selected = root / ("work/Media/letter.odt" if mode == "unsupported" else "work/Media/blob.dat" if mode == "blob" else "work/notes.txt" if mode in ("filter", "properties") else "work/A")
        child = subprocess.Popen([str(binary), "--tui", "--select", str(selected)], stdin=slave, stdout=slave, stderr=slave, env=env, preexec_fn=session)
        os.close(slave)
        screen, transcript = Screen(130, 31), bytearray()
        try:
            deadline = time.monotonic() + 20
            matched, typed = False, False
            stable, stable_at = None, time.monotonic()
            lines = [""] * 31
            while time.monotonic() < deadline and child.poll() is None:
                chunk = os.read(master, 65536) if select.select([master], [], [], .05)[0] else b""
                transcript.extend(chunk)
                screen.feed(chunk)
                if b"\x1b[?u" in chunk: os.write(master, b"\x1b[?0u")
                if b"\x1b[c" in chunk: os.write(master, b"\x1b[?62c")
                if b"\x1b[16t" in chunk: os.write(master, b"\x1b[6;22;8t")
                if b"\x1b[14t" in chunk: os.write(master, b"\x1b[4;682;1092t")
                lines = ["".join(cell["ch"] for cell in row) for row in screen.grid]
                matching = [line[79:] for line in lines if any(name in line[79:] for name in ("alpha.txt", "beta.md", "gamma.txt"))]
                if mode == "properties":
                    if not typed and any("notes.txt" in line for line in lines):
                        os.write(master, b"i")
                        typed = True
                    matched = any("Kind" in line and "Plain text document" in line for line in lines)
                elif mode == "blob":
                    list_mark = any("\ueae8 blob.dat" in line[28:79] for line in lines)
                    head_mark = any("\ueae8 blob.dat" in line[79:] for line in lines)
                    matched = list_mark and head_mark and any("Data" in line[79:] and "25 B" in line[79:] for line in lines)
                elif mode == "filter":
                    if not typed and any("notes.txt" in line for line in lines):
                        os.write(master, b"/o")
                        typed = True
                    if not any("Filter o" in line for line in lines):
                        continue
                    hits = []
                    for row in screen.grid[2:-2]:
                        for name in ("notes.txt", "todo.md"):
                            line = "".join(cell["ch"] for cell in row)
                            at = line.find(name, 28, 79)
                            if at < 0:
                                continue
                            index = at + name.index("o")
                            hits.append((row[index]["bold"], row[index]["bg"] == row[index - 1]["bg"] == row[index + 1]["bg"]))
                    matched = len(hits) >= 2 and all(bold and same_fill for bold, same_fill in hits)
                elif mode == "unsupported":
                    matched = any("This document could not be previewed." in line[79:] for line in lines) and any("Open With" in line[79:] for line in lines)
                else:
                    matched = all(any(name in line and f"{size} B" in line for line in matching) for name, size in (("alpha.txt",19),("beta.md",17),("gamma.txt",19)))
                if matched and b"\x1b[38;2;198;179;149m" in transcript:
                    fingerprint = json.dumps(screen.grid, sort_keys=True)
                    if fingerprint != stable:
                        stable, stable_at = fingerprint, time.monotonic()
                    elif time.monotonic() - stable_at >= .15:
                        break
                else:
                    stable = None
            assert matched and stable is not None and time.monotonic() - stable_at >= .15, f"{mode} settled TUI board state absent:\n" + "\n".join(lines)
            record = dict(columns=130, rows=31, preview_columns=50, binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), vt_sha256=hashlib.sha256(transcript).hexdigest(), grid=screen.grid, kind=f"real --tui PTY cell capture: {mode}")
            print("TUI17_CAPTURE_ZLIB " + base64.b64encode(zlib.compress(json.dumps(record).encode())).decode())
            print(f"PASS real --tui board state: {mode} at 130 columns")
        finally:
            if child.poll() is None:
                os.killpg(child.pid, signal.SIGTERM)
            try:
                child.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=10)
            os.close(master)
finally:
    cleanup()
