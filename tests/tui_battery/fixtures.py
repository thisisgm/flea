"""Verified fixture snapshots mounted at /fx only inside the candidate namespace."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import tempfile

from .evidence import MARKER, Blocked, Owned, absolute, digest, save, tree_identity

WORK = ["A", "Code", "Docs", "Empty", "Locked", "Many", "Media", "notes.txt", "todo.md"]
OFFICE_BYTES = {"budget.xlsx": 7970, "garden-report.docx": 8025, "garden-report.pdf": 92286,
                "lease.docx": 8192, "notes.txt": 99, "readme.md": 48, "talk.odp": 19532}
# Features no family declares: the relay itself implements graphics-negative, and the 3D states' own files are model-source.
BUILT_IN_FEATURES = frozenset({"graphics-negative", "model-source"})
NOTES = b"Visual gate notes\n\nThis file is the fixture's plain text sample.\nLine four keeps the preview busy.\n"
# Only a tmpfs fixture draws the boards' digits, because its folders report 40 bytes plus 20 per entry.
TMPFS = Path("/dev/shm")
TMPFS_EMPTY_FOLDER_BYTES = 40
FONT_CACHE = "/var/cache/fontconfig"


def regular_bytes(path, limit=64 * 1024 * 1024):
    fd = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise Blocked(f"fixture is not a bounded regular file: {path}")
        # A read of n bytes allocates n up front, so ask for the checked size, never the whole budget.
        with os.fdopen(fd, "rb", closefd=False) as source:
            data = source.read(info.st_size + 1)
        if len(data) > info.st_size:
            raise Blocked("fixture grew while it was read")
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
    try:
        manifest = json.loads(regular_bytes(source / "manifest.json"))
        families = manifest.get("families", {}) if isinstance(manifest, dict) else None
        if not isinstance(families, dict):
            raise Blocked("fixture manifest or its families is not an object")
    except (OSError, ValueError) as error:
        raise Blocked(f"fixture manifest is unreadable: {error}") from error
    report = {}
    for family in sorted({state["family"] for state in states}):
        named = sorted(state["id"] for state in states if state["family"] == family)
        spec = families.get(family)
        if spec is None:
            report[family] = {"status": "missing", "states": named}
            continue
        try:
            if not isinstance(spec, dict):
                raise Blocked(f"fixture family {family} is not an object")
            base = absolute(source / relative(spec["root"]))
            rows = [(base / relative(row["path"]), row["path"], row) for row in spec["entries"] if row["kind"] == "file"]
            rows += [(absolute(source / relative(row["source"])), row["source"], row) for row in spec.get("auxiliary", [])]
            for path, name, row in rows:
                data = regular_bytes(path)
                if len(data) != row["size"]:
                    raise Blocked(f"fixture source identity mismatch: {family}/{name}: size {len(data)} is not {row['size']}")
                if hashlib.sha256(data).hexdigest() != row["sha256"]:
                    raise Blocked(f"fixture source identity mismatch: {family}/{name}: sha256 differs from the manifest")
            supplied = set(spec.get("features", [])) | BUILT_IN_FEATURES
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


def unlock(top):
    """Opens every directory below top to its owner and returns the modes it replaced, by path relative to top."""
    modes = {}
    for path, directories, _ in os.walk(top):
        for name in directories:
            child = Path(path) / name
            if not child.is_symlink():
                modes[child.relative_to(top)] = stat.S_IMODE(child.lstat().st_mode)
                child.chmod(0o700)
    return modes


def discard(root):
    Owned(root)
    unlock(root)
    shutil.rmtree(root)


def build(case, family, source=None, dry=False, tmpfs=TMPFS):
    """The case's fixture, built in its own marked root on tmpfs; retain copies it into the case once the terminal is closed."""
    Owned(case)
    root = Path(tempfile.mkdtemp(prefix="flea-battery-fx.", dir=tmpfs))
    (root / MARKER).write_text(f"battery fixture {os.getpid()}\n")
    try:
        fx = Owned(root).directory("fx")
        folder = fx.lstat().st_size
        if folder != TMPFS_EMPTY_FOLDER_BYTES:
            raise Blocked(f"fixture root {root} reports {folder} bytes for an empty folder, not tmpfs's "
                          f"{TMPFS_EMPTY_FOLDER_BYTES}, so its folder totals would not be the boards'")
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
    except BaseException:
        discard(root)
        raise
    return fx, receipt


def specials(directory, names):
    """Sockets, pipes and devices a run left in its fixture; a copy would block on a pipe and cannot recreate a socket."""
    return [name for name in names if not stat.S_ISREG(mode := os.lstat(os.path.join(directory, name)).st_mode)
            and not stat.S_ISDIR(mode) and not stat.S_ISLNK(mode)]


def retain(fx, case):
    """Copies the case's tmpfs fixture to case/fx, where its sealed receipts and reconciliation read it, then frees the tmpfs."""
    root = Owned(fx.parent).path
    modes = unlock(fx)
    target = Owned(case).child("fx")
    try:
        shutil.copytree(fx, target, symlinks=True, ignore=specials)
        # Deepest first, so a closed folder such as Locked closes only after everything below it is copied.
        for relative, mode in sorted(modes.items(), key=lambda item: len(item[0].parts), reverse=True):
            (target / relative).chmod(mode)
    except OSError as error:
        raise Blocked(f"copy into {case} failed; the tmpfs copy stays at {root}: {error}") from error
    discard(root)


def no_chafa(fx, search=(Path("/usr/bin"), Path("/bin"))):
    """Expose the installed tool set through a private resolver that has no chafa entry."""
    Owned(fx.parent).child(fx.name)
    target = absolute(fx / "tools-no-chafa")
    target.mkdir()
    entries = {}
    for directory in (fx / "tools", *search):
        if not directory.is_dir():
            continue
        for path in sorted(directory.iterdir()):
            if path.name != "chafa" and path.is_file() and os.access(path, os.X_OK) and path.name not in entries:
                source = "/fx/tools/" + path.name if directory == fx / "tools" else str(path)
                (target / path.name).symlink_to(source)
                entries[path.name] = source
    return {"policy": "no-chafa", "PATH": "/fx/tools-no-chafa", "entries": entries,
            "chafa_present": (target / "chafa").exists() or (target / "chafa").is_symlink()}


def namespace(binary, fx, cwd, environment, tool_policy=None, cups_server=None, arguments=None):
    if not shutil.which("bwrap"):
        raise Blocked("bwrap is required for the isolated /fx namespace")
    argv = ["bwrap", "--die-with-parent", "--unshare-all", "--tmpfs", "/"]
    for path in ("/usr", "/bin", "/lib", "/lib64", "/etc"):
        if Path(path).exists():
            argv += ["--ro-bind", path, path]
    # Without the system font cache, fc-list outlasts Flea's 150 ms icon probe (src/tui/icons.rs) and draws text marks.
    if os.path.isdir(FONT_CACHE):
        argv += ["--ro-bind", FONT_CACHE, FONT_CACHE]
    argv += ["--proc", "/proc", "--dev", "/dev", "--ro-bind", "/sys", "/sys",
             "--dir", "/opt", "--ro-bind", str(binary), "/opt/flea", "--bind", str(fx), "/fx",
             "--bind", str(fx / "tmp"), "/tmp", "--clearenv"]
    env = {"PATH": "/fx/tools:/usr/bin:/bin", "HOME": "/fx/home", "XDG_CONFIG_HOME": "/fx/config",
           "XDG_DATA_HOME": "/fx/data", "XDG_STATE_HOME": "/fx/state", "XDG_CACHE_HOME": "/fx/cache",
           "XDG_RUNTIME_DIR": "/fx/runtime", "TMPDIR": "/fx/tmp", "LANG": "C.UTF-8", "TZ": "UTC"}
    env.update(environment)
    if cups_server is not None:
        if cups_server != "/fx/runtime/cups.sock" or not (fx / "runtime/cups.sock").is_socket():
            raise Blocked("CUPS fixture must use its actual private Unix socket")
        env["CUPS_SERVER"] = cups_server
    if tool_policy:
        if tool_policy != "no-chafa":
            raise Blocked("unknown private tool policy: " + str(tool_policy))
        policy = no_chafa(fx)
        save(fx / "data/tool-policy.json", policy)
        env["PATH"] = policy["PATH"]
    for key, value in env.items():
        argv += ["--setenv", key, value]
    return argv + ["--chdir", cwd, "/opt/flea"] + (["--tui", cwd] if arguments is None else list(arguments))
