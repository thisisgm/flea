#!/usr/bin/env python3
"""The Stack for Flea: up to 100 files, newest access first.

A file's access time is the latest of its recently-used.xbel stamps
(added, modified, visited, and each app's modified), its own mtime, and
a touch written when Flea opens it. Directories are left out. A path
that is not there is left out, so a file on an external drive shows up
only while that drive is plugged in. Any absolute path qualifies.

Stdout is one JSON object: {"paths": ["<newest>", ...]}.
"""

from __future__ import annotations

import json
import os
import stat
import sys
import tempfile
import threading
import time
import xml.etree.ElementTree as ET
from pathlib import Path

LIMIT = 100
TOUCH_KEEP = 200
STAT_BUDGET_S = 1.5
BOOKMARK_NS = "{http://www.freedesktop.org/standards/desktop-bookmarks}"


def locations():
    home = os.environ.get("THE_STACK_HOME") or str(Path.home())
    history = os.environ.get("THE_STACK_HISTORY")
    if not history:
        data_home = os.environ.get("XDG_DATA_HOME") or os.path.join(home, ".local", "share")
        if not data_home.startswith("/"):
            data_home = os.path.join(home, ".local", "share")
        history = os.path.join(data_home, "recently-used.xbel")
    touch = os.environ.get("THE_STACK_TOUCH") or os.path.join(home, ".local", "state", "flea", "the-stack.json")
    if "THE_STACK_DOWNLOADS" in os.environ:
        downloads = os.environ["THE_STACK_DOWNLOADS"]
    else:
        downloads = download_dir(home)
    return home, history, touch, downloads


def download_dir(home: str) -> str:
    path = os.path.join(home, ".config", "user-dirs.dirs")
    try:
        text = Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        return os.path.join(home, "Downloads")
    for line in text.splitlines():
        line = line.strip()
        if not line.startswith("XDG_DOWNLOAD_DIR="):
            continue
        raw = line.split("=", 1)[1].strip().strip('"')
        raw = raw.replace("$HOME", home)
        raw = raw.rstrip("/")
        return raw or home
    return os.path.join(home, "Downloads")


def stamp_of(value: str | None) -> float:
    text = (value or "").strip()
    if not text:
        return 0.0
    if text.endswith("Z"):
        text = text[:-1] + "+00:00"
    try:
        from datetime import datetime

        return datetime.fromisoformat(text).timestamp()
    except (ValueError, OverflowError):
        return 0.0


def path_of(href: str) -> str:
    raw = href or ""
    if raw[:7].lower() != "file://":
        return ""
    rest = raw[7:]
    cut = rest.find("/")
    if cut < 0:
        return ""
    authority = rest[:cut].lower()
    if authority and authority != "localhost":
        return ""
    try:
        decoded = _percent_decode(rest[cut:])
    except ValueError:
        return ""
    if not decoded.startswith("/") or decoded == "/":
        return ""
    if any(ord(ch) < 32 or ord(ch) == 127 for ch in decoded):
        return ""
    return os.path.normpath(decoded)


def _percent_decode(text: str) -> str:
    out = bytearray()
    i = 0
    data = text.encode("utf-8")
    while i < len(data):
        if data[i] == 0x25 and i + 2 < len(data):
            try:
                out.append(int(data[i + 1 : i + 3], 16))
            except ValueError:
                out.append(data[i])
                i += 1
                continue
            i += 3
            continue
        out.append(data[i])
        i += 1
    decoded = out.decode("utf-8")
    if "\x00" in decoded:
        raise ValueError("nul")
    return decoded


def bookmark_scores(history: str) -> dict[str, float]:
    scores: dict[str, float] = {}
    try:
        if os.path.getsize(history) > 20_000_000:
            return scores
        root = ET.parse(history).getroot()
    except (OSError, ET.ParseError):
        return scores
    for node in root.findall("bookmark"):
        path = path_of(node.get("href") or "")
        if not path:
            continue
        score = max(stamp_of(node.get("added")), stamp_of(node.get("modified")), stamp_of(node.get("visited")))
        for app in node.iter(BOOKMARK_NS + "application"):
            score = max(score, stamp_of(app.get("modified")))
        previous = scores.get(path, 0.0)
        if score > previous:
            scores[path] = score
    return scores


def load_touches(path: str) -> dict[str, float]:
    try:
        raw = json.loads(Path(path).read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError, UnicodeError):
        return {}
    if not isinstance(raw, dict):
        return {}
    out: dict[str, float] = {}
    for key, value in raw.items():
        if not isinstance(key, str) or not key.startswith("/") or "\x00" in key or "\n" in key:
            continue
        try:
            when = float(value)
        except (TypeError, ValueError):
            continue
        if when > 0:
            out[os.path.normpath(key)] = when
    return out


def save_touches(path: str, touches: dict[str, float]) -> None:
    kept = sorted(touches.items(), key=lambda item: item[1], reverse=True)[:TOUCH_KEEP]
    body = json.dumps(dict(kept), separators=(",", ":"))
    directory = os.path.dirname(path)
    os.makedirs(directory, mode=0o700, exist_ok=True)
    fd, temporary = tempfile.mkstemp(prefix=".the-stack-", dir=directory)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(body)
            handle.write("\n")
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    except Exception:
        try:
            os.unlink(temporary)
        except OSError:
            pass
        raise


def record_touch(touch_path: str, target: str) -> None:
    if not target.startswith("/") or "\x00" in target or "\n" in target or "\r" in target:
        raise SystemExit("touch path must be absolute")
    touches = load_touches(touch_path)
    touches[os.path.normpath(target)] = time.time()
    save_touches(touch_path, touches)


class _Info:
    __slots__ = ("mtime", "is_file")

    def __init__(self, mtime: float, is_file: bool) -> None:
        self.mtime = mtime
        self.is_file = is_file


def _classify(path: str) -> _Info | None:
    try:
        st = os.stat(path, follow_symlinks=False)
    except FileNotFoundError:
        return None
    except OSError:
        return _Info(0.0, True)
    if stat.S_ISDIR(st.st_mode):
        return _Info(st.st_mtime, False)
    if stat.S_ISLNK(st.st_mode):
        try:
            target = os.stat(path, follow_symlinks=True)
        except FileNotFoundError:
            return None
        except OSError:
            return _Info(st.st_mtime, True)
        if stat.S_ISDIR(target.st_mode):
            return _Info(target.st_mtime, False)
        if stat.S_ISREG(target.st_mode):
            return _Info(max(st.st_mtime, target.st_mtime), True)
        return None
    if stat.S_ISREG(st.st_mode):
        return _Info(st.st_mtime, True)
    return None


def classify_many(paths: list[str], budget: float = STAT_BUDGET_S) -> dict[str, _Info | None]:
    found: dict[str, _Info | None] = {}
    lock = threading.Lock()

    def run(path: str) -> None:
        info = _classify(path)
        with lock:
            found[path] = info

    threads = []
    for path in paths:
        thread = threading.Thread(target=run, args=(path,), daemon=True)
        thread.start()
        threads.append(thread)
    deadline = time.monotonic() + budget
    for thread in threads:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            break
        thread.join(remaining)
    return found


def download_files(directory: str, home: str) -> list[str]:
    if not directory or directory == home or not directory.startswith("/"):
        return []
    try:
        entries = list(os.scandir(directory))
    except OSError:
        return []
    names = []
    for entry in entries:
        if entry.name.startswith("."):
            continue
        names.append(entry.path)
    return names


def ranked(home: str, history: str, touch: str, downloads: str) -> list[str]:
    scores = bookmark_scores(history)
    for path, when in load_touches(touch).items():
        scores[path] = max(scores.get(path, 0.0), when)
    extra = download_files(downloads, home)
    candidates = list(dict.fromkeys(list(scores) + extra))
    info = classify_many(candidates)
    ranked_rows = []
    for path in candidates:
        if path not in info:
            continue
        about = info[path]
        if about is None or not about.is_file:
            continue
        when = max(scores.get(path, 0.0), about.mtime)
        if when <= 0:
            continue
        ranked_rows.append((when, path))
    ranked_rows.sort(key=lambda row: (-row[0], row[1]))
    return [path for _, path in ranked_rows[:LIMIT]]


def emit() -> int:
    home, history, touch, downloads = locations()
    json.dump({"paths": ranked(home, history, touch, downloads)}, sys.stdout, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0


def self_test() -> int:
    import shutil

    root = tempfile.mkdtemp(prefix="the-stack-")
    try:
        home = os.path.join(root, "home")
        outside = os.path.join(root, "media", "usb")
        downloads = os.path.join(home, "Downloads")
        os.makedirs(downloads)
        os.makedirs(outside)
        older = os.path.join(home, "older.txt")
        newer = os.path.join(home, "newer.txt")
        folder = os.path.join(home, "folder")
        missing = os.path.join(home, "gone.txt")
        external = os.path.join(outside, "stick.txt")
        grabbed = os.path.join(downloads, "grabbed.bin")
        linked_dir = os.path.join(home, "linkdir")
        os.mkdir(folder)
        spaced = os.path.join(home, "a b.txt")
        Path(older).write_text("old")
        Path(newer).write_text("new")
        Path(external).write_text("usb")
        Path(grabbed).write_text("dl")
        Path(spaced).write_text("sp")
        os.symlink(folder, linked_dir)
        os.utime(external, (1_704_067_200, 1_704_067_200))
        os.utime(spaced, (1_600_000_000, 1_600_000_000))
        old_stamp = "2020-01-01T00:00:00Z"
        new_stamp = "2024-06-01T12:00:00Z"
        history = os.path.join(root, "recent.xbel")
        Path(history).write_text(
            """<?xml version="1.0"?>
<xbel version="1.0" xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
  <bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>
  <bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>
  <bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>
  <bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>
  <bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>
  <bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>
  <bookmark href="%s" added="%s" modified="%s" visited="%s"/>
</xbel>
"""
            % (
                older, old_stamp, old_stamp, old_stamp,
                newer, old_stamp, old_stamp, old_stamp,
                folder, new_stamp, new_stamp, new_stamp,
                missing, new_stamp, new_stamp, new_stamp,
                external, old_stamp, old_stamp, "2024-01-01T00:00:00Z",
                linked_dir, new_stamp, new_stamp, new_stamp,
                "file://" + home + "/a%20b.txt", old_stamp, old_stamp, "2023-01-01T00:00:00Z",
            )
        )
        # newer.txt was written just now, so its mtime outranks older.txt's old visit.
        os.utime(older, (1_577_836_800, 1_577_836_800))
        touch = os.path.join(root, "touch.json")
        os.environ["THE_STACK_HOME"] = home
        os.environ["THE_STACK_HISTORY"] = history
        os.environ["THE_STACK_TOUCH"] = touch
        os.environ["THE_STACK_DOWNLOADS"] = downloads
        paths = ranked(home, history, touch, downloads)
        assert newer in paths and older in paths and external in paths and grabbed in paths and spaced in paths, paths
        assert folder not in paths and missing not in paths and linked_dir not in paths, paths
        assert paths.index(newer) < paths.index(older), paths
        assert paths.index(grabbed) < paths.index(external), paths
        record_touch(touch, older)
        paths = ranked(home, history, touch, downloads)
        assert paths[0] == older, paths
        many = os.path.join(home, "many")
        os.makedirs(many)
        history_many = os.path.join(root, "many.xbel")
        lines = ["<?xml version='1.0'?><xbel version='1.0'>"]
        for i in range(120):
            path = os.path.join(many, "f%03d.txt" % i)
            Path(path).write_text("x")
            os.utime(path, (1_600_000_000 + i, 1_600_000_000 + i))
            stamp = "2020-01-01T00:00:%02dZ" % (i % 60)
            lines.append(
                '<bookmark href="file://%s" added="%s" modified="%s" visited="%s"/>' % (path, stamp, stamp, stamp)
            )
        lines.append("</xbel>")
        Path(history_many).write_text("".join(lines))
        os.environ["THE_STACK_HISTORY"] = history_many
        os.environ["THE_STACK_DOWNLOADS"] = ""
        fresh_touch = os.path.join(root, "fresh-touch.json")
        capped = ranked(home, history_many, fresh_touch, "")
        assert len(capped) == LIMIT, len(capped)
        assert capped[0].endswith("f119.txt"), capped[0]
        # A downloads directory that is $HOME itself must not list the whole home.
        os.environ["THE_STACK_DOWNLOADS"] = home
        os.environ["THE_STACK_HISTORY"] = os.path.join(root, "empty.xbel")
        Path(os.environ["THE_STACK_HISTORY"]).write_text("<xbel version='1.0'/>")
        assert ranked(home, os.environ["THE_STACK_HISTORY"], os.path.join(root, "no-touch.json"), home) == []
        print("self-test ok", len(paths), "live sample would be separate")
        return 0
    finally:
        shutil.rmtree(root, ignore_errors=True)
        for key in ("THE_STACK_HOME", "THE_STACK_HISTORY", "THE_STACK_TOUCH", "THE_STACK_DOWNLOADS"):
            os.environ.pop(key, None)


def main(argv: list[str]) -> int:
    if len(argv) == 1:
        return emit()
    if argv[1:] == ["--self-test"]:
        return self_test()
    if len(argv) == 3 and argv[1] == "--touch":
        _, _, touch, _ = locations()
        record_touch(touch, argv[2])
        return 0
    sys.stderr.write("usage: the-stack.py [--touch PATH]\n")
    return 2


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv))
    except BrokenPipeError:
        raise SystemExit(0)
