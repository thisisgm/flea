#!/usr/bin/env python3
"""The Stack for Flea: up to 100 files, newest access first.

A file's access time is the latest of its recently-used.xbel stamps
(added, modified, visited, and each app's modified), its own mtime, and
a touch written when Flea opens it or when a watched folder sees a new
file or an open. Directories are left out. A path that is not there is
left out, so a file on an external drive shows up only while that drive
is plugged in. Any absolute path qualifies.

Stdout is one JSON object: {"paths": ["<newest>", ...]}.
"""

from __future__ import annotations

import ctypes
import ctypes.util
import fcntl
import json
import os
import signal
import stat
import struct
import sys
import tempfile
import threading
import time
import xml.etree.ElementTree as ET
from pathlib import Path

LIMIT = 100
TOUCH_KEEP = 200
STAT_BUDGET_S = 1.5
BIRTH_DIR_CAP = 4000
TOUCH_GAP_S = 0.4
BOOKMARK_NS = "{http://www.freedesktop.org/standards/desktop-bookmarks}"
# These are the folders a person saves into. A user-dir that is $HOME itself is not one of them.
BIRTH_KEYS = (
    "XDG_DOWNLOAD_DIR",
    "XDG_DOCUMENTS_DIR",
    "XDG_MUSIC_DIR",
    "XDG_PICTURES_DIR",
    "XDG_VIDEOS_DIR",
    "XDG_PROJECTS_DIR",
)
# Build trees move on their own. They are not files the person just saved.
SKIP_DIRS = {
    "node_modules", "target", "dist", "build", "__pycache__", ".git", "venv", ".venv",
    "vendor", "coverage", ".next", "out", ".cache", "debug", "release", "CMakeFiles",
    ".gradle", "deps", "_build", "site-packages",
}
PARTIAL_SUFFIXES = (
    ".crdownload", ".part", ".partial", ".tmp", ".download", ".opdownload", ".swp",
)
# Thumbnailers and Flea itself open files to draw them. That is not the person opening the file.
IGNORE_COMM = {"qs", "flea", "flea-bin"}
IGNORE_COMM_PREFIXES = ("gdk-pixbuf", "ffmpegthumb", "totem-video", "tumbler", "thumbnail")
IN_CLOSE_WRITE = 0x00000008
IN_OPEN = 0x00000020
IN_MOVED_TO = 0x00000080
IN_CREATE = 0x00000100
IN_Q_OVERFLOW = 0x00004000
IN_IGNORED = 0x00008000
IN_ONLYDIR = 0x01000000
IN_DONT_FOLLOW = 0x02000000
IN_EXCL_UNLINK = 0x04000000
IN_ISDIR = 0x40000000
IN_NONBLOCK = 0x00000800
WATCH_MASK = (
    IN_CREATE | IN_MOVED_TO | IN_CLOSE_WRITE | IN_OPEN | IN_ONLYDIR | IN_DONT_FOLLOW | IN_EXCL_UNLINK
)
EVENT_HEAD = struct.Struct("iIII")


def locations():
    home = os.environ.get("THE_STACK_HOME") or str(Path.home())
    history = os.environ.get("THE_STACK_HISTORY")
    if not history:
        data_home = os.environ.get("XDG_DATA_HOME") or os.path.join(home, ".local", "share")
        if not data_home.startswith("/"):
            data_home = os.path.join(home, ".local", "share")
        history = os.path.join(data_home, "recently-used.xbel")
    touch = os.environ.get("THE_STACK_TOUCH") or os.path.join(home, ".local", "state", "flea", "the-stack.json")
    # Tests pin one directory, including an empty string that means "scan nothing".
    if "THE_STACK_DOWNLOADS" in os.environ:
        raw = os.environ["THE_STACK_DOWNLOADS"]
        births = [raw] if raw else []
    else:
        births = birth_directories(home)
    return home, history, touch, births


def _user_dir_map(home: str) -> dict[str, str]:
    found: dict[str, str] = {}
    path = os.path.join(home, ".config", "user-dirs.dirs")
    try:
        text = Path(path).read_text(encoding="utf-8", errors="replace")
    except OSError:
        text = ""
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, raw = line.split("=", 1)
        raw = raw.strip().strip('"').replace("$HOME", home).rstrip("/")
        if key and raw:
            found[key] = raw
    return found


def _real(path: str) -> str:
    try:
        return os.path.realpath(path)
    except OSError:
        return path


def birth_directories(home: str) -> list[str]:
    """Folders whose new files belong on The Stack. $HOME itself is never one of them."""
    home_real = _real(home) if home else ""
    mapped = _user_dir_map(home)
    defaults = {
        "XDG_DOWNLOAD_DIR": os.path.join(home, "Downloads"),
        "XDG_DOCUMENTS_DIR": os.path.join(home, "Documents"),
        "XDG_MUSIC_DIR": os.path.join(home, "Music"),
        "XDG_PICTURES_DIR": os.path.join(home, "Pictures"),
        "XDG_VIDEOS_DIR": os.path.join(home, "Videos"),
        "XDG_PROJECTS_DIR": os.path.join(home, "Projects"),
    }
    raws = [mapped.get(key) or defaults[key] for key in BIRTH_KEYS]
    extra = os.environ.get("OMARCHY_SCREENSHOT_DIR", "").strip().replace("$HOME", home).rstrip("/")
    if extra:
        raws.append(extra)
    seen: set[str] = set()
    out: list[str] = []
    for raw in raws:
        if not raw or not raw.startswith("/") or not os.path.isdir(raw):
            continue
        real = _real(raw)
        if not real or real == home_real or real in seen:
            continue
        seen.add(real)
        out.append(real)
    return out


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


def keep_dir(name: str) -> bool:
    return bool(name) and not name.startswith(".") and name not in SKIP_DIRS


def keep_file(name: str) -> bool:
    if not name or name.startswith(".") or name.endswith("~"):
        return False
    lowered = name.lower()
    return not lowered.endswith(PARTIAL_SUFFIXES)


def birth_scores(directories: list[str], home: str) -> dict[str, float]:
    """mtime of regular files under the birth folders. A hung walk keeps what it already found."""
    found: dict[str, float] = {}
    lock = threading.Lock()
    home_real = _real(home) if home else ""

    def run() -> None:
        seen_dirs = 0
        for directory in directories:
            if not directory or not directory.startswith("/"):
                continue
            real = _real(directory)
            if not real or (home_real and real == home_real) or not os.path.isdir(real):
                continue
            for dirpath, dirnames, filenames in os.walk(real, followlinks=False):
                seen_dirs += 1
                if seen_dirs > BIRTH_DIR_CAP:
                    return
                dirnames[:] = [name for name in dirnames if keep_dir(name)]
                for name in filenames:
                    if not keep_file(name):
                        continue
                    path = os.path.join(dirpath, name)
                    info = _classify(path)
                    if info is None or not info.is_file or info.mtime <= 0:
                        continue
                    with lock:
                        found[path] = info.mtime

    walker = threading.Thread(target=run, daemon=True)
    walker.start()
    walker.join(STAT_BUDGET_S)
    with lock:
        return dict(found)


def ranked(home: str, history: str, touch: str, births: list[str]) -> list[str]:
    scores = bookmark_scores(history)
    for path, when in load_touches(touch).items():
        scores[path] = max(scores.get(path, 0.0), when)
    born = birth_scores(births, home)
    for path, when in born.items():
        scores[path] = max(scores.get(path, 0.0), when)
    # Birth files were just classified. Everything else may live on a drive that is slow to stat.
    pending = [path for path in scores if path not in born]
    info = classify_many(pending)
    ranked_rows = []
    for path, when in scores.items():
        if path in born:
            if when <= 0:
                continue
            ranked_rows.append((when, path))
            continue
        if path not in info:
            continue
        about = info[path]
        if about is None or not about.is_file:
            continue
        scored = max(when, about.mtime)
        if scored <= 0:
            continue
        ranked_rows.append((scored, path))
    ranked_rows.sort(key=lambda row: (-row[0], row[1]))
    return [path for _, path in ranked_rows[:LIMIT]]


def emit() -> int:
    home, history, touch, births = locations()
    json.dump({"paths": ranked(home, history, touch, births)}, sys.stdout, separators=(",", ":"))
    sys.stdout.write("\n")
    return 0


def _libc():
    name = ctypes.util.find_library("c")
    lib = ctypes.CDLL(name or "libc.so.6", use_errno=True)
    lib.inotify_init1.argtypes = [ctypes.c_int]
    lib.inotify_init1.restype = ctypes.c_int
    lib.inotify_add_watch.argtypes = [ctypes.c_int, ctypes.c_char_p, ctypes.c_uint32]
    lib.inotify_add_watch.restype = ctypes.c_int
    return lib


def _ignored_comm(comm: str) -> bool:
    if comm in IGNORE_COMM:
        return True
    return any(comm.startswith(prefix) for prefix in IGNORE_COMM_PREFIXES)


def foreign_open(path: str) -> bool:
    """True when some program other than Flea or a thumbnailer currently has the file open."""
    try:
        wanted = os.path.realpath(path)
    except OSError:
        wanted = path
    me = str(os.getpid())
    try:
        pids = os.listdir("/proc")
    except OSError:
        return False
    for pid in pids:
        if not pid.isdigit() or pid == me:
            continue
        try:
            comm = Path(os.path.join("/proc", pid, "comm")).read_text(encoding="utf-8", errors="replace").strip()
        except OSError:
            continue
        if _ignored_comm(comm):
            continue
        fd_dir = os.path.join("/proc", pid, "fd")
        try:
            fds = os.listdir(fd_dir)
        except OSError:
            continue
        for fd in fds:
            try:
                linked = os.readlink(os.path.join(fd_dir, fd))
            except OSError:
                continue
            if not linked.startswith("/"):
                continue
            live = linked.split(" (deleted)", 1)[0]
            try:
                same = os.path.realpath(live) == wanted
            except OSError:
                same = live == wanted or live == path
            if same:
                return True
    return False


def record_touches(touch_path: str, updates: dict[str, float]) -> None:
    if not updates:
        return
    current = load_touches(touch_path)
    changed = False
    for path, when in updates.items():
        if when > current.get(path, 0.0) + TOUCH_GAP_S:
            current[path] = when
            changed = True
    if changed:
        save_touches(touch_path, current)


def watch_births(home: str, touch_path: str, births: list[str], stop: threading.Event, ready: threading.Event | None = None) -> None:
    """Record a touch when a birth folder gains a file, or another program opens one."""
    directory = os.path.dirname(touch_path)
    os.makedirs(directory, mode=0o700, exist_ok=True)
    lock_fd = os.open(touch_path + ".lock", os.O_CREAT | os.O_RDWR, 0o600)
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX)
        lib = _libc()
        fd = lib.inotify_init1(IN_NONBLOCK)
        if fd < 0:
            return
        watches: dict[int, str] = {}

        def add_watch(path: str) -> None:
            if len(watches) >= BIRTH_DIR_CAP or not keep_dir(os.path.basename(path)) and path not in births:
                return
            try:
                wd = lib.inotify_add_watch(fd, os.fsencode(path), WATCH_MASK)
            except OSError:
                return
            if wd >= 0:
                watches[wd] = path

        for birth in births:
            real = _real(birth)
            if not real or real == _real(home) or not os.path.isdir(real):
                continue
            for dirpath, dirnames, _names in os.walk(real, followlinks=False):
                if len(watches) >= BIRTH_DIR_CAP:
                    break
                dirnames[:] = [name for name in dirnames if keep_dir(name)]
                add_watch(dirpath)
        if ready is not None:
            ready.set()
        definite: dict[str, float] = {}
        opened: dict[str, float] = {}
        while not stop.is_set():
            import select

            readable, _, _ = select.select([fd], [], [], 0.2)
            if readable:
                try:
                    data = os.read(fd, 65536)
                except BlockingIOError:
                    data = b""
                offset = 0
                while offset + EVENT_HEAD.size <= len(data):
                    wd, mask, _cookie, name_len = EVENT_HEAD.unpack_from(data, offset)
                    offset += EVENT_HEAD.size
                    raw_name = data[offset:offset + name_len].split(b"\x00", 1)[0]
                    offset += name_len
                    if mask & IN_Q_OVERFLOW:
                        continue
                    folder = watches.get(wd, "")
                    if mask & IN_IGNORED:
                        watches.pop(wd, None)
                        continue
                    if not folder:
                        continue
                    name = raw_name.decode("utf-8", "replace")
                    if not name:
                        continue
                    path = os.path.join(folder, name)
                    if mask & IN_ISDIR:
                        if mask & (IN_CREATE | IN_MOVED_TO) and keep_dir(name):
                            add_watch(path)
                        continue
                    if not keep_file(name):
                        continue
                    now = time.time()
                    if mask & (IN_CREATE | IN_MOVED_TO | IN_CLOSE_WRITE):
                        definite[path] = now
                    elif mask & IN_OPEN:
                        opened[path] = now
            due: dict[str, float] = {}
            due.update(definite)
            definite.clear()
            for path, when in list(opened.items()):
                if foreign_open(path):
                    due[path] = when
                    opened.pop(path, None)
                elif time.time() - when > 1.0:
                    opened.pop(path, None)
            if due:
                record_touches(touch_path, due)
        os.close(fd)
    finally:
        if ready is not None:
            ready.set()
        os.close(lock_fd)


def watch_main() -> int:
    home, _history, touch, births = locations()
    stop = threading.Event()

    def request_stop(_signum, _frame):
        stop.set()

    signal.signal(signal.SIGTERM, request_stop)
    signal.signal(signal.SIGINT, request_stop)
    watch_births(home, touch, births, stop)
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
        paths = ranked(home, history, touch, [downloads])
        assert newer in paths and older in paths and external in paths and grabbed in paths and spaced in paths, paths
        assert folder not in paths and missing not in paths and linked_dir not in paths, paths
        assert paths.index(newer) < paths.index(older), paths
        assert paths.index(grabbed) < paths.index(external), paths
        record_touch(touch, older)
        paths = ranked(home, history, touch, [downloads])
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
        capped = ranked(home, history_many, fresh_touch, [])
        assert len(capped) == LIMIT, len(capped)
        assert capped[0].endswith("f119.txt"), capped[0]
        # A birth folder that is $HOME itself must not list the whole home.
        os.environ["THE_STACK_DOWNLOADS"] = home
        os.environ["THE_STACK_HISTORY"] = os.path.join(root, "empty.xbel")
        Path(os.environ["THE_STACK_HISTORY"]).write_text("<xbel version='1.0'/>")
        assert ranked(home, os.environ["THE_STACK_HISTORY"], os.path.join(root, "no-touch.json"), [home]) == []
        pictures = os.path.join(home, "Pictures")
        shot = os.path.join(pictures, "Screenshots", "shot.png")
        noise = os.path.join(pictures, "node_modules", "built.js")
        partial = os.path.join(pictures, "fetch.crdownload")
        os.makedirs(os.path.dirname(shot))
        os.makedirs(os.path.dirname(noise))
        Path(shot).write_text("png")
        Path(noise).write_text("js")
        Path(partial).write_text("part")
        born = ranked(home, os.environ["THE_STACK_HISTORY"], os.path.join(root, "born-touch.json"), [pictures])
        assert shot in born and noise not in born and partial not in born, born
        assert born[0] == shot, born
        watch_touch = os.path.join(root, "watch-touch.json")
        stop = threading.Event()
        ready = threading.Event()
        watcher = threading.Thread(
            target=watch_births, args=(home, watch_touch, [pictures], stop, ready), daemon=True
        )
        watcher.start()
        assert ready.wait(2), "watcher did not arm"
        arrived = os.path.join(pictures, "arrived.txt")
        Path(arrived).write_text("now")
        deadline = time.time() + 3
        while time.time() < deadline and arrived not in load_touches(watch_touch):
            time.sleep(0.05)
        assert arrived in load_touches(watch_touch), load_touches(watch_touch)
        time.sleep(TOUCH_GAP_S + 0.1)
        before = load_touches(watch_touch)[arrived]
        opener = __import__("subprocess").Popen(
            [sys.executable, "-c", "f=open(r'%s','rb'); f.read(); import time; time.sleep(2)" % arrived]
        )
        deadline = time.time() + 3
        while time.time() < deadline and load_touches(watch_touch).get(arrived, 0) <= before + 0.05:
            time.sleep(0.05)
        opener.kill()
        opener.wait(timeout=2)
        assert load_touches(watch_touch).get(arrived, 0) > before + 0.05, load_touches(watch_touch)
        stop.set()
        watcher.join(2)
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
    if argv[1:] == ["--watch"]:
        return watch_main()
    if len(argv) == 3 and argv[1] == "--touch":
        _, _, touch, _ = locations()
        record_touch(touch, argv[2])
        return 0
    sys.stderr.write("usage: the-stack.py [--touch PATH | --watch]\n")
    return 2


if __name__ == "__main__":
    try:
        raise SystemExit(main(sys.argv))
    except BrokenPipeError:
        raise SystemExit(0)
