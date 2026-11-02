"""Own a test window, its private runtime, and every IPC call addressed to it."""
import json
import os
from pathlib import Path
import pwd
import signal
import socket
import stat
import struct
import subprocess
import sys
import tempfile
import time

from flea_transport_lifecycle import drain, identity, subreaper
from flea_native_inputs import seed, seed_fonts

REPO = Path(__file__).resolve().parents[2]
MARKER = ".flea-test-sandbox"


class TransportError(RuntimeError):
    pass


def drain_timeout():
    """Seconds an owned process group may take to drain on its own before teardown forces it (FLEA_TEST_DRAIN_TIMEOUT)."""
    value = float(os.environ.get("FLEA_TEST_DRAIN_TIMEOUT", "30"))
    if not 0 < value < float("inf"):
        raise TransportError("transport deadlines must be positive and finite")
    return value


def private(path):
    path = Path(path)
    info = path.lstat()
    if not path.is_absolute() or not stat.S_ISDIR(info.st_mode) or info.st_uid != os.geteuid() or info.st_mode & 0o077:
        raise TransportError(f"unsafe transport root: {path}")
    if not (path / MARKER).is_file():
        raise TransportError(f"unmarked transport root: {path}")
    return path


def owned(pid, record):
    for attempt in range(4):
        current = identity(pid)
        if current["group"] != record["group"] or current["session"] != record["group"]:
            raise TransportError(f"foreign process group for PID {pid}")
        if current["state"] == "Z":
            return current
        try:
            environment = Path(f"/proc/{pid}/environ").read_bytes().split(b"\0")
        except ProcessLookupError:
            # An exiting process answers ESRCH here; identity() raises FileNotFoundError once it is gone.
            current = identity(pid)
            if current["state"] == "Z":
                return current
            raise
        except PermissionError:
            if attempt < 3:
                time.sleep(0.05)
                continue
            if identity(pid)["state"] == "Z":
                return identity(pid)
            raise TransportError(f"unreadable ownership for PID {pid}")
        if f"FLEA_TEST_WINDOW={record['root']}".encode() in environment or descends(pid, record):
            return current
        if identity(pid)["state"] == "Z":
            return identity(pid)
        raise TransportError(f"foreign PID {pid}")
    raise TransportError(f"unreadable ownership for PID {pid}")


def descends(pid, record):
    # A helper that clears its environment (bwrap --clearenv) is still ours when its parents lead to this launch.
    roots = {(record["pid"], record["start"]), (record.get("supervisor"), record.get("supervisor_start"))}
    for _ in range(64):
        parent = identity(pid)["parent"]
        if parent <= 1:
            return False
        try:
            if (parent, identity(parent)["start"]) in roots:
                return True
        except FileNotFoundError:
            return False
        pid = parent
    return False


def load(root, exited=False):
    root = private(root)
    record = json.loads((root / "window.json").read_text())
    if record["root"] != str(root):
        raise TransportError("window record belongs to another root")
    leader = identity(record["group"])
    if leader["start"] != record["leader_start"]:
        raise TransportError("launch process group was reused")
    current = identity(record["pid"])
    if current["state"] == "Z" and not exited:
        raise TransportError("window has exited")
    if current["state"] != "Z":
        owned(record["pid"], record)
    if current["start"] != record["start"]:
        raise TransportError("window PID was reused")
    return record


def built_binary(name, override=None):
    target = Path(os.environ.get("CARGO_TARGET_DIR", REPO / "target")).resolve()
    path = Path(override).absolute() if override else target / "debug" / name
    if path.name != name or path.parent.name not in ("debug", "release") or path.parent.parent.resolve() != target:
        raise TransportError(f"{name} must come from this worktree's build: {path}")
    if path.resolve().parent != target / path.parent.name or path.is_symlink():
        raise TransportError(f"binary escapes this worktree's build: {path}")
    if not path.is_file() or not os.access(path, os.X_OK):
        raise TransportError(f"missing worktree binary: {path}")
    return str(path)


def sandbox_backend(override):
    path = Path(override)
    if not path.is_absolute():
        raise TransportError(f"stub backend must be an absolute path: {path}")
    try:
        info = path.lstat()
    except FileNotFoundError:
        raise TransportError(f"stub backend does not exist: {path}") from None
    if stat.S_ISLNK(info.st_mode):
        raise TransportError(f"stub backend is a symlink: {path}")
    resolved = path.resolve(strict=True)
    if not any((ancestor / MARKER).is_file() for ancestor in resolved.parents):
        raise TransportError(f"stub backend is outside a marked sandbox: {path}")
    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or not os.access(resolved, os.X_OK):
        raise TransportError(f"stub backend is not an owned executable file: {path}")
    homes = {Path(home).resolve() for home in (os.environ.get("HOME"), pwd.getpwuid(os.geteuid()).pw_dir) if home}
    # A planted marker never makes a home dot-directory runnable, except this worktree's own tree (T3 worktrees live under ~/.t3).
    for home in homes:
        inside_dot_dir = resolved.is_relative_to(home) and resolved.relative_to(home).parts[0].startswith(".")
        if inside_dot_dir and not resolved.is_relative_to(REPO):
            raise TransportError(f"stub backend is inside a home dot-directory: {path}")
    return str(resolved)


def endpoint(record, supplied=None, timeout=None, discovering=False):
    runtime = Path(record.get("runtime_dir") or Path(record["root"]) / "runtime")
    path = Path(record["socket"])
    if supplied is not None and str(path) != str(supplied):
        raise TransportError("wrong socket path")
    info = path.lstat()
    if not path.is_relative_to(runtime) or not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.geteuid():
        raise TransportError("foreign socket")
    if [info.st_dev, info.st_ino] != record["socket_identity"]:
        raise TransportError("socket was replaced")
    if path.is_symlink() or path.resolve() != path:
        raise TransportError("socket path contains a symlink")
    connection = socket.socket(socket.AF_UNIX)
    connection.settimeout(record["timeout"] if timeout is None else timeout)
    try:
        relative = path.relative_to(runtime)
        connection.connect(f"{record['runtime']}/{relative}")
        pid, uid, _ = struct.unpack("3i", connection.getsockopt(socket.SOL_SOCKET, socket.SO_PEERCRED, 12))
        if discovering and record["wrap"] and uid == os.geteuid():
            current = owned(pid, record)
            record.update(pid=pid, start=current["start"])
        if (pid, uid) != (record["pid"], os.geteuid()):
            raise TransportError(f"reply from another window: PID {pid}")
        return connection
    except BaseException:
        connection.close()
        raise


def exchange(record, arguments, timeout):
    deadline = time.monotonic() + timeout
    with endpoint(record, timeout=timeout) as connection:
        request = {"show": True} if arguments == ["show"] else {"call": arguments[1:]}
        connection.settimeout(max(0.001, deadline - time.monotonic()))
        connection.sendall((json.dumps(request) + "\n").encode())
        data = bytearray()
        while b"\n" not in data:
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError("IPC read timed out")
            connection.settimeout(remaining)
            chunk = connection.recv(65536)
            if not chunk:
                raise TransportError("incomplete IPC reply")
            data.extend(chunk)
            if len(data) > 16 * 1024 * 1024:
                raise TransportError("oversized IPC reply")
        reply = json.loads(bytes(data).split(b"\n", 1)[0])
        if type(reply.get("code")) is not int or reply["code"] not in (0, 2, 3, 255):
            raise TransportError("invalid IPC status")
        if not all(isinstance(reply.get(key), str) for key in ("stdout", "stderr")):
            raise TransportError("invalid IPC streams")
        return subprocess.CompletedProcess(arguments, reply["code"], reply["stdout"].encode(), reply["stderr"].encode())


def client_run(record, command, timeout=None):
    timeout = record["timeout"] if timeout is None else timeout
    environment = dict(os.environ, XDG_RUNTIME_DIR=record["runtime"])
    client = subprocess.Popen(command, env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                              start_new_session=True)
    try:
        stdout, stderr = client.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(client.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        client.communicate()
        raise
    return subprocess.CompletedProcess(command, client.returncode, stdout, stderr)


def qml_listing(record, flags):
    with endpoint(record):
        pass
    return client_run(record, ["qs", "list", *flags])


def qml_metadata(record):
    result = qml_listing(record, ["--all", "--json"])
    if result.returncode:
        raise TransportError(result.stderr.decode().strip() or "QML instance listing failed")
    rows = json.loads(result.stdout)
    if len(rows) != 1 or str(rows[0].get("pid")) != str(record["pid"]) or not isinstance(rows[0].get("id"), str):
        raise TransportError("QML instance metadata does not match the owned window")
    return rows[0]


class Window:
    def __init__(self, root):
        self.root = private(root)

    @classmethod
    def start(cls, path, *, parent=None, binary=None, backend=None, frontend=None, picker=False, ui=None, timeout=5, log=None, command=None,
              wrap=(), select=None, share=None):
        frontend = frontend or os.environ.get("FLEA_TEST_FRONTEND", "gpui")
        if frontend not in ("qml", "gpui"):
            raise TransportError(f"unknown test frontend: {frontend}")
        direct_qml = command and Path(command[0]).name in ("qs", "quickshell")
        binary = built_binary("flea", binary or (None if direct_qml else os.environ.get("FLEA_BIN")))
        if command and not direct_qml:
            if str(command[0]) != binary or len(command) != 3 or command[1] != "--gui":
                raise TransportError("unsupported owned launch command")
            path = command[2]
        if direct_qml and (len(command) != 3 or command[1] != "-p"):
            raise TransportError("unsupported QML launch command")
        if direct_qml and frontend == "gpui":
            raise TransportError("FLEA_IPC_NOT_PORTED qml.launch")
        if select is not None and not str(select):
            raise TransportError("select target is empty")
        # A peer window's runtime is shared the way two real windows share /run/user/UID, so their IPC reaches each other.
        if share is not None:
            share = private(Path(share).absolute())
            share = private(Path(json.loads((share / "options.json").read_text()).get("share") or share))
            private(share / "runtime")
        if direct_qml and select is not None:
            raise TransportError(f"a select target needs the flea --gui launch, not a direct qs command: {select}")
        gui = built_binary("flea-gui", os.environ.get("FLEA_GUI_BIN")) if frontend == "gpui" else None
        if backend is not None:
            backend = sandbox_backend(backend)
        elif direct_qml:
            backend = os.environ.get("FLEA_BIN", binary)
        else:
            backend = binary
        parent = Path(parent or REPO / ".superpowers/g010t")
        parent.mkdir(parents=True, exist_ok=True)
        root = Path(tempfile.mkdtemp(prefix="w.", dir=parent)).absolute()
        (root / MARKER).write_text("owned Flea window\n")
        options = dict(root=str(root), path=str(Path(path).absolute()), select=None if select is None else str(Path(select).absolute()),
                       binary=binary, gui=gui,
                       frontend=frontend, picker=picker, ui=str(Path(ui or REPO / "ui").absolute()), timeout=float(timeout),
                       log=str(log or root / "window.log"), command=command if direct_qml else None, wrap=list(wrap),
                       backend=backend, share=None if share is None else str(share),
                       owner=int(os.environ.get("FLEA_TEST_OWNER_PID", os.getpid())),
                       drain_timeout=drain_timeout())
        if not all(0 < options[key] < float("inf") for key in ("timeout", "drain_timeout")):
            raise TransportError("transport deadlines must be positive and finite")
        options["owner_start"] = identity(options["owner"])["start"]
        (root / "options.json").write_text(json.dumps(options))
        with (root / "supervisor.log").open("wb") as output:
            supervisor = subprocess.Popen([sys.executable, "-B", __file__, "_serve", str(root)], stdin=subprocess.DEVNULL,
                                          stdout=output, stderr=output, start_new_session=True)
        window = cls(root)
        deadline = time.monotonic() + timeout + options["drain_timeout"] + 7
        while True:
            if (root / "stopped").exists():
                outcome = json.loads((root / "outcome.json").read_text())
                supervisor.wait(timeout=5)
                raise TransportError(outcome.get("error") or "window launch was cancelled")
            if (root / "window.json").exists() and not (root / "error").exists():
                return window
            if time.monotonic() >= deadline:
                (root / "stop").touch()
                raise TransportError(f"window launch cleanup timed out: {root}")
            time.sleep(0.02)

    @property
    def pid(self):
        return self.record()["pid"]

    def record(self):
        return load(self.root)

    def call(self, *arguments, pid=None, socket_path=None, timeout=None, drive=False, selector=None):
        record = self.record()
        if pid is not None and str(pid) != str(record["pid"]):
            raise TransportError(f"foreign PID {pid}")
        timeout = record["timeout"] if timeout is None else float(timeout)
        arguments = list(map(str, arguments))
        if not arguments or (arguments != ["show"] and (arguments[0] != "call" or len(arguments) < 3)):
            raise TransportError("expected show or call target function")
        if socket_path is not None and str(socket_path) != record["socket"]:
            raise TransportError("wrong socket path")
        if record["frontend"] == "gpui":
            return exchange(record, arguments, timeout)
        with endpoint(record, socket_path, timeout):
            pass
        route = ["--pid", str(record["pid"])]
        if selector is not None:
            flag, value = selector
            if flag not in ("-i", "--id") or qml_metadata(record)["id"] != value:
                raise TransportError("foreign QML instance ID")
            route = list(selector)
        command = (["omarchy-drive", "ipc"] if drive else ["qs", "ipc"]) + route + (arguments[1:] if drive else arguments)
        return client_run(record, command, timeout)

    def wait_stopped(self, timeout):
        deadline = time.monotonic() + timeout
        while not (self.root / "stopped").exists():
            if time.monotonic() >= deadline:
                raise TransportError("owned process group did not drain")
            time.sleep(0.02)
        return json.loads((self.root / "outcome.json").read_text())

    def quit(self):
        record = load(self.root)
        if record["frontend"] == "qml":
            self.stop()
            return subprocess.CompletedProcess([], 0, b"", b"")
        target = "fleapicker" if record["picker"] else "flea"
        result = self.call("call", target, "quit")
        if (result.returncode, result.stdout, result.stderr) != (0, b"", b""):
            raise TransportError(f"quit was not acknowledged: {result}")
        (self.root / "quit").touch()
        outcome = self.wait_stopped(2 * record["drain_timeout"] + 7)
        if outcome.get("error") or outcome["forced"] or outcome["returncode"] != 0:
            raise TransportError(f"acknowledged quit did not exit naturally: {outcome}")
        return result

    def stop(self, *, pid=None, timeout=40):
        if pid is not None:
            source = self.root / ("window.json" if (self.root / "window.json").exists() else "launch.json")
            record = json.loads(source.read_text())
            if str(pid) != str(record["pid"]):
                raise TransportError(f"refusing teardown of foreign PID {pid}")
        if (self.root / "stopped").exists():
            return
        if (self.root / "window.json").exists():
            load(self.root, exited=True)
        (self.root / "stop").touch()
        self.wait_stopped(timeout)


def group_members(record):
    members = []
    for process in Path("/proc").iterdir():
        if not process.name.isdecimal():
            continue
        try:
            current = identity(process.name)
            if current["group"] == record["group"] and current["state"] != "Z":
                if owned(int(process.name), record)["state"] != "Z":
                    members.append(int(process.name))
        # ESRCH on its environ means the process exited between the two reads, the same as a vanished stat.
        except (FileNotFoundError, ProcessLookupError):
            continue
    return members


def serve(root):
    private(root)
    options = json.loads((root / "options.json").read_text())
    child = None
    record = None
    failure = None
    terminate = False
    outcome = dict(returncode=None, forced=False)
    try:
        subreaper()
        runtime_dir = root / "runtime"
        if options.get("share"):
            runtime_dir = private(Path(options["share"]) / "runtime")
        else:
            runtime = Path(tempfile.mkdtemp(prefix="runtime", dir=root))
            runtime.rename(runtime_dir)
            (runtime_dir / MARKER).touch()
            real_runtime = os.environ.get("XDG_RUNTIME_DIR")
            if real_runtime and (Path(real_runtime) / "hypr").is_dir():
                (runtime_dir / "hypr").symlink_to((Path(real_runtime) / "hypr").resolve(), target_is_directory=True)
        fd = os.open(runtime_dir, os.O_RDONLY | os.O_DIRECTORY)
        # Keep lstat's final component a directory while leaving room for Hyprland's event socket.
        alias = f"/proc/{os.getpid()}/fd/{fd}/."
        environment = dict(os.environ, XDG_RUNTIME_DIR=alias, FLEA_BIN=options["backend"],
                           FLEA_UI=options["ui"], FLEA_PATH=options["path"], FLEA_TEST_WINDOW=str(root),
                           FLEA_FRONTEND=options["frontend"])
        display = os.environ.get("WAYLAND_DISPLAY", "")
        if display and not Path(display).is_absolute() and os.environ.get("XDG_RUNTIME_DIR"):
            environment["WAYLAND_DISPLAY"] = str(Path(os.environ["XDG_RUNTIME_DIR"]) / display)
        for key, name in (("HOME", "home"), ("XDG_CONFIG_HOME", "config"), ("XDG_STATE_HOME", "state"),
                          ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache")):
            directory = root / name
            directory.mkdir()
            existing = Path(environment.get(key, "/"))
            safe = existing.is_absolute() and any((ancestor / MARKER).is_file() for ancestor in (existing, *existing.parents))
            if not safe:
                environment[key] = str(directory)
        if environment["HOME"] == str(root / "home"):
            config = root / "config" if environment["XDG_CONFIG_HOME"] == str(root / "config") else None
            seed(os.environ.get("HOME", ""), root / "home", config)
        elif environment["XDG_CONFIG_HOME"] == str(root / "config"):
            seed_fonts(Path(environment["HOME"]), private_config=root / "config")
        command = [options["binary"], "--gui"]
        # A reveal hands the launcher the target, the way `flea --gui --select <target> <dir>` does.
        if options["select"]:
            command += ["--select", options["select"]]
        command.append(options["path"])
        if options["command"]:
            command = options["command"]
        if options["picker"]:
            environment["FLEA_PICKER_REPLY"] = str(root / "picker-reply")
            environment["FLEA_PICKER"] = json.dumps({"folder": options["path"]})
            if not options["command"]:
                command = [options["binary"], "--pick", str(root / "picker-reply")]
        with open(options["log"], "ab") as output:
            child = subprocess.Popen(options["wrap"] + command, env=environment, stdin=subprocess.DEVNULL, stdout=output, stderr=output,
                                     start_new_session=True)
        current = identity(child.pid)
        record = dict(options, pid=child.pid, start=current["start"], runtime=alias, runtime_dir=str(runtime_dir),
                      group=child.pid, leader_start=current["start"],
                      supervisor=os.getpid(), supervisor_start=identity(os.getpid())["start"])
        (root / "launch.json").write_text(json.dumps(record))
        deadline = time.monotonic() + options["timeout"]
        refused = None
        while True:
            if (root / "stop").exists():
                break
            if identity(child.pid)["state"] == "Z":
                raise TransportError("window exited before IPC")
            candidates = list(runtime_dir.rglob("*"))
            for candidate in candidates:
                try:
                    info = candidate.lstat()
                except FileNotFoundError:
                    continue
                if stat.S_ISSOCK(info.st_mode):
                    record.update(socket=str(candidate), socket_identity=[info.st_dev, info.st_ino])
                    try:
                        with endpoint(record, discovering=True):
                            pass
                    except (ConnectionRefusedError, FileNotFoundError):
                        continue
                    except TransportError as error:
                        if not options.get("share"):
                            raise
                        refused = error
                        continue
                    (root / "window.pending").write_text(json.dumps(record))
                    (root / "window.pending").rename(root / "window.json")
                    break
            else:
                if time.monotonic() >= deadline:
                    raise TransportError("window socket wait timed out" + (f": {refused}" if refused else ""))
                time.sleep(0.02)
                continue
            break
        while identity(child.pid)["state"] != "Z":
            if (root / "stop").exists():
                terminate = True
                break
            if (root / "quit").exists():
                break
            try:
                owner = identity(options["owner"])
                if owner["start"] != options["owner_start"] or owner["state"] == "Z":
                    terminate = True
                    break
            except FileNotFoundError:
                terminate = True
                break
            time.sleep(0.02)
    except BaseException as error:
        failure = str(error)
        terminate = True
        (root / "error").write_text(failure)
    finally:
        if child is not None and record is not None:
            try:
                def on_forcing():
                    nonlocal failure
                    failure = failure or "natural exit exceeded its deadline"
                    (root / "error").write_text(failure)
                outcome = drain(child, record, lambda: group_members(record), lambda pid: owned(pid, record),
                                terminate=terminate, grace=options["drain_timeout"], on_forcing=on_forcing, whole_group=failure is not None)
                if not terminate and (outcome["forced"] or outcome["returncode"] != 0):
                    failure = failure or "natural exit failed or required forced termination"
            except BaseException as error:
                failure = failure or f"teardown failed: {error}"
                # An unwritable report must not skip the kill below; the final report writes it again and fails loud.
                try:
                    (root / "error").write_text(failure)
                except OSError:
                    pass
                outcome = dict(returncode=None, forced=True)
                # A failed drain still ends the window's own group while its leader is the one launched, then reports stopped.
                try:
                    if identity(record["group"])["start"] == record["leader_start"]:
                        os.killpg(record["group"], signal.SIGKILL)
                    outcome = dict(returncode=child.wait(timeout=options["drain_timeout"]), forced=True)
                except BaseException:
                    outcome = dict(returncode=None, forced=True)
        if outcome is not None:
            outcome["error"] = failure
            if failure:
                (root / "error").write_text(failure)
            (root / "outcome.pending").write_text(json.dumps(outcome))
            (root / "outcome.pending").rename(root / "outcome.json")
            (root / "stopped").touch()


if __name__ == "__main__":
    if sys.argv[1:2] == ["_serve"]:
        serve(Path(sys.argv[2]))
