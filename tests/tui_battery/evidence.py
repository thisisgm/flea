"""Owned paths, command receipts and content identities."""
import hashlib
import json
import os
from pathlib import Path
import stat
import subprocess
import time

MARKER = ".flea-test-sandbox"


class Blocked(RuntimeError):
    pass


def digest(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def save(path, data):
    with Path(path).open("x", encoding="utf-8") as output:
        json.dump(data, output, ensure_ascii=False, indent=2)
        output.write("\n")


def absolute(path):
    path = Path(path)
    if not path.is_absolute() or ".." in path.parts:
        raise Blocked("paths must be absolute and contain no parent traversal")
    for part in (path, *path.parents):
        if part.is_symlink():
            raise Blocked(f"symlink in owned path: {part}")
    return path


class Owned:
    def __init__(self, path):
        self.path = absolute(path)
        marker = self.path / MARKER
        if not marker.is_file() or marker.is_symlink() or not marker.read_text().strip():
            raise Blocked(f"missing nonempty sandbox marker: {marker}")

    def child(self, relative):
        relative = Path(relative)
        if relative.is_absolute() or ".." in relative.parts or str(relative) in ("", "."):
            raise Blocked("owned child must be a nonempty relative path")
        target = absolute(self.path / relative)
        if not target.is_relative_to(self.path) or target == self.path:
            raise Blocked("owned child escaped sandbox")
        return target

    def directory(self, relative):
        target = self.child(relative)
        target.mkdir(parents=True, exist_ok=False)
        return target


def output_root(path):
    path = absolute(path)
    if path.exists():
        raise Blocked("output must be new; previous receipts are never overwritten")
    parents = [p for p in path.parents if (p / MARKER).is_file()]
    if not parents:
        raise Blocked("output must be beneath a marked mktemp root")
    owner = Owned(parents[0])
    owner.child(path.relative_to(owner.path))
    path.mkdir(parents=True)
    (path / MARKER).write_text(f"battery {os.getpid()} {time.time_ns()}\n")
    return Owned(path)


class Commands:
    def __init__(self):
        self.records = []

    def run(self, argv, timeout=10, check=True, **kwargs):
        argv = [str(a) for a in argv]
        receipt = {"argv": argv, "start_ns": time.time_ns()}
        self.records.append(receipt)
        try:
            result = subprocess.run(argv, capture_output=True, timeout=timeout, **kwargs)
        except subprocess.TimeoutExpired as error:
            receipt.update(exit_code=None, reason="timeout", stderr=(error.stderr or b"").decode(errors="replace"))
            raise Blocked(f"command timed out: {argv[0]}") from error
        except OSError as error:
            receipt.update(exit_code=None, reason=str(error))
            raise Blocked(f"command could not start: {argv[0]}: {error}") from error
        receipt.update(exit_code=result.returncode, end_ns=time.time_ns(),
                       stdout=result.stdout.decode(errors="replace"), stderr=result.stderr.decode(errors="replace"))
        if check and result.returncode:
            raise Blocked(f"command exited {result.returncode}: {argv[0]}: {receipt['stderr'][:300]}")
        return result


def tree_identity(root):
    rows = []
    for path in sorted(Path(root).rglob("*")):
        info = path.lstat()
        row = {"path": str(path.relative_to(root)), "mode": stat.S_IMODE(info.st_mode),
               "mtime_ns": info.st_mtime_ns, "size": info.st_size, "uid": info.st_uid, "gid": info.st_gid}
        if path.is_symlink():
            row["link"] = os.readlink(path)
        elif path.is_file():
            row["sha256"] = digest(path)
        elif not path.is_dir():
            raise Blocked(f"nonregular fixture entry: {path}")
        rows.append(row)
    return rows


def source_identity(repo, binary, boards, commands):
    head = commands.run(["git", "-C", repo, "rev-parse", "HEAD"]).stdout.strip().decode()
    patch = commands.run(["git", "-C", repo, "diff", "--binary", "HEAD"]).stdout
    untracked = commands.run(["git", "-C", repo, "ls-files", "--others", "--exclude-standard", "-z"]).stdout
    files = {os.fsdecode(p): digest(Path(repo) / os.fsdecode(p)) for p in untracked.split(b"\0") if p}
    refs = {str(p.relative_to(boards)): digest(p) for p in sorted(Path(boards).glob("boards/tui-*.*"))}
    for name in ("decisions.json", "specs/TUI-SPEC.md", "specs/OFFICE-PRINT-SPEC.md", "source/tui_icons.py", "source/tui_capture.py", "source/boards_tui.py"):
        p = Path(boards) / name
        if p.is_file():
            refs[name] = digest(p)
    for surface in ("filter-close", "search-contents", "jobs", "open-refused"):
        for suffix in ("html", "png"):
            p = Path(boards) / "boards" / f"{surface}.{suffix}"
            if p.is_file():
                refs[str(p.relative_to(boards))] = digest(p)
    return {"commit": head, "dirty_patch_sha256": hashlib.sha256(patch).hexdigest(), "untracked": files,
            "binary": str(binary), "elf_sha256": digest(binary), "references": refs,
            "locale": os.environ.get("LANG"), "TZ": os.environ.get("TZ"), "uid": os.getuid(), "gid": os.getgid()}
