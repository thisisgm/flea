"""Private, marked fixtures and executable protocol doubles for the UISH driver."""
import base64
import hashlib
import json
import os
from pathlib import Path
import selectors
import shutil
import stat
import subprocess
import sys
import tempfile
import time

from flea_native_inputs import seed

UNIT = "m9-uish-03"
MARKER = ".flea-test-sandbox"
EPOCH = 1_700_000_000
MAX_BYTES = 65_536


class Refused(RuntimeError):
    """An ownership or protocol boundary refused an action."""


class FixturePremiseError(RuntimeError):
    """A required input is unavailable; no product assertion has run."""


def regular_bytes(path, limit=MAX_BYTES):
    descriptor = os.open(path, os.O_RDONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
    with os.fdopen(descriptor, "rb") as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):
            raise Refused(f"not a regular file: {path}")
        data = stream.read(limit + 1)
    if len(data) > limit:
        raise Refused(f"fixture input exceeds {limit} bytes: {path}")
    return data


def digest(path):
    return hashlib.sha256(regular_bytes(path, 16 * 1024 * 1024)).hexdigest()


def absolute(path):
    if not str(path) or not Path(path).is_absolute() or ".." in Path(path).parts:
        raise Refused("path must be nonempty, absolute and without parent traversal")
    return Path(path)


def workspace_root(path):
    workspace = absolute(path).resolve(strict=True)
    if workspace != Path(__file__).resolve().parents[2]:
        raise Refused("fixture workspace must be the helper's own worktree")
    return workspace


class FixtureRoot:
    """Only roots created here, underneath this worktree's unit directory, are mutable."""

    @classmethod
    def create(cls, workspace):
        workspace = workspace_root(workspace)
        parent = workspace / ".superpowers" / UNIT
        if parent.resolve() != parent:
            raise Refused("fixture parent is a symlink")
        parent.mkdir(parents=True, exist_ok=True)
        root = Path(tempfile.mkdtemp(prefix="fixtures-", dir=parent))
        token = os.urandom(24).hex()
        (root / MARKER).write_text(json.dumps({"unit": UNIT, "token": token}))
        return cls(workspace, root, token)

    def __init__(self, workspace, root, token):
        self.workspace = workspace_root(workspace)
        self.root = absolute(root)
        self.token = token
        self.children = {}
        self.validate()
        self.inode = self.root.stat().st_ino

    def validate(self):
        parent = self.workspace / ".superpowers" / UNIT
        if self.root.parent != parent or self.root.resolve() != self.root:
            raise Refused("fixture root escaped its worktree unit directory")
        if not self.root.name.startswith("fixtures-"):
            raise Refused("foreign fixture root")
        try:
            marker = json.loads(regular_bytes(self.root / MARKER))
        except (OSError, ValueError) as error:
            raise Refused("missing or invalid fixture marker") from error
        if marker != {"unit": UNIT, "token": self.token}:
            raise Refused("missing or foreign fixture marker")
        if hasattr(self, "inode") and self.root.stat().st_ino != self.inode:
            raise Refused("fixture root was replaced")

    def path(self, relative):
        self.validate()
        relative = Path(relative)
        if relative.is_absolute() or not relative.parts or ".." in relative.parts:
            raise Refused("fixture child must be a nonempty relative path")
        target = self.root / relative
        if not target.resolve().is_relative_to(self.root) or target.resolve() == self.root:
            raise Refused("fixture child escaped its marked root")
        return target

    def directory(self, relative):
        target = self.path(relative)
        target.mkdir(parents=True, exist_ok=True, mode=0o700)
        return target

    def write(self, relative, content, executable=False):
        target = self.path(relative)
        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        data = content.encode() if isinstance(content, str) else content
        descriptor = os.open(target, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(descriptor, "wb") as stream:
            stream.write(data)
        target.chmod(0o700 if executable else 0o600)
        os.utime(target, (EPOCH, EPOCH))
        return target

    def remove(self, path):
        """Reuse the shell guard's canonical/marked checks within the explicitly granted W."""
        target = absolute(path)
        self.validate()
        if target != self.root:
            if not target.is_relative_to(self.root):
                raise Refused("cleanup escaped owned root")
            self.path(target.relative_to(self.root))
        if self.children:
            raise Refused("drain owned processes before cleanup")
        guard = self.workspace / "tools/flea-sandbox-guard"
        script = '''. "$1"
sandbox_plausible "$3" || sandbox_refuse "invalid cleanup path"
p=$(sandbox_canon "$3") || sandbox_refuse "unresolvable cleanup path"
[ "$p" = "$3" ] || sandbox_refuse "cleanup path is a symlink alias"
sandbox_under "$p" "$2" || sandbox_refuse "cleanup escaped owned root"
sandbox_owned "$2" || sandbox_refuse "missing marker"
'''
        result = subprocess.run(["/bin/bash", "-c", script, "uish-guard", str(guard),
                                 str(self.root), str(target)], capture_output=True,
                                env={"HOME": str(self.workspace), "PATH": "/usr/bin:/bin"}, timeout=5)
        if result.returncode:
            raise Refused(result.stderr.decode().strip())
        if target.is_dir():
            shutil.rmtree(target)
        else:
            target.unlink()

    def environment(self):
        paths = {"HOME": "home", "XDG_CONFIG_HOME": "home/.config", "XDG_DATA_HOME": "data",
                 "XDG_STATE_HOME": "state", "XDG_CACHE_HOME": "c#a?che", "XDG_RUNTIME_DIR": "runtime",
                 "FLEA_UNDO_DIR": "undo", "TMPDIR": "tmp"}
        result = {key: str(self.directory(value)) for key, value in paths.items()}
        result.update(PATH=f"{self.directory('bin')}:/usr/bin:/bin", LANG="C.UTF-8",
                      LC_ALL="C.UTF-8", PYTHONDONTWRITEBYTECODE="1")
        self.directory("c#a?che/thumbnails/large")
        self.directory("c#a?che/thumbnails/fail")
        return result

    def spawn(self, argv, **kwargs):
        if "env" in kwargs or "cwd" in kwargs:
            raise Refused("fixture children use only the private environment and working directory")
        child = subprocess.Popen(argv, cwd=self.root, env=self.environment(), **kwargs)
        self.children[child.pid] = child
        return child

    def stop(self, pid):
        child = self.children.get(pid)
        if child is None or child.pid != pid:
            raise Refused(f"foreign PID: {pid}")
        if child.poll() is None:
            child.terminate()
            try:
                child.wait(timeout=2)
            except subprocess.TimeoutExpired:
                child.kill()
                child.wait(timeout=2)
        del self.children[pid]
        return child.returncode

    def presentation(self, source_home=None):
        environment = self.environment()
        if source_home is not None:
            destinations = []
            for name in ("theme/colors.toml", "theme/shell.toml", "theme/icons.theme", "theme.name"):
                destinations.append(self.path("home/.local/state/omarchy/current/" + name))
            for name in ("omarchy/shell.toml", "fontconfig/fonts.conf", "fontconfig/conf.d/50-omarchy-input-method.conf"):
                destinations.append(self.path("home/.config/" + name))
            if any(path.exists() or path.is_symlink() for path in destinations):
                raise Refused("presentation inputs must be copied into new files")
            seed(absolute(source_home), Path(environment["HOME"]), Path(environment["XDG_CONFIG_HOME"]))
            description = {"kind": "copied-inputs", "source": str(source_home), "native_fidelity": False}
        else:
            self.write("home/.local/state/omarchy/current/theme/colors.toml",
                       'background = "#181818"\nforeground = "#eeeeee"\naccent = "#88aaff"\n')
            self.write("home/.local/state/omarchy/current/theme/shell.toml", "base-size = 14\n")
            self.write("home/.local/state/omarchy/current/theme.name", "uish-fixture-palette\n")
            self.write("home/.config/omarchy/shell.toml", "base-size = 14\n")
            description = {"kind": "declared-fixture-palette", "name": "uish-fixture-palette",
                           "font": "platform-default", "icons": "platform-default", "operator_theme": False}
        self.write("presentation.json", json.dumps(description, sort_keys=True))
        return description

    def inventory(self, relatives):
        return {str(name): {"sha256": digest(self.path(name)),
                           "mode": stat.S_IMODE(self.path(name).stat().st_mode)} for name in sorted(relatives)}

    def install_double(self, name, spec):
        if not name or not all(ch.isalnum() or ch in "-_" for ch in name):
            raise Refused("invalid executable double name")
        validate_schedule(spec)
        self.write(f"doubles/{name}.json", json.dumps(spec, sort_keys=True))
        code = ("#!/usr/bin/python3 -B\nimport sys\n"
                f"sys.path.insert(0, {str(self.workspace / 'tests/lib')!r})\n"
                "from flea_uish_fixtures import double_main\n"
                f"double_main({str(self.workspace)!r}, {str(self.root)!r}, {self.token!r}, {name!r})\n")
        return self.write(f"bin/{name}", code, executable=True)


def validate_schedule(spec):
    if not isinstance(spec, dict):
        raise Refused("double schedule must be an object")
    steps = spec.get("steps", [])
    if spec.get("mode") not in ("argv", "backend") or not isinstance(steps, list) or not 1 <= len(steps) <= 32:
        raise Refused("double requires argv/backend mode and 1..32 steps")
    if spec["mode"] == "argv" and len(steps) != 1:
        raise Refused("argv doubles have exactly one invocation")
    total = 0
    for step in steps:
        if not isinstance(step, dict):
            raise Refused("double step must be an object")
        delay = step.get("delay_ms", 0)
        if type(delay) is not int or not 0 <= delay <= 250:
            raise Refused("double delay must be 0..250 ms")
        total += delay
        expected = step.get("expect")
        if spec["mode"] == "backend" and (not isinstance(expected, dict) or "c" not in expected):
            raise Refused("backend expectations must name the command")
        if spec["mode"] == "argv" and (not isinstance(expected, list) or any(not isinstance(x, str) for x in expected)):
            raise Refused("argv expectations must be strings")
        if type(step.get("exit", 0)) is not int or not 0 <= step.get("exit", 0) <= 125:
            raise Refused("invalid double exit code")
        replies = step.get("replies", [])
        if not isinstance(replies, list) or len(replies) > 8 or any(not isinstance(reply, dict) or "t" not in reply for reply in replies):
            raise Refused("double replies must be at most eight protocol objects")
        if any(not isinstance(step.get(key, ""), str) for key in ("stdout", "stderr")):
            raise Refused("double output must be text")
    if total > 1000 or len(json.dumps(spec).encode()) > MAX_BYTES:
        raise Refused("double schedule exceeds its time or byte bound")


def double_main(workspace, root, token, name):
    """These are backend/provider fixtures, never GUI IPC readers or native evidence."""
    try:
        fixture = FixtureRoot(workspace, root, token)
        spec = json.loads(regular_bytes(fixture.path(f"doubles/{name}.json")))
        validate_schedule(spec)
        pending = bytearray()
        deadline = time.monotonic() + 5
        with selectors.DefaultSelector() as selector:
            if spec["mode"] == "backend":
                if sys.argv[1:] != ["--backend"]:
                    raise Refused("backend double requires --backend")
                selector.register(sys.stdin, selectors.EVENT_READ)
            for step in spec["steps"]:
                if spec["mode"] == "backend":
                    while b"\n" not in pending:
                        if not selector.select(max(0, deadline - time.monotonic())):
                            raise Refused("backend double input deadline")
                        chunk = os.read(sys.stdin.fileno(), 4096)
                        if not chunk or len(pending) + len(chunk) > MAX_BYTES:
                            raise Refused("backend double truncated or oversized input")
                        pending.extend(chunk)
                    line, _, rest = pending.partition(b"\n")
                    pending = bytearray(rest)
                    actual = json.loads(line)
                else:
                    actual = sys.argv[1:]
                if actual != step["expect"]:
                    raise Refused(f"unexpected {name} request: {actual!r}")
                time.sleep(step.get("delay_ms", 0) / 1000)
                for reply in step.get("replies", []):
                    print(json.dumps(reply), flush=True)
                sys.stdout.write(step.get("stdout", ""))
                sys.stdout.flush()
                sys.stderr.write(step.get("stderr", ""))
                if step.get("exit", 0):
                    raise SystemExit(step["exit"])
    except (Refused, OSError, ValueError) as error:
        print(f"FIXTURE_REFUSED: {error}", file=sys.stderr)
        raise SystemExit(64) from error


def smoke_fixture(fixture):
    """Use the existing PNG builder; keep the controller's Btrfs fixtures untouched."""
    from tui_battery import png

    palette = fixture.presentation()
    for name, content in (("listing/alpha.txt", "alpha\n"), ("listing/zeta.txt", "zeta\n"),
                          ("dirs/aaa/payload", "s"), ("dirs/zzz/payload", "z" * 257),
                          ("urls/a.url", "[InternetShortcut]\nURL=https://example.com/\n")):
        fixture.write(name, content)
    image = fixture.path("sources/image.png")
    image.parent.mkdir(mode=0o700)
    png.write(image, 64, 48, bytes((70, 130, 180, 255)) * (64 * 48))
    os.utime(image, (EPOCH, EPOCH))
    encoded = base64.b64encode(image.read_bytes()).decode()
    executable = fixture.write("bin/drive-url-thumbnail", "#!/usr/bin/python3 -B\nimport base64, sys\n"
                               f"sys.path.insert(0, {str(fixture.workspace / 'tests/lib')!r})\n"
                               "from flea_uish_fixtures import FixtureRoot, Refused\n"
                               f"f = FixtureRoot({str(fixture.workspace)!r}, {str(fixture.root)!r}, {fixture.token!r})\n"
                               "if len(sys.argv) != 4 or not sys.argv[3].isdigit(): raise Refused('thumbnail argv')\n"
                               "if not 1 <= int(sys.argv[3]) <= 4096: raise Refused('thumbnail size')\n"
                               "if sys.argv[1] != str(f.path('urls/a.url')): raise Refused('thumbnail input')\n"
                               "out = f.path('c#a?che/thumbnails/large')\n"
                               "from pathlib import Path\n"
                               "target = Path(sys.argv[2])\n"
                               "if target.parent != out or target.suffix != '.png': raise Refused('thumbnail output')\n"
                               "if target.exists() or target.is_symlink(): f.remove(str(target))\n"
                               f"f.write(target.relative_to(f.root), base64.b64decode({encoded!r}))\n", executable=True)
    fixture.write("data/thumbnailers/url.thumbnailer", "[Thumbnailer Entry]\n"
                  f"TryExec={executable}\nExec={executable} %i %o %s\nMimeType=application/x-mswinurl;\n")
    return palette


def require_decoder_fixture(fixture, relative="sources/image.png"):
    from tui_battery import png

    try:
        path = fixture.path(relative)
        regular_bytes(path)
        width, height, _ = png.read(path)
    except (OSError, ValueError, Refused) as error:
        raise FixturePremiseError(f"missing or invalid decoder fixture: {relative}") from error
    return {"path": relative, "sha256": digest(path), "width": width, "height": height}
