"""Verified fixture snapshots mounted at /fx only inside the candidate namespace."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat

from .evidence import Blocked, Owned, absolute, digest, save, tree_identity

WORK = ["A", "Code", "Docs", "Empty", "Locked", "Many", "Media", "notes.txt", "todo.md"]
OFFICE_BYTES = {"budget.xlsx": 7970, "garden-report.docx": 8025, "garden-report.pdf": 92286,
                "lease.docx": 8192, "notes.txt": 99, "readme.md": 48, "talk.odp": 19532}
NOTES = b"Visual gate notes\n\nThis file is the fixture's plain text sample.\nLine four keeps the preview busy.\n"


def regular_bytes(path, limit=64 * 1024 * 1024):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise Blocked(f"fixture is not a bounded regular file: {path}")
        with os.fdopen(fd, "rb", closefd=False) as source:
            data = source.read(limit + 1)
        if len(data) > limit:
            raise Blocked("fixture grew beyond its size budget")
        return data
    finally:
        os.close(fd)


def relative(path):
    p = Path(path)
    if p.is_absolute() or ".." in p.parts or str(p) in ("", "."):
        raise Blocked("fixture path must be relative and contained")
    return p


def import_family(source, family, target):
    source = absolute(source)
    manifest = json.loads(regular_bytes(source / "manifest.json"))
    if family not in manifest.get("families", {}):
        raise Blocked(f"fixture manifest lacks family: {family}")
    spec = manifest["families"][family]
    base = absolute(source / relative(spec["root"]))
    entries = spec["entries"]
    if len(entries) > 4096:
        raise Blocked("fixture manifest exceeds entry budget")
    seen, directories, hashes, total = set(), [], [], 0
    for row in sorted(entries, key=lambda r: len(Path(r["path"]).parts)):
        name = relative(row["path"])
        if str(name) in seen:
            raise Blocked("duplicate fixture manifest entry")
        seen.add(str(name))
        destination = target / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        if any(p.is_symlink() for p in (destination, *destination.parents)):
            raise Blocked("fixture destination crosses a symlink")
        mode = row["mode"]
        if not isinstance(mode, int) or mode & ~0o777:
            raise Blocked("fixture mode must exclude special bits")
        if row["kind"] == "directory":
            destination.mkdir(exist_ok=True)
            directories.append((destination, row))
        elif row["kind"] == "symlink":
            link = row["target"]
            resolved = (destination.parent / link).resolve()
            if Path(link).is_absolute() or not resolved.is_relative_to(target):
                raise Blocked("fixture symlink escapes its snapshot")
            destination.symlink_to(link)
            os.utime(destination, ns=(row["mtime_ns"], row["mtime_ns"]), follow_symlinks=False)
        elif row["kind"] == "file":
            origin = absolute(base / name)
            data = regular_bytes(origin)
            total += len(data)
            if total > 256 * 1024 * 1024:
                raise Blocked("fixture family exceeds the 256 MiB byte budget")
            sha = hashlib.sha256(data).hexdigest()
            if len(data) != row["size"] or sha != row["sha256"]:
                raise Blocked(f"fixture source identity mismatch: {family}/{name}")
            with destination.open("xb") as output:
                output.write(data)
            destination.chmod(mode)
            os.utime(destination, ns=(row["mtime_ns"], row["mtime_ns"]))
            hashes.append({"path": str(name), "sha256": sha})
        else:
            raise Blocked("fixture manifest contains an unsupported entry kind")
    names = {p.name for p in target.iterdir()}
    if not set(WORK).issubset(names):
        raise Blocked("fixture lacks the nine original work entries")
    if family == "work" and names != set(WORK):
        raise Blocked("original work fixture must contain exactly nine entries")
    if family == "office":
        docs = target / "Docs"
        if {p.name for p in docs.iterdir()} != set(OFFICE_BYTES) | {"report.pdf"}:
            raise Blocked("office Docs must contain exactly eight approved files")
        if any((docs / name).stat().st_size != size for name, size in OFFICE_BYTES.items()):
            raise Blocked("office document sizes disagree with reference metadata")
    elif family == "work" and len(list((target / "Docs").iterdir())) != 3:
        raise Blocked("original Docs must contain three entries, separate from office")
    receipt = tree_identity(target)
    for path, row in reversed(directories):
        path.chmod(row["mode"])
        os.utime(path, ns=(row["mtime_ns"], row["mtime_ns"]))
    return {"manifest_sha256": digest(source / "manifest.json"), "family": family,
            "entries": entries, "copied": hashes, "actual_before_directory_modes": receipt,
            "features": spec.get("features", []), "auxiliary": spec.get("auxiliary", []),
            "representations": spec.get("representations", {})}


def inventory(source, states):
    """Verifies, without copying, every family the selected states name: entry bytes, auxiliary bytes and declared features."""
    source = absolute(source)
    manifest = json.loads(regular_bytes(source / "manifest.json"))
    report = {}
    for family in sorted({state["family"] for state in states}):
        named = sorted(state["id"] for state in states if state["family"] == family)
        spec = manifest.get("families", {}).get(family)
        if spec is None:
            report[family] = {"status": "missing", "states": named}
            continue
        try:
            base = absolute(source / relative(spec["root"]))
            rows = [(base / relative(row["path"]), row) for row in spec["entries"] if row["kind"] == "file"]
            rows += [(absolute(source / relative(row["source"])), row) for row in spec.get("auxiliary", [])]
            for path, row in rows:
                data = regular_bytes(path)
                if len(data) != row["size"] or hashlib.sha256(data).hexdigest() != row["sha256"]:
                    raise Blocked(f"fixture source identity mismatch: {family}/{row['path']}")
            supplied = set(spec.get("features", [])) | {"graphics-negative", "model-source"}
            wanted = {need for state in states if state["family"] == family for need in state["requires"]}
            report[family] = {"status": "present", "states": named, "entries": len(spec["entries"]),
                              "auxiliary": len(spec.get("auxiliary", [])), "features": spec.get("features", []),
                              "representations": sorted(spec.get("representations", {})),
                              "requires_not_supplied": sorted(wanted - supplied)}
        except (Blocked, OSError, KeyError, TypeError) as error:
            report[family] = {"status": "invalid", "states": named, "reason": str(error)}
    return report


def seed_auxiliary(source, fx, receipt):
    copied = []
    for row in receipt.get("auxiliary", []):
        name = relative(row["path"])
        if name.parts[0] not in ("tools", "config", "data", "state", "home"):
            raise Blocked("auxiliary fixture may only seed private tools, config, data, state or home")
        destination = fx / name
        absolute(destination)
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.exists() or destination.is_symlink():
            raise Blocked("auxiliary fixture would replace an existing entry")
        data = regular_bytes(absolute(Path(source) / relative(row["source"])))
        sha = hashlib.sha256(data).hexdigest()
        if sha != row["sha256"] or len(data) != row["size"]:
            raise Blocked("auxiliary fixture source identity mismatch")
        with destination.open("xb") as output:
            output.write(data)
        mode = row.get("mode", 0o600)
        if not isinstance(mode, int) or mode & ~0o777:
            raise Blocked("auxiliary fixture has unsafe mode bits")
        destination.chmod(mode)
        copied.append({"path": str(name), "sha256": sha})
    receipt["auxiliary_copied"] = copied


def build(case, family, source=None, dry=False):
    owner = Owned(case)
    fx = owner.directory("fx")
    work = fx / "v/work"
    work.mkdir(parents=True)
    for name in ("home", "config", "data", "state", "cache", "runtime", "tmp", "tools"):
        (fx / name).mkdir(mode=0o700)
    if source:
        receipt = import_family(source, family, work)
        seed_auxiliary(source, fx, receipt)
    elif dry:
        for name in WORK[:-2]:
            (work / name).mkdir()
        (work / "notes.txt").write_bytes(NOTES)
        (work / "todo.md").write_bytes(b"fixture markdown\n")
        for i in range(1, 81):
            (work / "Many" / f"item-{i:02d}.txt").write_bytes(b"item X\n")
        receipt = {"family": family, "synthetic": True, "entries": tree_identity(work)}
    else:
        raise Blocked("approved fixture bytes and metadata manifest required via --fixtures")
    save(case / "fixtures.json", receipt)
    return fx, receipt


def namespace(binary, fx, cwd, environment):
    if not shutil.which("bwrap"):
        raise Blocked("bwrap is required for the isolated /fx namespace")
    argv = ["bwrap", "--die-with-parent", "--unshare-all", "--tmpfs", "/"]
    for path in ("/usr", "/bin", "/lib", "/lib64", "/etc"):
        if Path(path).exists():
            argv += ["--ro-bind", path, path]
    argv += ["--proc", "/proc", "--dev", "/dev", "--ro-bind", "/sys", "/sys",
             "--dir", "/opt", "--ro-bind", str(binary), "/opt/flea", "--bind", str(fx), "/fx",
             "--bind", str(fx / "tmp"), "/tmp", "--clearenv"]
    env = {"PATH": "/fx/tools:/usr/bin:/bin", "HOME": "/fx/home", "XDG_CONFIG_HOME": "/fx/config",
           "XDG_DATA_HOME": "/fx/data", "XDG_STATE_HOME": "/fx/state", "XDG_CACHE_HOME": "/fx/cache",
           "XDG_RUNTIME_DIR": "/fx/runtime", "TMPDIR": "/fx/tmp", "LANG": "C.UTF-8", "TZ": "UTC"}
    env.update(environment)
    for key, value in env.items():
        argv += ["--setenv", key, value]
    return argv + ["--chdir", cwd, "/opt/flea", "--tui", cwd]
